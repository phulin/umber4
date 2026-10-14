use crate::{
    Engine, Error, Outcome, Result, condition::Condition, scalars::Target, token::*, values,
};
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
    consumers: Vec<Consumer>,
    definition: Option<DefinitionBuilder>,
    output: Vec<u8>,
    prefix: u8,
    ended: bool,
    depth: usize,
    stats: Stats,
}
enum Action {
    Continue,
    Replay,
    Call(values::Owner),
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
            consumers: vec![Consumer::Execute],
            definition: None,
            output: Vec::new(),
            prefix: 0,
            ended: false,
            depth: 0,
            stats: Stats {
                consumer_record_bytes: std::mem::size_of::<Consumer>(),
                ..Stats::default()
            },
        }
    }
    fn push(&mut self, consumer: Consumer) -> Result<()> {
        if self.consumers.len() == 256 {
            return Err(Error::Capacity);
        }
        self.consumers.push(consumer);
        if TRACE {
            self.stats.max_consumer_depth = self.stats.max_consumer_depth.max(self.consumers.len());
        }
        Ok(())
    }
    fn run(mut self) -> Result<(Outcome, Stats)> {
        while !self.ended {
            if let Some(Consumer::Integer(number, _)) = self.consumers.last_mut() {
                while let Some(digit) = self
                    .engine
                    .file
                    .literal_digit(self.engine.scalars.catcodes())
                {
                    number.push(digit)?;
                }
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
            if let Action::Call(owner) = self.feed(token)? {
                self.produce(owner)?;
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
    fn produce(&mut self, owner: values::Owner) -> Result<()> {
        if self.depth == 128 {
            return Err(Error::Capacity);
        }
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
                    if let Some(Consumer::Integer(number, _)) = self.consumers.last_mut() {
                        while next != end {
                            // SAFETY: the current body owner retains this window;
                            // batching calls no consumer and only copies words.
                            let digit = Engine::digit(unsafe { next.read() }, 10);
                            if digit >= 10 {
                                break;
                            }
                            next = unsafe { next.add(1) };
                            position += 1;
                            number.push(digit)?;
                        }
                        if next == end {
                            break;
                        }
                    }
                    // SAFETY: owner retains the immutable initialized window.
                    // Child producers cannot mutate its value or its cells.
                    let token = unsafe { next.read() };
                    next = unsafe { next.add(1) };
                    position += 1;
                    if let Action::Call(child) = self.feed(token)? {
                        if position == range.end {
                            if TRACE {
                                self.stats.tail_calls += 1;
                            }
                            // No pointer into the old body is used after this
                            // transfer. The child already owns its definition.
                            owner = child;
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
    fn feed(&mut self, token: Token) -> Result<Action> {
        loop {
            let read = Read {
                token,
                meaning: self.engine.resolve(token),
            };
            let mode = *self.consumers.last().unwrap();
            if !mode.raw() {
                if read.meaning.is_macro() {
                    let owner = self.engine.values.get(read.meaning.value as u32);
                    let def = owner.definition();
                    if def.prefix_end != 0 || !def.parameters.is_empty() {
                        return Err(Error::Unsupported);
                    }
                    if TRACE {
                        self.stats.macro_calls += 1;
                    }
                    return Ok(Action::Call(owner));
                }
                match read.meaning.command {
                    cmd::UNDEFINED => return Err(Error::Undefined),
                    cmd::NUMBER => {
                        self.integer(Finish::Emit)?;
                        return Ok(Action::Continue);
                    }
                    cmd::THE => {
                        self.push(Consumer::Quantity)?;
                        return Ok(Action::Continue);
                    }
                    cmd::IFTRUE | cmd::IFFALSE | cmd::IFNUM => {
                        let condition = self.engine.conditions.len();
                        self.engine.conditions.push(Condition::Pending);
                        if read.meaning.command == cmd::IFNUM {
                            self.integer(Finish::CompareLeft(condition))?;
                        } else {
                            self.choose(condition, read.meaning.command == cmd::IFTRUE)?;
                        }
                        return Ok(Action::Continue);
                    }
                    cmd::ELSE | cmd::FI => {
                        self.boundary(read.meaning.command)?;
                        return Ok(Action::Continue);
                    }
                    _ if read.meaning.expandable() => return Err(Error::Unsupported),
                    _ => {}
                }
            }
            match self.consume(read)? {
                Action::Replay => continue,
                action => return Ok(action),
            }
        }
    }
    fn consume(&mut self, read: Read) -> Result<Action> {
        match *self.consumers.last().unwrap() {
            Consumer::Execute => self.execute(read)?,
            Consumer::DefTarget(global) => {
                if read.token.is_space() {
                    return Ok(Action::Continue);
                }
                if !read.token.is_cs() {
                    return Err(Error::Syntax);
                }
                self.start_definition(read.token.id(), global)?;
                *self.consumers.last_mut().unwrap() = Consumer::DefOpen;
            }
            Consumer::DefOpen => {
                if !read.token.is_begin() {
                    return Err(Error::Unsupported);
                }
                self.definition
                    .as_mut()
                    .unwrap()
                    .tokens
                    .push(Token(PARAM | 255))?;
                *self.consumers.last_mut().unwrap() = Consumer::DefBody(1);
            }
            Consumer::DefBody(mut depth) => {
                if read.token.cat() == 6 && !read.token.is_cs() {
                    return Err(Error::Unsupported);
                }
                if read.token.is_begin() {
                    depth += 1;
                }
                if read.token.is_end() {
                    depth -= 1;
                }
                if depth == 0 {
                    self.finish_definition();
                } else {
                    self.definition.as_mut().unwrap().tokens.push(read.token)?;
                    *self.consumers.last_mut().unwrap() = Consumer::DefBody(depth);
                }
            }
            Consumer::Message(0) => {
                if matches!(read.meaning.command, cmd::SPACE | cmd::RELAX) {
                    return Ok(Action::Continue);
                }
                if read.meaning.command != cmd::BEGIN {
                    return Err(Error::Syntax);
                }
                *self.consumers.last_mut().unwrap() = Consumer::Message(1);
            }
            Consumer::Message(mut depth) => {
                if read.meaning.command == cmd::BEGIN {
                    depth += 1;
                }
                if read.meaning.command == cmd::END_GROUP_CHAR {
                    depth -= 1;
                }
                if depth == 0 {
                    self.consumers.pop();
                    self.engine.messages.push(std::mem::take(&mut self.output));
                } else {
                    self.engine.print_token(read.token, &mut self.output);
                    *self.consumers.last_mut().unwrap() = Consumer::Message(depth);
                }
            }
            Consumer::Integer(number, finish) => return self.number(read, number, finish),
            Consumer::Equals(finish) => {
                if read.meaning.command == cmd::SPACE {
                    return Ok(Action::Continue);
                }
                *self.consumers.last_mut().unwrap() = Consumer::Integer(Number::default(), finish);
                if read.token != Token::other(b'=') {
                    return Ok(Action::Replay);
                }
            }
            Consumer::Arithmetic(operation, global) => {
                if read.meaning.command == cmd::SPACE {
                    return Ok(Action::Continue);
                }
                if read.meaning.command != cmd::COUNT {
                    return Err(Error::Unsupported);
                }
                self.consumers.pop();
                self.integer(Finish::Target(TargetKind::Count, global, operation))?;
            }
            Consumer::Keyword(matched, finish) => {
                if matched == 0 && read.meaning.command == cmd::SPACE {
                    return Ok(Action::Continue);
                }
                if !read.token.is_cs()
                    && read.token.byte().to_ascii_lowercase() == b"by"[matched as usize]
                {
                    *self.consumers.last_mut().unwrap() = if matched == 0 {
                        Consumer::Keyword(1, finish)
                    } else {
                        Consumer::Integer(Number::default(), finish)
                    };
                } else if matched == 0 {
                    *self.consumers.last_mut().unwrap() =
                        Consumer::Integer(Number::default(), finish);
                    return Ok(Action::Replay);
                } else {
                    return Err(Error::Unsupported);
                }
            }
            Consumer::Quantity => {
                if read.meaning.command != cmd::COUNT {
                    return Err(Error::Unsupported);
                }
                self.consumers.pop();
                self.integer(Finish::EmitCount)?;
            }
            Consumer::Relation(left, condition) => {
                if read.meaning.command == cmd::SPACE {
                    return Ok(Action::Continue);
                }
                if ![Token::other(b'='), Token::other(b'<'), Token::other(b'>')]
                    .contains(&read.token)
                {
                    return Err(Error::Syntax);
                }
                self.consumers.pop();
                self.integer(Finish::CompareRight(condition, left, read.token.byte()))?;
            }
            Consumer::LetTarget(global) => {
                if read.token.is_space() {
                    return Ok(Action::Continue);
                }
                if !read.token.is_cs() {
                    return Err(Error::Syntax);
                }
                *self.consumers.last_mut().unwrap() =
                    Consumer::LetValue(read.token.id(), global, 0);
            }
            Consumer::LetValue(target, global, phase) => {
                if phase == 0 && read.token.is_space() {
                    return Ok(Action::Continue);
                }
                if phase == 0 && read.token == Token::other(b'=') {
                    *self.consumers.last_mut().unwrap() = Consumer::LetValue(target, global, 1);
                } else if phase == 1 && read.token.is_space() {
                    *self.consumers.last_mut().unwrap() = Consumer::LetValue(target, global, 2);
                } else {
                    self.consumers.pop();
                    self.engine.assign_meaning(target, read.meaning, global);
                }
            }
            Consumer::Skip(mut nested, to_fi) => {
                if read.meaning.is_conditional() {
                    nested += 1;
                }
                if read.meaning.command == cmd::FI {
                    if nested == 0 {
                        self.consumers.pop();
                        self.engine.conditions.pop();
                        return Ok(Action::Continue);
                    }
                    nested -= 1;
                } else if read.meaning.command == cmd::ELSE && nested == 0 && !to_fi {
                    self.consumers.pop();
                    *self.engine.conditions.last_mut().unwrap() = Condition::Else;
                    return Ok(Action::Continue);
                }
                *self.consumers.last_mut().unwrap() = Consumer::Skip(nested, to_fi);
            }
        }
        Ok(Action::Continue)
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
        self.consumers.pop();
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
            cmd::DEF | cmd::GDEF => {
                self.push(Consumer::DefTarget(global || command == cmd::GDEF))?
            }
            cmd::LET => self.push(Consumer::LetTarget(global))?,
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
            cmd::MESSAGE if !global => self.push(Consumer::Message(0))?,
            cmd::BEGIN | cmd::END_GROUP_CHAR | cmd::BEGIN_GROUP | cmd::END_GROUP if !global => {
                self.engine.execute(read)?
            }
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
