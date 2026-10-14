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
    pub(super) value: i32,
    pub(super) negative: bool,
    pub(super) digits: bool,
}
#[derive(Clone, Copy)]
pub(super) enum Consumer {
    Execute,
    DefTarget(bool),
    DefOpen,
    DefBody(u32),
    Message(u32),
    Integer {
        value: i32,
        negative: bool,
        digits: bool,
        finish: Finish,
    },
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
    pub(super) fn consumer(self, finish: Finish) -> Consumer {
        Consumer::Integer {
            value: self.value,
            negative: self.negative,
            digits: self.digits,
            finish,
        }
    }
    pub(super) fn append(value: i32, digit: i32) -> Result<i32> {
        value
            .checked_mul(10)
            .and_then(|v| v.checked_add(digit))
            .ok_or(Error::Overflow)
    }
    pub(super) fn push(&mut self, digit: i32) -> Result<()> {
        self.value = Self::append(self.value, digit)?;
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
        self.push(Number::default().consumer(finish))
    }
    pub(super) fn number(
        &mut self,
        read: Read,
        mut number: Number,
        finish: Finish,
    ) -> Result<bool> {
        let digit = Engine::digit(read.token, 10);
        if digit < 10 {
            number.push(digit)?;
            self.consumer = number.consumer(finish);
        } else if !number.digits {
            if read.token == Token::other(b'-') {
                number.negative = !number.negative;
                self.consumer = number.consumer(finish);
            } else if read.meaning.command == cmd::COUNT {
                self.integer(Finish::InternalCount)?;
            } else if read.meaning.command != cmd::SPACE && read.token != Token::other(b'+') {
                return Err(Error::Number);
            }
        } else {
            self.finish_integer(
                if number.negative {
                    -number.value
                } else {
                    number.value
                },
                finish,
            )?;
            if read.meaning.command != cmd::SPACE {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn index(value: i32) -> Result<usize> {
        if !(0..=255).contains(&value) {
            return Err(Error::Number);
        }
        Ok(value as usize)
    }
    #[inline(always)]
    fn finish_integer(&mut self, mut value: i32, mut finish: Finish) -> Result<()> {
        loop {
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
                    self.consumer = if operation == -1 {
                        Consumer::Equals(next)
                    } else {
                        Consumer::Keyword(0, next)
                    };
                    return Ok(());
                }
                Finish::Assign(target, global, operation, catcode) => {
                    self.pop();
                    if catcode && !(0..=15).contains(&value) {
                        return Err(Error::Number);
                    }
                    return self.engine.update_integer(target, global, |old| {
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
                    });
                }
                Finish::CompareLeft(condition) => {
                    self.consumer = Consumer::Relation(value, condition);
                    return Ok(());
                }
                Finish::CompareRight(condition, left, relation) => {
                    self.pop();
                    return self.choose(
                        condition,
                        match relation {
                            b'=' => left == value,
                            b'<' => left < value,
                            b'>' => left > value,
                            _ => unreachable!(),
                        },
                    );
                }
                Finish::InternalCount => {
                    self.pop();
                    let Consumer::Integer {
                        negative,
                        finish: parent,
                        ..
                    } = self.consumer
                    else {
                        unreachable!()
                    };
                    value = self.engine.scalars[Target::count(Self::index(value)?)].value;
                    if negative {
                        value = value.wrapping_neg();
                    }
                    finish = parent;
                }
                _ => return self.finish_integer_slow(value, finish),
            }
        }
    }
    #[inline(never)]
    fn finish_integer_slow(&mut self, value: i32, finish: Finish) -> Result<()> {
        self.pop();
        match finish {
            Finish::Emit => self.emit_number(value),
            Finish::EmitCount => {
                self.emit_number(self.engine.scalars[Target::count(Self::index(value)?)].value)
            }
            _ => unreachable!(),
        }
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
