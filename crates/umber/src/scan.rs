use crate::{Engine, Error, Result, token::*};

impl Engine<'_> {
    pub(crate) fn nonspace(&mut self, expanded: bool) -> Result<Read> {
        loop {
            let read = if expanded {
                self.next_expanded()?
            } else {
                self.next_token()?
            };
            if read.meaning.command != cmd::SPACE {
                return Ok(read);
            }
        }
    }
    pub(crate) fn scan_index(&mut self) -> Result<usize> {
        let value = self.scan_int()?;
        if !(0..=255).contains(&value) {
            return Err(Error::Number);
        }
        Ok(value as usize)
    }
    pub(crate) fn scan_equals(&mut self) -> Result<()> {
        let read = self.nonspace(true)?;
        if read.raw.token != Token::other(b'=') {
            self.back(read.raw.token);
        }
        Ok(())
    }
    pub(crate) fn internal_int(&mut self, read: Read) -> Result<i32> {
        match read.meaning.command {
            cmd::COUNT => {
                let index = self.scan_index()?;
                Ok(self.counts[index].value)
            }
            cmd::COUNT_REF => Ok(self.counts[read.meaning.value as usize].value),
            cmd::CATCODE => {
                let index = self.scan_index()?;
                Ok(self.catcodes[index].value)
            }
            cmd::ENDLINE => Ok(self.endline.value),
            _ => Err(Error::Number),
        }
    }
    pub(crate) fn scan_int(&mut self) -> Result<i32> {
        let mut negative = false;
        let mut read = loop {
            let read = self.nonspace(true)?;
            if read.raw.token == Token::other(b'-') {
                negative = !negative;
            } else if read.raw.token != Token::other(b'+') {
                break read;
            }
        };
        let value = if read.raw.token == Token::other(b'`') {
            let raw = self.next_raw()?;
            let value = if !raw.token.is_cs() {
                i32::from(raw.token.byte())
            } else if (2..258).contains(&raw.token.id()) {
                (raw.token.id() - 2) as i32
            } else if (258..514).contains(&raw.token.id()) {
                (raw.token.id() - 258) as i32
            } else {
                return Err(Error::Number);
            };
            let read = self.next_expanded()?;
            if read.meaning.command != cmd::SPACE {
                self.back(read.raw.token);
            }
            value
        } else if matches!(
            read.meaning.command,
            cmd::COUNT | cmd::COUNT_REF | cmd::CATCODE | cmd::ENDLINE
        ) {
            self.internal_int(read)?
        } else {
            let radix = if read.raw.token == Token::other(b'\'') {
                8
            } else if read.raw.token == Token::other(b'"') {
                16
            } else {
                10
            };
            if radix != 10 {
                read = self.next_expanded()?;
            }
            let mut value = 0i32;
            let mut digits = 0;
            loop {
                let token = read.raw.token;
                let digit = if !token.is_cs() && token.cat() == 12 && token.byte().is_ascii_digit()
                {
                    i32::from(token.byte() - b'0')
                } else if radix == 16
                    && !token.is_cs()
                    && matches!(token.cat(), 11 | 12)
                    && (b'A'..=b'F').contains(&token.byte())
                {
                    i32::from(token.byte() - b'A' + 10)
                } else {
                    radix
                };
                if digit >= radix {
                    break;
                }
                value = value
                    .checked_mul(radix)
                    .and_then(|v| v.checked_add(digit))
                    .ok_or(Error::Overflow)?;
                digits += 1;
                read = self.next_expanded()?;
            }
            if read.meaning.command != cmd::SPACE {
                self.back(read.raw.token);
            }
            if digits == 0 {
                return Err(Error::Number);
            }
            value
        };
        Ok(if negative {
            value.wrapping_neg()
        } else {
            value
        })
    }
    pub(crate) fn scan_keyword(&mut self, keyword: &[u8]) -> Result<bool> {
        let mut tokens = [Token(0); 8];
        for (index, &byte) in keyword.iter().enumerate() {
            let read = if index == 0 {
                self.nonspace(true)?
            } else {
                self.next_expanded()?
            };
            tokens[index] = read.raw.token;
            if read.raw.token.is_cs() || read.raw.token.byte().to_ascii_lowercase() != byte {
                for token in tokens[..=index].iter().rev() {
                    self.back(*token);
                }
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub(crate) fn scan_cs(&mut self) -> Result<usize> {
        let read = loop {
            let read = self.next_token()?;
            if !read.raw.token.is_space() {
                break read;
            }
        };
        if !read.raw.token.is_cs() || read.raw.token == FROZEN_RELAX {
            return Err(Error::Syntax);
        }
        Ok(read.raw.token.id())
    }
    pub(crate) fn opening_brace(&mut self) -> Result<()> {
        loop {
            let read = self.next_expanded()?;
            if matches!(read.meaning.command, cmd::RELAX | cmd::SPACE) {
                continue;
            }
            return if read.meaning.command == cmd::BEGIN {
                Ok(())
            } else {
                Err(Error::Syntax)
            };
        }
    }
}
