use crate::{Engine, Error, Result, token::*};

impl Engine<'_> {
    pub(crate) fn expand(&mut self, read: Read) -> Result<()> {
        self.with_expansion(|engine| engine.expand_inner(read))
    }
    #[inline]
    pub(crate) fn with_expansion<T>(
        &mut self,
        action: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        if self.expansion_depth >= 256 {
            return Err(Error::Capacity);
        }
        self.expansion_depth += 1;
        let result = action(self);
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
                crate::lists::Internal::Font(token) => {
                    self.check_source_limit()?;
                    self.back(token);
                    Ok(())
                }
                crate::lists::Internal::Quantity(crate::quantities::Quantity::Integer(value)) => {
                    self.insert_number(value)
                }
                crate::lists::Internal::Quantity(value) => {
                    let mut text = [0; 128];
                    let length = crate::quantities::quantity_text(value, &mut text);
                    let mut output = crate::convert::Generated::default();
                    for &byte in &text[..length] {
                        self.generated_token(&mut output, Token::string_character(byte))?;
                    }
                    self.finish_generated(output)
                }
                crate::lists::Internal::Tokens(Some(owner)) => self.push_tokens(owner, 0),
                crate::lists::Internal::Tokens(None) => Ok(()),
            },
            cmd::STRING | cmd::MEANING | cmd::ROMAN | cmd::DETOKENIZE => self.convert(read),
            cmd::UNEXPANDED => {
                let mut output = crate::convert::Generated::default();
                self.scan_raw_text(&mut |engine: &mut Self, token: Token| {
                    engine.generated_token(&mut output, token)
                })?;
                self.finish_generated(output)
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
                let token = self
                    .with_scanner(crate::scanner::Scanner::Normal, |engine| engine.next_raw())?
                    .token;
                self.push_inline([token, Token(0)], 1, true);
                Ok(())
            }
            cmd::CSNAME => {
                let id = self.scan_name()?;
                if self.document.bindings[id as usize].meaning() == Meaning::UNDEFINED {
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
        if (-9..=99).contains(&value) {
            self.check_source_limit()?;
            match value {
                -9..=-1 => {
                    self.insert_pair(Token::other(b'-'), Token::other(b'0' + (-value) as u8));
                }
                0..=9 => self.back(Token::other(b'0' + value as u8)),
                _ => self.insert_pair(
                    Token::other(b'0' + (value / 10) as u8),
                    Token::other(b'0' + (value % 10) as u8),
                ),
            }
            return Ok(());
        }
        let mut output = crate::convert::Generated::default();
        self.emit_number(value, &mut |engine: &mut Self, token: Token| {
            engine.generated_token(&mut output, token)
        })?;
        self.finish_generated(output)
    }
}
