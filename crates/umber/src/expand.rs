use crate::{Engine, Error, Result, token::*};

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
            cmd::NUMBER => {
                let value = self.scan_int()?;
                self.insert_number(value)
            }
            cmd::THE => match self.the_value()? {
                crate::lists::Internal::Integer(value) => self.insert_number(value),
                crate::lists::Internal::Tokens(span) => self.push_tokens(span),
            },
            cmd::STRING | cmd::MEANING | cmd::ROMAN | cmd::DETOKENIZE => self.convert(read),
            cmd::UNEXPANDED => {
                self.opening_brace()?;
                let packet = self.take_packet(1)?;
                let mut builder = None;
                self.scan_raw_body(|engine, token| {
                    engine.generated_token(packet, &mut builder, token)
                })?;
                self.finish_generated(packet, builder);
                self.push_sequence(SequenceRef::Argument(packet, 0), Some(packet), None)
            }
            cmd::EXPANDAFTER => {
                let first = self.next_raw()?.token;
                let second = self.next_token()?;
                if second.meaning.is_macro() {
                    self.begin_macro(second.meaning.value as u32)?;
                } else if second.meaning.expandable() {
                    self.expand(second)?;
                } else {
                    self.back(second.token);
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
                let id = self.scan_name()?;
                if self.bindings[id as usize].meaning() == Meaning::UNDEFINED {
                    self.assign_meaning(id as usize, Meaning::RELAX, false);
                }
                self.back(Token::cs(id));
                Ok(())
            }
            cmd::ENDCSNAME => Err(Error::Syntax),
            cmd::UNLESS => {
                let test = self.next_token()?;
                if !test.meaning.is_conditional() || test.meaning.command == cmd::IFCASE {
                    return Err(Error::Conditional);
                }
                self.conditional(test.meaning.command, true)
            }
            command if read.meaning.is_conditional() => self.conditional(command, false),
            cmd::ELSE | cmd::FI | cmd::OR => self.condition_boundary(read),
            _ => Err(Error::Unsupported),
        }
    }
    pub(crate) fn insert_number(&mut self, value: i32) -> Result<()> {
        let packet = self.take_packet(1)?;
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
            )?;
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
            )?;
            if power == 1 {
                break;
            }
            power /= 10;
        }
        self.finish_capture(packet, builder);
        self.push_sequence(SequenceRef::Argument(packet, 0), Some(packet), None)
    }
}
