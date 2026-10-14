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
    pub(super) fn finish_integer(&mut self, mut value: i32, mut finish: Finish) -> Result<()> {
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
                    return self.assign_integer(target, global, operation, catcode, value);
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
                    // This parent completes immediately, so read it directly
                    // without restoring a soon-to-be-replaced active record.
                    let Consumer::Integer {
                        negative,
                        finish: parent,
                        ..
                    } = self.suspended.pop().unwrap()
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
    #[inline(always)]
    fn arithmetic(old: i32, operation: i32, value: i32) -> Result<i32> {
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
    }
    #[inline(always)]
    fn assign_integer(
        &mut self,
        target: Target,
        global: bool,
        operation: i32,
        catcode: bool,
        value: i32,
    ) -> Result<()> {
        if catcode && !(0..=15).contains(&value) {
            return Err(Error::Number);
        }
        self.engine.update_integer(target, global, |old| {
            Self::arithmetic(old, operation, value)
        })
    }
    // Speculation changes only this borrowed slice cursor. A failed recognition
    // leaves execution, its consumer, and the producer cursor untouched.
    #[inline(never)]
    pub(super) fn scalar_window(&mut self, read: Read, words: &[Token]) -> usize {
        if self.prefix & 2 != 0 {
            return 0;
        }
        let mut rest = words;
        let mut operation = -1;
        let catcode = read.meaning.command == cmd::CATCODE;
        let target = if read.meaning.command == cmd::ENDLINE {
            Target::ENDLINE
        } else {
            if read.meaning.command == cmd::ARITH {
                let Some(register) = self.window_nonspace(&mut rest) else {
                    return 0;
                };
                if self.engine.resolve(register).command != cmd::COUNT {
                    return 0;
                }
                operation = read.meaning.value;
            }
            let Some(index) = self.window_number::<true>(&mut rest) else {
                return 0;
            };
            let Ok(index) = Self::index(index) else {
                return 0;
            };
            if catcode {
                Target::catcode(index)
            } else {
                Target::count(index)
            }
        };
        while rest
            .first()
            .is_some_and(|&token| self.engine.resolve(token).command == cmd::SPACE)
        {
            rest = &rest[1..];
        }
        if operation == -1 {
            if rest.first() == Some(&Token::other(b'=')) {
                rest = &rest[1..];
            }
        } else if rest
            .first()
            .is_some_and(|&token| !token.is_cs() && token.byte().eq_ignore_ascii_case(&b'b'))
        {
            let Some(&token) = rest.get(1) else {
                return 0;
            };
            if token.is_cs() || !token.byte().eq_ignore_ascii_case(&b'y') {
                return 0;
            }
            rest = &rest[2..];
        }
        let Some(value) = self.window_number::<true>(&mut rest) else {
            return 0;
        };
        if catcode && !(0..=15).contains(&value) {
            return 0;
        }
        let Ok(result) = Self::arithmetic(self.engine.scalars[target].value, operation, value)
        else {
            return 0;
        };
        let global = self.prefix & 1 != 0;
        self.prefix = 0;
        // Recognition has no execution effects, so this computed value still
        // describes the slot read by the shared group-aware scalar setter.
        // The setter returns errors only from its callback, which is constant
        // here. Arithmetic failure already rejected the speculative operation.
        self.engine
            .update_integer(target, global, |_| Ok(result))
            .expect("constant scalar update is infallible");
        // A completed assignment leaves execution active, where a resolved
        // relax is inert. Consume this terminator with the operation.
        if rest
            .first()
            .is_some_and(|&token| self.engine.resolve(token).command == cmd::RELAX)
        {
            rest = &rest[1..];
        }
        words.len() - rest.len()
    }
    #[inline(always)]
    fn window_nonspace(&self, rest: &mut &[Token]) -> Option<Token> {
        loop {
            let (&token, tail) = rest.split_first()?;
            *rest = tail;
            let meaning = self.engine.resolve(token);
            if meaning.expandable() {
                return None;
            }
            if meaning.command != cmd::SPACE {
                return Some(token);
            }
        }
    }
    #[inline(always)]
    fn window_number<const INTERNAL: bool>(&self, rest: &mut &[Token]) -> Option<i32> {
        let mut negative = false;
        let token = loop {
            let token = self.window_nonspace(rest)?;
            if token == Token::other(b'-') {
                negative = !negative;
            } else if token != Token::other(b'+') {
                break token;
            }
        };
        let mut value = Engine::digit(token, 10);
        if value >= 10 {
            if !INTERNAL || self.engine.resolve(token).command != cmd::COUNT {
                return None;
            }
            let index = Self::index(self.window_number::<false>(rest)?).ok()?;
            let value = self.engine.scalars[Target::count(index)].value;
            return Some(if negative {
                value.wrapping_neg()
            } else {
                value
            });
        }
        loop {
            let (&token, tail) = rest.split_first()?;
            let digit = Engine::digit(token, 10);
            if digit >= 10 {
                let meaning = self.engine.resolve(token);
                if meaning.expandable() {
                    return None;
                }
                if meaning.command == cmd::SPACE {
                    *rest = tail;
                }
                return Some(if negative { -value } else { value });
            }
            value = Number::append(value, digit).ok()?;
            *rest = tail;
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
