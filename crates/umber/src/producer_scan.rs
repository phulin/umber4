use super::*;
pub(super) fn append(value: i32, digit: i32) -> Result<i32> {
    Engine::accumulate(value, 10, digit)
}
pub(super) fn index(value: i32) -> Result<usize> {
    if !(0..=255).contains(&value) {
        return Err(Error::Number);
    }
    Ok(value as usize)
}
impl<const TRACE: bool> Producer<'_, TRACE> {
    #[inline(always)]
    pub(super) fn expanded(&mut self, input: &mut Frame, depth: u16) -> Result<Read> {
        loop {
            let read = self.raw(input)?;
            if read.meaning.expandable() {
                self.expand(input, read, depth)?;
            } else {
                return Ok(read);
            }
        }
    }
    #[inline(always)]
    pub(super) fn nonspace<const EXPAND: bool>(
        &mut self,
        input: &mut Frame,
        depth: u16,
    ) -> Result<Read> {
        loop {
            let read = if EXPAND {
                self.expanded(input, depth)?
            } else {
                self.raw(input)?
            };
            if if EXPAND {
                read.meaning.command != cmd::SPACE
            } else {
                !read.token.is_space()
            } {
                return Ok(read);
            }
        }
    }
    #[inline(never)]
    pub(super) fn integer(&mut self, input: &mut Frame, depth: u16) -> Result<i32> {
        if depth == 256 {
            return Err(Error::Capacity);
        }
        if TRACE {
            self.stats.max_integer_depth = self.stats.max_integer_depth.max(usize::from(depth) + 1);
        }
        let mut value = 0;
        let mut negative = false;
        let mut digits = false;
        loop {
            let Some(read) = self.integer_token(input, &mut value, &mut digits)? else {
                return Ok(if negative { -value } else { value });
            };
            if read.meaning.expandable() {
                self.expand(input, read, depth + 1)?;
                continue;
            }
            let digit = Engine::digit(read.token, 10);
            if digit < 10 {
                value = append(value, digit)?;
                digits = true;
            } else if digits {
                if read.meaning.command != cmd::SPACE {
                    input.unread(read.token);
                }
                return Ok(if negative { -value } else { value });
            } else if read.token == Token::other(b'-') {
                negative = !negative;
            } else if read.meaning.command == cmd::COUNT {
                let register = index(self.integer(input, depth + 1)?)?;
                let value = self.engine.scalars[Target::count(register)].value;
                return Ok(if negative {
                    value.wrapping_neg()
                } else {
                    value
                });
            } else if read.meaning.command != cmd::SPACE && read.token != Token::other(b'+') {
                return Err(Error::Number);
            }
        }
    }
    #[inline(always)]
    pub(super) fn equals(&mut self, input: &mut Frame) -> Result<()> {
        if self.take_literal::<false>(input, b"=") {
            return Ok(());
        }
        let read = self.nonspace::<true>(input, 0)?;
        if read.token != Token::other(b'=') {
            input.unread(read.token);
        }
        Ok(())
    }
    #[inline(always)]
    pub(super) fn by(&mut self, input: &mut Frame) -> Result<()> {
        if self.take_literal::<true>(input, b"by") {
            return Ok(());
        }
        let first = self.nonspace::<true>(input, 0)?;
        if !first.token.is_cs() && first.token.byte().eq_ignore_ascii_case(&b'b') {
            let second = self.expanded(input, 0)?;
            if second.token.is_cs() || !second.token.byte().eq_ignore_ascii_case(&b'y') {
                return Err(Error::Unsupported);
            }
        } else {
            input.unread(first.token);
        }
        Ok(())
    }
    #[inline(never)]
    pub(super) fn expand(&mut self, input: &mut Frame, read: Read, depth: u16) -> Result<()> {
        if read.meaning.is_macro() {
            if TRACE {
                self.stats.macro_calls += 1;
            }
            let id = read.meaning.value as u32;
            if !self.tail(input, id) {
                let body = Body::new(self.engine.values.get(id));
                self.push_input(input, Source::Body(body))?;
            }
            return Ok(());
        }
        match read.meaning.command {
            cmd::NUMBER => {
                let value = self.integer(input, depth)?;
                self.decimal(input, value)?;
            }
            cmd::THE => {
                if depth == 256 {
                    return Err(Error::Capacity);
                }
                if self.expanded(input, depth + 1)?.meaning.command != cmd::COUNT {
                    return Err(Error::Unsupported);
                }
                let register = index(self.integer(input, depth)?)?;
                self.decimal(input, self.engine.scalars[Target::count(register)].value)?;
            }
            cmd::IFTRUE | cmd::IFFALSE | cmd::IFNUM => {
                let condition = self.engine.conditions.len();
                self.engine.conditions.push(Condition::Pending);
                let yes = if read.meaning.command == cmd::IFNUM {
                    let left = self.integer(input, depth)?;
                    let relation = self.nonspace::<true>(input, depth)?.token;
                    if ![Token::other(b'='), Token::other(b'<'), Token::other(b'>')]
                        .contains(&relation)
                    {
                        return Err(Error::Syntax);
                    }
                    let right = self.integer(input, depth)?;
                    match relation.byte() {
                        b'=' => left == right,
                        b'<' => left < right,
                        b'>' => left > right,
                        _ => unreachable!(),
                    }
                } else {
                    read.meaning.command == cmd::IFTRUE
                };
                if yes {
                    self.engine.conditions[condition] = Condition::Then { case: false };
                } else {
                    self.skip(input, false)?;
                }
            }
            cmd::FI => {
                self.engine.conditions.pop().ok_or(Error::Conditional)?;
            }
            cmd::ELSE => {
                if !matches!(self.engine.conditions.last(), Some(Condition::Then { .. })) {
                    return Err(Error::Conditional);
                }
                self.skip(input, true)?;
            }
            cmd::UNDEFINED => return Err(Error::Undefined),
            _ => return Err(Error::Unsupported),
        }
        Ok(())
    }
    fn skip(&mut self, input: &mut Frame, to_fi: bool) -> Result<()> {
        let mut nested = 0;
        loop {
            let command = self.raw(input)?.meaning.command;
            if cmd::is_conditional(command) {
                nested += 1;
            }
            if command == cmd::FI {
                if nested == 0 {
                    self.engine.conditions.pop();
                    return Ok(());
                }
                nested -= 1;
            } else if command == cmd::ELSE && nested == 0 && !to_fi {
                *self.engine.conditions.last_mut().unwrap() = Condition::Else;
                return Ok(());
            }
        }
    }
}
