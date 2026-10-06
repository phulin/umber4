use crate::{Engine, Error, Result, token::*};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Condition {
    Pending,
    Then,
    Else,
}

impl Engine<'_> {
    pub(crate) fn expand(&mut self, read: Read) -> Result<()> {
        if self.expansion_depth >= 256 {
            return Err(Error::Capacity);
        }
        self.expansion_depth += 1;
        let result = self.expand_inner(read);
        self.expansion_depth -= 1;
        result
    }
    fn expand_inner(&mut self, read: Read) -> Result<()> {
        match read.meaning.command {
            cmd::UNDEFINED => Err(Error::Undefined),
            cmd::NUMBER | cmd::THE => {
                let value = if read.meaning.command == cmd::NUMBER {
                    self.scan_int()?
                } else {
                    let operand = self.next_expanded()?;
                    self.internal_int(operand)?
                };
                self.insert_number(value)
            }
            cmd::EXPANDAFTER => {
                let first = self.next_raw()?.token;
                let second = self.next_token()?;
                if second.meaning.command == cmd::MACRO {
                    self.begin_macro(second.meaning.value as u32)?;
                } else if second.meaning.expandable() {
                    self.expand(second)?;
                } else {
                    self.back(second.raw.token);
                }
                self.back(first);
                Ok(())
            }
            cmd::NOEXPAND => {
                let token = self.next_raw()?.token;
                self.sources.push(Source::Inline {
                    tokens: [token, Token(0)],
                    position: 0,
                    len: 1,
                    suppress: true,
                });
                Ok(())
            }
            cmd::CSNAME => {
                let mut name = Vec::new();
                loop {
                    let part = self.next_expanded()?;
                    if part.meaning.command == cmd::ENDCSNAME {
                        break;
                    }
                    if part.raw.token.is_cs() {
                        return Err(Error::Syntax);
                    }
                    name.push(part.raw.token.byte());
                }
                let id = self.intern(&name);
                if self.bindings[id as usize].meaning == Meaning::UNDEFINED {
                    self.assign_meaning(id as usize, Meaning::RELAX, false);
                }
                self.back(Token::cs(id));
                Ok(())
            }
            cmd::ENDCSNAME => Err(Error::Syntax),
            cmd::IFNUM | cmd::IFX | cmd::IFTRUE | cmd::IFFALSE => {
                self.conditional(read.meaning.command)
            }
            cmd::ELSE | cmd::FI => {
                let phase = *self.conditions.last().ok_or(Error::Conditional)?;
                if phase == Condition::Pending {
                    self.insert_pair(FROZEN_RELAX, read.raw.token);
                    return Ok(());
                }
                if read.meaning.command == cmd::FI {
                    self.conditions.pop();
                    return Ok(());
                }
                if phase == Condition::Else {
                    return Err(Error::Conditional);
                }
                while self.skip_boundary()? != cmd::FI {}
                self.conditions.pop();
                Ok(())
            }
            _ => Err(Error::Unsupported),
        }
    }
    fn conditional(&mut self, command: u8) -> Result<()> {
        let target = self.conditions.len();
        self.conditions.push(Condition::Pending);
        let yes = match command {
            cmd::IFTRUE => true,
            cmd::IFFALSE => false,
            cmd::IFX => {
                let a = self.next_token()?.meaning;
                let b = self.next_token()?.meaning;
                self.same_meaning(a, b)
            }
            cmd::IFNUM => {
                let a = self.scan_int()?;
                let relation = self.nonspace(true)?.raw.token;
                let b = self.scan_int()?;
                if relation == Token::other(b'=') {
                    a == b
                } else if relation == Token::other(b'<') {
                    a < b
                } else if relation == Token::other(b'>') {
                    a > b
                } else {
                    return Err(Error::Syntax);
                }
            }
            _ => unreachable!(),
        };
        if yes {
            self.conditions[target] = Condition::Then;
            return Ok(());
        }
        loop {
            let boundary = self.skip_boundary()?;
            if self.conditions.len() - 1 != target {
                if boundary == cmd::FI {
                    self.conditions.pop();
                }
            } else {
                if boundary == cmd::FI {
                    self.conditions.pop();
                } else {
                    self.conditions[target] = Condition::Else;
                }
                return Ok(());
            }
        }
    }
    fn skip_boundary(&mut self) -> Result<u8> {
        let mut depth = 0;
        loop {
            let command = self.next_token()?.meaning.command;
            if matches!(command, cmd::IFNUM | cmd::IFX | cmd::IFTRUE | cmd::IFFALSE) {
                depth += 1;
            } else if command == cmd::FI {
                if depth == 0 {
                    return Ok(command);
                }
                depth -= 1;
            } else if command == cmd::ELSE && depth == 0 {
                return Ok(command);
            }
        }
    }
    fn insert_number(&mut self, value: i32) -> Result<()> {
        let packet = self.take_packet(1);
        let mut builder = None;
        if value < 0 {
            self.capture_raw(
                packet,
                0,
                Raw {
                    token: Token::other(b'-'),
                    origin: None,
                    suppress: false,
                },
                &mut builder,
            );
        }
        let number = value.unsigned_abs();
        let mut power = 1;
        while number / power >= 10 {
            power *= 10;
        }
        loop {
            let token = Token::other(b'0' + ((number / power) % 10) as u8);
            self.capture_raw(
                packet,
                0,
                Raw {
                    token,
                    origin: None,
                    suppress: false,
                },
                &mut builder,
            );
            if power == 1 {
                break;
            }
            power /= 10;
        }
        self.finish_capture(packet, builder);
        self.push_sequence(SequenceRef::Argument(packet, 0), Some(packet), None)
    }
}
