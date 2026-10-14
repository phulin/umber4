use super::*;
#[derive(Clone, Copy)]
pub(super) enum TargetKind {
    Count,
    Catcode,
}
#[derive(Clone, Copy)]
pub(super) enum Finish {
    Target(TargetKind, bool, i32),
    Assign(Target, bool, i32, bool),
    Emit,
    EmitCount,
    InternalCount,
    CompareLeft(usize),
    CompareRight(usize, i32, u8),
}
#[derive(Clone, Copy, Default)]
pub(super) struct Number {
    value: i32,
    negative: bool,
    digits: bool,
}
#[derive(Clone, Copy)]
pub(super) enum Consumer {
    Execute,
    DefTarget(bool),
    DefOpen,
    DefBody(u32),
    Message(u32),
    Integer(Number, Finish),
    Equals(Finish),
    Arithmetic(i32, bool),
    Keyword(u8, Finish),
    Quantity,
    Relation(i32, usize),
    LetTarget(bool),
    LetValue(usize, bool, u8),
    Skip(usize, bool),
}
impl Number {
    pub(super) fn push(&mut self, digit: i32) -> Result<()> {
        self.value = self
            .value
            .checked_mul(10)
            .and_then(|v| v.checked_add(digit))
            .ok_or(Error::Overflow)?;
        self.digits = true;
        Ok(())
    }
}
impl Consumer {
    pub fn raw(self) -> bool {
        matches!(
            self,
            Self::DefTarget(_)
                | Self::DefOpen
                | Self::DefBody(_)
                | Self::LetTarget(_)
                | Self::LetValue(..)
                | Self::Skip(..)
        )
    }
}
impl<const TRACE: bool> Producer<'_, TRACE> {
    pub(super) fn integer(&mut self, finish: Finish) -> Result<()> {
        self.push(Consumer::Integer(Number::default(), finish))
    }
    pub(super) fn number(
        &mut self,
        read: Read,
        mut number: Number,
        finish: Finish,
    ) -> Result<Action> {
        let digit = Engine::digit(read.token, 10);
        if digit < 10 {
            number.push(digit)?;
            *self.consumers.last_mut().unwrap() = Consumer::Integer(number, finish);
        } else if !number.digits {
            if read.token == Token::other(b'-') {
                number.negative = !number.negative;
                *self.consumers.last_mut().unwrap() = Consumer::Integer(number, finish);
            } else if read.meaning.command == cmd::COUNT {
                self.integer(Finish::InternalCount)?;
            } else if read.meaning.command != cmd::SPACE && read.token != Token::other(b'+') {
                return Err(Error::Number);
            }
        } else {
            self.consumers.pop();
            self.finish_integer(
                if number.negative {
                    -number.value
                } else {
                    number.value
                },
                finish,
            )?;
            if read.meaning.command != cmd::SPACE {
                return Ok(Action::Replay);
            }
        }
        Ok(Action::Continue)
    }
    fn index(value: i32) -> Result<usize> {
        if !(0..=255).contains(&value) {
            return Err(Error::Number);
        }
        Ok(value as usize)
    }
    fn finish_integer(&mut self, value: i32, finish: Finish) -> Result<()> {
        match finish {
            Finish::Target(kind, global, operation) => {
                let index = Self::index(value)?;
                let target = match kind {
                    TargetKind::Count => Target::count(index),
                    TargetKind::Catcode => Target::catcode(index),
                };
                let next = Finish::Assign(
                    target,
                    global,
                    operation,
                    matches!(kind, TargetKind::Catcode),
                );
                self.push(if operation == -1 {
                    Consumer::Equals(next)
                } else {
                    Consumer::Keyword(0, next)
                })?;
            }
            Finish::Assign(target, global, operation, catcode) => {
                if catcode && !(0..=15).contains(&value) {
                    return Err(Error::Number);
                }
                self.engine.update_integer(target, global, |old| {
                    Ok(match operation {
                        -1 => value,
                        0 => old.wrapping_add(value),
                        1 => {
                            let product = i64::from(old) * i64::from(value);
                            if product.abs() > i64::from(i32::MAX) {
                                return Err(Error::Overflow);
                            }
                            product as i32
                        }
                        2 => old.checked_div(value).ok_or(Error::Overflow)?,
                        _ => unreachable!(),
                    })
                })?;
            }
            Finish::Emit => self.emit_number(value)?,
            Finish::EmitCount => {
                self.emit_number(self.engine.scalars[Target::count(Self::index(value)?)].value)?
            }
            Finish::InternalCount => {
                let Consumer::Integer(number, finish) = self.consumers.pop().unwrap() else {
                    unreachable!()
                };
                let value = self.engine.scalars[Target::count(Self::index(value)?)].value;
                self.finish_integer(
                    if number.negative {
                        value.wrapping_neg()
                    } else {
                        value
                    },
                    finish,
                )?;
            }
            Finish::CompareLeft(condition) => self.push(Consumer::Relation(value, condition))?,
            Finish::CompareRight(condition, left, relation) => self.choose(
                condition,
                match relation {
                    b'=' => left == value,
                    b'<' => left < value,
                    b'>' => left > value,
                    _ => unreachable!(),
                },
            )?,
        }
        Ok(())
    }
    fn emit_number(&mut self, value: i32) -> Result<()> {
        if value < 0 {
            self.feed(Token::other(b'-'))?;
        }
        self.digits(value.unsigned_abs())
    }
    fn digits(&mut self, value: u32) -> Result<()> {
        if value >= 10 {
            self.digits(value / 10)?;
        }
        self.feed(Token::other(b'0' + (value % 10) as u8))?;
        Ok(())
    }
}
