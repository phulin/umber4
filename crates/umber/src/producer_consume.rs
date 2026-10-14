use super::*;
impl<const TRACE: bool> Producer<'_, TRACE> {
    #[inline(always)]
    pub(super) fn consume(&mut self, read: Read) -> Result<bool> {
        match self.consumer {
            Consumer::Execute => self.execute(read)?,
            Consumer::Integer {
                value,
                negative,
                digits,
                finish,
            } => {
                return self.number(
                    read,
                    Number {
                        value,
                        negative,
                        digits,
                    },
                    finish,
                );
            }
            Consumer::Equals(finish) => {
                if read.meaning.command == cmd::SPACE {
                    return Ok(false);
                }
                self.consumer = Number::default().consumer(finish);
                if read.token != Token::other(b'=') {
                    return Ok(true);
                }
            }
            Consumer::Arithmetic(operation, global) => {
                if read.meaning.command == cmd::SPACE {
                    return Ok(false);
                }
                if read.meaning.command != cmd::COUNT {
                    return Err(Error::Unsupported);
                }
                self.consumer = Number::default().consumer(Finish::Target(
                    TargetKind::Count,
                    global,
                    operation,
                ));
            }
            Consumer::Keyword(matched, finish) => {
                if matched == 0 && read.meaning.command == cmd::SPACE {
                    return Ok(false);
                }
                if !read.token.is_cs()
                    && read.token.byte().to_ascii_lowercase() == b"by"[matched as usize]
                {
                    self.consumer = if matched == 0 {
                        Consumer::Keyword(1, finish)
                    } else {
                        Number::default().consumer(finish)
                    };
                } else if matched == 0 {
                    self.consumer = Number::default().consumer(finish);
                    return Ok(true);
                } else {
                    return Err(Error::Unsupported);
                }
            }
            Consumer::Quantity => {
                if read.meaning.command != cmd::COUNT {
                    return Err(Error::Unsupported);
                }
                self.consumer = Number::default().consumer(Finish::EmitCount);
            }
            Consumer::Relation(left, condition) => {
                if read.meaning.command == cmd::SPACE {
                    return Ok(false);
                }
                if ![Token::other(b'='), Token::other(b'<'), Token::other(b'>')]
                    .contains(&read.token)
                {
                    return Err(Error::Syntax);
                }
                self.consumer = Number::default().consumer(Finish::CompareRight(
                    condition,
                    left,
                    read.token.byte(),
                ));
            }
            Consumer::LetTarget(global) => {
                if read.token.is_space() {
                    return Ok(false);
                }
                if !read.token.is_cs() {
                    return Err(Error::Syntax);
                }
                self.consumer = Consumer::LetValue(read.token.id(), global, 0);
            }
            Consumer::LetValue(target, global, phase) => {
                if phase == 0 && read.token.is_space() {
                    return Ok(false);
                }
                if phase == 0 && read.token == Token::other(b'=') {
                    self.consumer = Consumer::LetValue(target, global, 1);
                } else if phase == 1 && read.token.is_space() {
                    self.consumer = Consumer::LetValue(target, global, 2);
                } else {
                    self.pop();
                    self.engine.assign_meaning(target, read.meaning, global);
                }
            }
            Consumer::Skip(mut nested, to_fi) => {
                if read.meaning.is_conditional() {
                    nested += 1;
                }
                if read.meaning.command == cmd::FI {
                    if nested == 0 {
                        self.pop();
                        self.engine.conditions.pop();
                        return Ok(false);
                    }
                    nested -= 1;
                } else if read.meaning.command == cmd::ELSE && nested == 0 && !to_fi {
                    self.pop();
                    *self.engine.conditions.last_mut().unwrap() = Condition::Else;
                    return Ok(false);
                }
                self.consumer = Consumer::Skip(nested, to_fi);
            }
            _ => return self.consume_slow(read),
        }
        Ok(false)
    }
    #[inline(never)]
    fn consume_slow(&mut self, read: Read) -> Result<bool> {
        match self.consumer {
            Consumer::DefTarget(global) => {
                if read.token.is_space() {
                    return Ok(false);
                }
                if !read.token.is_cs() {
                    return Err(Error::Syntax);
                }
                self.start_definition(read.token.id(), global)?;
                self.consumer = Consumer::DefOpen;
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
                self.consumer = Consumer::DefBody(1);
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
                    self.consumer = Consumer::DefBody(depth);
                }
            }
            Consumer::Message(0) => {
                if matches!(read.meaning.command, cmd::SPACE | cmd::RELAX) {
                    return Ok(false);
                }
                if read.meaning.command != cmd::BEGIN {
                    return Err(Error::Syntax);
                }
                self.consumer = Consumer::Message(1);
            }
            Consumer::Message(mut depth) => {
                if read.meaning.command == cmd::BEGIN {
                    depth += 1;
                }
                if read.meaning.command == cmd::END_GROUP_CHAR {
                    depth -= 1;
                }
                if depth == 0 {
                    self.pop();
                    self.engine.messages.push(std::mem::take(&mut self.output));
                } else {
                    self.engine.print_token(read.token, &mut self.output);
                    self.consumer = Consumer::Message(depth);
                }
            }
            _ => unreachable!(),
        }
        Ok(false)
    }
}
