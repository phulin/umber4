use crate::{
    Engine, Error, Outcome, Result, condition::Condition, scalars::Target, token::*, values,
};
#[path = "producer_execute.rs"]
mod execute;
#[path = "producer_input.rs"]
mod input;
#[path = "producer_scan.rs"]
mod scan;

#[derive(Default, Debug)]
pub struct Stats {
    pub pending_record_bytes: usize,
    pub macro_calls: u64,
    pub tail_calls: u64,
    pub max_native_depth: usize,
    pub max_pending_sources: usize,
    pub max_integer_depth: usize,
}
struct Body {
    owner: values::Owner,
    next: *const Token,
    end: *const Token,
    window_end: u32,
    limit: u32,
}
enum Source {
    Body(Body),
    Digits(u64),
    Token(Token),
}
struct Pending {
    scope: usize,
    source: Source,
}
struct Frame {
    body: Option<Body>,
    back: Option<Token>,
    parent: *mut Frame,
}
struct Producer<'a, const TRACE: bool> {
    engine: Engine<'a>,
    pending: Vec<Pending>,
    ended: bool,
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
            pending: Vec::new(),
            ended: false,
            stats: Stats {
                pending_record_bytes: std::mem::size_of::<Pending>(),
                ..Stats::default()
            },
        }
    }
    fn run(mut self) -> Result<(Outcome, Stats)> {
        let mut input = Frame {
            body: None,
            back: None,
            parent: std::ptr::null_mut(),
        };
        self.execute_input::<false>(&mut input, 0)?;
        debug_assert!(self.engine.sources.is_empty());
        Ok((
            Outcome {
                counts: self.engine.scalars.counts().map(|v| v.value),
                messages: self.engine.messages,
            },
            self.stats,
        ))
    }
    #[inline(never)]
    fn produce(&mut self, id: u32, parent: &mut Frame, depth: u16) -> Result<()> {
        if depth == 128 {
            return Err(Error::Capacity);
        }
        let mut input = Frame {
            body: Some(Body::new(self.engine.values.get(id))),
            back: None,
            parent,
        };
        if TRACE {
            self.stats.max_native_depth = self.stats.max_native_depth.max(usize::from(depth) + 1);
        }
        self.execute_input::<true>(&mut input, depth + 1)
    }
    #[inline(always)]
    fn execute_input<const BODY: bool>(&mut self, input: &mut Frame, depth: u16) -> Result<()> {
        while !self.ended {
            let read = if BODY && input.back.is_none() && !self.owns_pending(input) {
                let Some(read) = self.body_tokens(input, depth)? else {
                    break;
                };
                read
            } else {
                if input.back.is_none()
                    && self.owns_pending(input)
                    && let Source::Body(body) = &mut self.pending.last_mut().unwrap().source
                {
                    body.skip_inert(|token| self.engine.resolve(token));
                }
                let Some(token) = self.local(input)? else {
                    break;
                };
                Read {
                    token,
                    meaning: self.engine.resolve(token),
                }
            };
            if read.meaning.is_macro() {
                let id = read.meaning.value as u32;
                if TRACE {
                    self.stats.macro_calls += 1;
                }
                if !self.tail(input, id) {
                    self.produce(id, input, depth)?;
                }
            } else if read.meaning.expandable() {
                self.expand(input, read, 0)?;
            } else {
                self.execute(input, read)?;
            }
        }
        Ok(())
    }
    #[inline(always)]
    fn body_tokens(&mut self, input: &mut Frame, depth: u16) -> Result<Option<Read>> {
        let body = input.body.as_ref().unwrap();
        let (mut next, mut end) = (body.next, body.end);
        loop {
            if next == end {
                let body = input.body.as_mut().unwrap();
                body.next = next;
                if !body.refill() {
                    return Ok(None);
                }
                (next, end) = (body.next, body.end);
            }
            // SAFETY: the frame owns this initialized immutable token window.
            // Its cursor is published before any child or scanner can read it.
            let token = unsafe { next.read() };
            next = unsafe { next.add(1) };
            let read = Read {
                token,
                meaning: self.engine.resolve(token),
            };
            if read.meaning.is_macro() {
                let id = read.meaning.value as u32;
                if TRACE {
                    self.stats.macro_calls += 1;
                }
                let body = input.body.as_mut().unwrap();
                body.next = next;
                if next == end && body.window_end == body.limit {
                    if body.owner.id == id {
                        body.restart();
                    } else {
                        *body = Body::new(self.engine.values.get(id));
                    }
                    if TRACE {
                        self.stats.tail_calls += 1;
                    }
                } else {
                    self.produce(id, input, depth)?;
                    if self.ended {
                        return Ok(None);
                    }
                }
                // A child can consume its caller's body, but creates pending
                // input only in its own scope and drains it before returning.
                let body = input.body.as_ref().unwrap();
                (next, end) = (body.next, body.end);
            } else if !matches!(read.meaning.command, cmd::RELAX | cmd::SPACE | cmd::PAR) {
                input.body.as_mut().unwrap().next = next;
                return Ok(Some(read));
            }
        }
    }
    #[inline(always)]
    fn tail(&mut self, input: &mut Frame, id: u32) -> bool {
        if input.back.is_some() {
            return false;
        }
        let body = if self.owns_pending(input) {
            let Some(Pending {
                source: Source::Body(body),
                ..
            }) = self.pending.last_mut()
            else {
                return false;
            };
            body
        } else if let Some(body) = &mut input.body {
            body
        } else {
            return false;
        };
        if body.next != body.end || body.window_end != body.limit {
            return false;
        }
        if body.owner.id != id {
            *body = Body::new(self.engine.values.get(id));
        } else {
            body.restart();
        }
        if TRACE {
            self.stats.tail_calls += 1;
        }
        true
    }
}
#[cfg(test)]
#[path = "producer_tests.rs"]
mod tests;
