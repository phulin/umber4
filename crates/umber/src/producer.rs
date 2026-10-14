use crate::{
    Engine, Error, Outcome, Result, condition::Condition, scalars::Target, token::*, values,
};
#[path = "producer_consume.rs"]
mod consume;
#[path = "producer_scan.rs"]
mod scan;
use scan::{Consumer, Finish, Number, TargetKind};

#[derive(Default, Debug)]
pub struct Stats {
    pub consumer_record_bytes: usize,
    pub macro_calls: u64,
    pub tail_calls: u64,
    pub max_producer_depth: usize,
    pub max_consumer_depth: usize,
}
struct DefinitionBuilder {
    target: usize,
    global: bool,
    tokens: values::Builder,
}
struct Producer<'a, const TRACE: bool> {
    engine: Engine<'a>,
    consumer: Consumer,
    suspended: Vec<Consumer>,
    definition: Option<DefinitionBuilder>,
    output: Vec<u8>,
    prefix: u8,
    ended: bool,
    depth: usize,
    stats: Stats,
}
impl Engine<'_> {
    pub fn run_producer(self) -> Result<Outcome> {
        Producer::<false>::new(self)
            .run()
            .map(|(outcome, _)| outcome)
    }
    pub fn run_producer_diagnostic(self) -> Result<(Outcome, Stats)> {
        Producer::<true>::new(self).run()
    }
}
impl<'a, const TRACE: bool> Producer<'a, TRACE> {
    fn new(engine: Engine<'a>) -> Self {
        Self {
            engine,
            consumer: Consumer::Execute,
            suspended: Vec::new(),
            definition: None,
            output: Vec::new(),
            prefix: 0,
            ended: false,
            depth: 0,
            stats: Stats {
                consumer_record_bytes: std::mem::size_of::<Consumer>(),
                max_consumer_depth: 1,
                ..Stats::default()
            },
        }
    }
    fn push(&mut self, consumer: Consumer) -> Result<()> {
        if self.suspended.len() == 255 {
            return Err(Error::Capacity);
        }
        self.suspended
            .push(std::mem::replace(&mut self.consumer, consumer));
        if TRACE {
            self.stats.max_consumer_depth =
                self.stats.max_consumer_depth.max(self.suspended.len() + 1);
        }
        Ok(())
    }
    fn pop(&mut self) -> Consumer {
        std::mem::replace(&mut self.consumer, self.suspended.pop().unwrap())
    }
    fn run(mut self) -> Result<(Outcome, Stats)> {
        while !self.ended {
            if let Consumer::Integer { value, digits, .. } = &mut self.consumer {
                let mut accumulated = *value;
                let mut found = false;
                while let Some(digit) = self
                    .engine
                    .file
                    .literal_digit(self.engine.scalars.catcodes())
                {
                    accumulated = Number::append(accumulated, digit)?;
                    found = true;
                }
                *value = accumulated;
                *digits |= found;
            }
            let token = self.engine.file.read(
                self.engine.scalars.catcodes(),
                self.engine.scalars[Target::ENDLINE].value,
                &mut self.engine.names,
                self.engine.par,
            )?;
            if self.engine.names.len() > self.engine.bindings.len() {
                self.engine.grow_bindings(self.engine.names.len());
            }
            if let Some(id) = self.feed(token)? {
                self.produce(id)?;
            }
        }
        debug_assert!(self.engine.sources.is_empty());
        Ok((
            Outcome {
                counts: self.engine.scalars.counts().map(|v| v.value),
                messages: self.engine.messages,
            },
            self.stats,
        ))
    }
    // One Rust frame owns the current immutable body. Only non-tail body calls
    // recurse; source exhaustion returns naturally to the caller's producer.
    #[inline(never)]
    fn produce(&mut self, id: u32) -> Result<()> {
        if self.depth == 128 {
            return Err(Error::Capacity);
        }
        let owner = self.engine.values.get(id);
        self.depth += 1;
        if TRACE {
            self.stats.max_producer_depth = self.stats.max_producer_depth.max(self.depth);
        }
        let result = self.produce_inner(owner);
        self.depth -= 1;
        result
    }
    fn produce_inner(&mut self, mut owner: values::Owner) -> Result<()> {
        'body: loop {
            let range = owner.definition().body;
            let mut position = range.start;
            while position < range.end && !self.ended {
                let (mut next, end, window_end) = owner.tokens.window(position, range.end);
                while next != end && !self.ended {
                    if let Consumer::Integer { value, digits, .. } = &mut self.consumer {
                        let mut accumulated = *value;
                        let mut found = false;
                        while next != end {
                            // SAFETY: the current body owner retains this window;
                            // batching calls no consumer and only copies words.
                            let digit = Engine::digit(unsafe { next.read() }, 10);
                            if digit >= 10 {
                                break;
                            }
                            next = unsafe { next.add(1) };
                            position += 1;
                            accumulated = Number::append(accumulated, digit)?;
                            found = true;
                        }
                        *value = accumulated;
                        *digits |= found;
                        if next == end {
                            break;
                        }
                    }
                    // SAFETY: owner retains the immutable initialized window.
                    // Child producers cannot mutate its value or its cells.
                    let token = unsafe { next.read() };
                    next = unsafe { next.add(1) };
                    position += 1;
                    let read = Read {
                        token,
                        meaning: self.engine.resolve(token),
                    };
                    if matches!(self.consumer, Consumer::Execute)
                        && matches!(
                            read.meaning.command,
                            cmd::COUNT | cmd::CATCODE | cmd::ENDLINE | cmd::ARITH
                        )
                    {
                        // SAFETY: the current body owner retains these immutable
                        // initialized words throughout recognition and assignment.
                        let words = unsafe {
                            std::slice::from_raw_parts(next, end.offset_from(next) as usize)
                        };
                        let consumed = self.scalar_window(read, words);
                        if consumed != 0 {
                            // SAFETY: recognition consumes only this window.
                            next = unsafe { next.add(consumed) };
                            position += consumed as u32;
                            continue;
                        }
                    }
                    if let Some(child) = self.deliver(read)? {
                        if position == range.end {
                            if TRACE {
                                self.stats.tail_calls += 1;
                            }
                            // Acquire a different body before releasing the old
                            // owner. A self tail call retains the existing owner;
                            // its value ID stays reserved while that owner lives.
                            if owner.id != child {
                                owner = self.engine.values.get(child);
                            }
                            continue 'body;
                        }
                        self.produce(child)?;
                    }
                }
                if !self.ended {
                    debug_assert_eq!(position, window_end);
                }
            }
            return Ok(());
        }
    }
    #[inline(always)]
    fn feed(&mut self, token: Token) -> Result<Option<u32>> {
        self.deliver(Read {
            token,
            meaning: self.engine.resolve(token),
        })
    }
    #[inline(always)]
    fn deliver(&mut self, mut read: Read) -> Result<Option<u32>> {
        loop {
            if read.meaning.expandable() && !self.consumer.raw() {
                if read.meaning.is_macro() {
                    // Definitions are validated when constructed. Aliases
                    // preserve that invariant; tail calls can retain the owner.
                    if TRACE {
                        self.stats.macro_calls += 1;
                    }
                    return Ok(Some(read.meaning.value as u32));
                }
                self.expand_read(read)?;
                return Ok(None);
            }
            if !self.consume(read)? {
                return Ok(None);
            }
            read.meaning = self.engine.resolve(read.token);
        }
    }
    #[inline(never)]
    fn expand_read(&mut self, read: Read) -> Result<()> {
        match read.meaning.command {
            cmd::UNDEFINED => return Err(Error::Undefined),
            cmd::NUMBER => self.integer(Finish::Emit)?,
            cmd::THE => self.push(Consumer::Quantity)?,
            cmd::IFTRUE | cmd::IFFALSE | cmd::IFNUM => {
                let condition = self.engine.conditions.len();
                self.engine.conditions.push(Condition::Pending);
                if read.meaning.command == cmd::IFNUM {
                    self.integer(Finish::CompareLeft(condition))?;
                } else {
                    self.choose(condition, read.meaning.command == cmd::IFTRUE)?;
                }
            }
            cmd::ELSE | cmd::FI => self.boundary(read.meaning.command)?,
            _ => return Err(Error::Unsupported),
        }
        Ok(())
    }
    #[inline(never)]
    fn start_definition(&mut self, target: usize, global: bool) -> Result<()> {
        self.definition = Some(DefinitionBuilder {
            target,
            global,
            tokens: self.engine.values.builder()?,
        });
        Ok(())
    }
    #[inline(never)]
    fn finish_definition(&mut self) {
        self.pop();
        let definition = self.definition.take().unwrap();
        let end = definition.tokens.len();
        let owner = definition.tokens.finish(Some(Definition {
            prefix_end: 0,
            body: ArenaRange { start: 1, end },
            parameters: Box::new([]),
            long: false,
            uses_parameters: false,
        }));
        self.engine.assign_meaning(
            definition.target,
            Meaning {
                command: cmd::MACRO,
                value: owner.id as i32,
            },
            definition.global,
        );
    }
    #[inline(always)]
    fn execute(&mut self, read: Read) -> Result<()> {
        let command = read.meaning.command;
        match command {
            cmd::GLOBAL => {
                self.prefix |= 1;
                return Ok(());
            }
            cmd::LONG => {
                self.prefix |= 2;
                return Ok(());
            }
            cmd::RELAX | cmd::SPACE | cmd::PAR => return Ok(()),
            _ => {}
        }
        let prefix = std::mem::take(&mut self.prefix);
        let global = prefix & 1 != 0;
        if prefix & 2 != 0 && !matches!(command, cmd::DEF | cmd::GDEF) {
            return Err(Error::Syntax);
        }
        match command {
            cmd::COUNT | cmd::CATCODE => self.integer(Finish::Target(
                if command == cmd::COUNT {
                    TargetKind::Count
                } else {
                    TargetKind::Catcode
                },
                global,
                -1,
            ))?,
            cmd::ENDLINE => self.push(Consumer::Equals(Finish::Assign(
                Target::ENDLINE,
                global,
                -1,
                false,
            )))?,
            cmd::ARITH => self.push(Consumer::Arithmetic(read.meaning.value, global))?,
            cmd::BEGIN | cmd::BEGIN_GROUP if !global => {
                self.engine.level = self.engine.level.checked_add(1).ok_or(Error::Capacity)?;
                self.engine
                    .save
                    .push(crate::execute::Save::Boundary(command == cmd::BEGIN_GROUP));
            }
            cmd::END_GROUP_CHAR | cmd::END_GROUP if !global => {
                self.engine.close_group(command == cmd::END_GROUP)?;
            }
            _ => return self.execute_slow(read, global),
        }
        Ok(())
    }
    #[inline(never)]
    fn execute_slow(&mut self, read: Read, global: bool) -> Result<()> {
        let command = read.meaning.command;
        match command {
            cmd::DEF | cmd::GDEF => {
                self.push(Consumer::DefTarget(global || command == cmd::GDEF))?
            }
            cmd::LET => self.push(Consumer::LetTarget(global))?,
            cmd::MESSAGE if !global => self.push(Consumer::Message(0))?,
            cmd::END if !global => {
                if self.engine.level != 0 {
                    return Err(Error::Group);
                }
                if !self.engine.conditions.is_empty() {
                    return Err(Error::Conditional);
                }
                self.ended = true;
            }
            _ => return Err(Error::Unsupported),
        }
        Ok(())
    }
    fn choose(&mut self, condition: usize, yes: bool) -> Result<()> {
        if yes {
            self.engine.conditions[condition] = Condition::Then { case: false };
        } else {
            self.push(Consumer::Skip(0, false))?;
        }
        Ok(())
    }
    fn boundary(&mut self, command: u8) -> Result<()> {
        let condition = self.engine.conditions.last().ok_or(Error::Conditional)?;
        if command == cmd::FI {
            self.engine.conditions.pop();
        } else if matches!(condition, Condition::Then { .. }) {
            self.push(Consumer::Skip(0, true))?;
        } else {
            return Err(Error::Conditional);
        }
        Ok(())
    }
}
#[cfg(test)]
#[path = "producer_tests.rs"]
mod tests;
