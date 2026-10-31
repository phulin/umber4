use crate::{Engine, Error, Result, scalars::Target, token::*};

impl Engine<'_> {
    #[inline(always)]
    fn digit(token: Token, radix: i32) -> i32 {
        let digit = token.0.wrapping_sub(Token::other(b'0').0);
        if digit < 10 {
            return digit as i32;
        }
        if radix == 16 {
            let letter = token.0.wrapping_sub(Token::character(11, b'A').0);
            if letter < 6 {
                return letter as i32 + 10;
            }
            let other = token.0.wrapping_sub(Token::other(b'A').0);
            if other < 6 {
                return other as i32 + 10;
            }
        }
        radix
    }
    #[inline(always)]
    fn scan_digit_window(&mut self, radix: i32, value: &mut i32) -> Result<bool> {
        let Some(cursor) = self
            .document
            .sources
            .last_mut()
            .and_then(Source::cursor_mut)
        else {
            return Ok(false);
        };
        while cursor.next != cursor.end {
            // SAFETY: the source retains this initialized immutable window.
            // This scan neither replaces sources nor changes backing storage.
            let token = unsafe { cursor.next.read() };
            let digit = Self::digit(token, radix);
            if digit < radix {
                // SAFETY: next is before end in the same allocation.
                cursor.next = unsafe { cursor.next.add(1) };
                *value = value
                    .checked_mul(radix)
                    .and_then(|v| v.checked_add(digit))
                    .ok_or(Error::Overflow)?;
                continue;
            }
            if token.is_parameter() {
                return Ok(false);
            }
            // Engine::resolve would borrow the engine across this cursor.
            let meaning = if token.is_cs() {
                self.document.bindings[token.id()].meaning()
            } else {
                Meaning {
                    command: token.cat(),
                    value: i32::from(token.byte()),
                }
            };
            if meaning.expandable() {
                return Ok(false);
            }
            // A nonspace terminator already has its original token identity at
            // the cursor. Its next read resolves its meaning afresh, just as a
            // backed token would. Inline sources retain the ordinary reader's
            // one-read suppression handling.
            if meaning.command == cmd::SPACE {
                // SAFETY: the terminating token is inside this source window.
                cursor.next = unsafe { cursor.next.add(1) };
            }
            return Ok(true);
        }
        Ok(false)
    }
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
        if read.token != Token::other(b'=') {
            self.back(read.token);
        }
        Ok(())
    }
    pub(crate) fn internal_int(&mut self, read: Read) -> Result<i32> {
        if let Some(value) = self.page_quantity(read)? {
            return Ok(match value {
                crate::quantities::Quantity::Integer(v) => v,
                crate::quantities::Quantity::Dimension(v) => v.0,
                crate::quantities::Quantity::Glue(v) => v.width.0,
            });
        }
        match read.meaning.command {
            cmd::PAR_SHAPE => Ok(self
                .document
                .layout_values
                .shape
                .lines
                .as_ref()
                .map_or(0, |s| s.len() as i32)),
            cmd::FONT_DIMEN | cmd::FONT_METRIC => Ok(self.font_dimension(read)?.0),
            cmd::FONT_INT | cmd::SPACE_FACTOR => self.font_integer(read),
            cmd::CHAR_GIVEN => Ok(read.meaning.value),
            cmd::NUMEXPR => self.scan_expression(),
            cmd::DIMEN | cmd::DIMEN_REF | cmd::DIMEN_PARAM => {
                let target = self.dimen_target(read)?;
                Ok(self.document.layout_values.dimens[target].value.0)
            }
            cmd::SKIP | cmd::SKIP_REF | cmd::GLUE_PARAM => {
                let target = self.glue_target(read)?;
                Ok(self.document.layout_values.glues[target].value.width.0)
            }
            _ => {
                let target = self.scalar_target(read)?;
                Ok(self.document.scalars[target].value)
            }
        }
    }
    #[inline]
    pub(crate) fn scalar_target(&mut self, read: Read) -> Result<Target> {
        Ok(match read.meaning.command {
            cmd::SF_CODE => Target::sfcode(self.scan_index()?),
            cmd::COUNT => Target::count(self.scan_index()?),
            cmd::COUNT_REF => Target::count(read.meaning.value as usize),
            cmd::CATCODE => Target::catcode(self.scan_index()?),
            cmd::ENDLINE => Target::ENDLINE,
            cmd::INTEGER_PARAM => Target::parameter(read.meaning.value as usize),
            cmd::CASE_CODE => Target::casecode(read.meaning.value as usize, self.scan_index()?),
            _ => return Err(Error::Number),
        })
    }
    #[inline(always)]
    fn literal_digit(&mut self) -> Option<i32> {
        if self.document.sources.is_empty() {
            return self
                .document
                .file
                .literal_digit(self.document.scalars.catcodes());
        }
        let cursor = self
            .document
            .sources
            .last_mut()
            .and_then(Source::cursor_mut)?;
        if cursor.next == cursor.end {
            return None;
        }
        // SAFETY: this source retains its initialized immutable window. Peeking
        // neither changes sources nor retires their backing storage.
        let digit = Self::digit(unsafe { cursor.next.read() }, 10);
        if digit >= 10 {
            return None;
        }
        // SAFETY: next is before end in the same allocation. A literal digit
        // needs no expansion or meaning lookup; the suffix uses the usual scan.
        cursor.next = unsafe { cursor.next.add(1) };
        Some(digit)
    }
    // Keep the suffix loop out of the general scanner's recursive stack frame.
    #[inline(never)]
    pub(crate) fn scan_digits<const RADIX: i32>(&mut self, mut value: i32) -> Result<i32> {
        loop {
            if self.scan_digit_window(RADIX, &mut value)? {
                return Ok(value);
            }
            let read = self.next_expanded()?;
            let digit = Self::digit(read.token, RADIX);
            if digit >= RADIX {
                if read.meaning.command != cmd::SPACE {
                    self.back(read.token);
                }
                return Ok(value);
            }
            value = value
                .checked_mul(RADIX)
                .and_then(|v| v.checked_add(digit))
                .ok_or(Error::Overflow)?;
        }
    }
    #[inline]
    pub(crate) fn scan_int(&mut self) -> Result<i32> {
        if let Some(value) = self.literal_digit() {
            return self.scan_digits::<10>(value);
        }
        self.scan_int_slow()
    }
    #[inline(never)]
    fn scan_int_slow(&mut self) -> Result<i32> {
        let mut negative = false;
        let mut read = loop {
            let read = self.nonspace(true)?;
            if read.token == Token::other(b'-') {
                negative = !negative;
            } else if read.token != Token::other(b'+') {
                break read;
            }
        };
        let value = if read.token == Token::other(b'`') {
            let token = self.next_raw()?.token;
            let byte = if token.is_cs() {
                token.single_byte().or(token.active_byte())
            } else {
                Some(token.byte())
            };
            let value = i32::from(byte.ok_or(Error::Number)?);
            let read = self.next_expanded()?;
            if read.meaning.command != cmd::SPACE {
                self.back(read.token);
            }
            value
        } else if let Some(value) = self.quantity(read)? {
            match value {
                crate::quantities::Quantity::Integer(v) => v,
                crate::quantities::Quantity::Dimension(v) => v.0,
                crate::quantities::Quantity::Glue(v) => v.width.0,
            }
        } else {
            let radix = if read.token == Token::other(b'\'') {
                8
            } else if read.token == Token::other(b'"') {
                16
            } else {
                10
            };
            if radix != 10 {
                read = self.next_expanded()?;
            }
            let digit = Self::digit(read.token, radix);
            if digit >= radix {
                if read.meaning.command != cmd::SPACE {
                    self.back(read.token);
                }
                return Err(Error::Number);
            }
            match radix {
                8 => self.scan_digits::<8>(digit)?,
                16 => self.scan_digits::<16>(digit)?,
                _ => self.scan_digits::<10>(digit)?,
            }
        };
        Ok(if negative {
            value.wrapping_neg()
        } else {
            value
        })
    }
    pub(crate) fn scan_keyword(&mut self, keyword: &[u8]) -> Result<bool> {
        if let Some(cursor) = self
            .document
            .sources
            .last_mut()
            .and_then(Source::cursor_mut)
        {
            // SAFETY: both pointers belong to this retained immutable window.
            let available = unsafe { cursor.end.offset_from(cursor.next) } as usize;
            if keyword.len() <= available {
                let matched = keyword.iter().enumerate().all(|(index, &byte)| {
                    // SAFETY: index < keyword.len() <= available. Peeking neither
                    // mutates the source nor retires its backing storage.
                    let token = unsafe { cursor.next.add(index).read() };
                    token.0 < 16 << 8 && token.byte().to_ascii_lowercase() == byte
                });
                if matched {
                    // SAFETY: the matched range ends at or before window end.
                    cursor.next = unsafe { cursor.next.add(keyword.len()) };
                    return Ok(true);
                }
            }
        }
        let mut tokens = [Token(0); 8];
        for (index, &byte) in keyword.iter().enumerate() {
            loop {
                let read = self.next_expanded()?;
                // TeX tests character identity before skipping a leading
                // spacer: case changes can give that category a keyword byte.
                if !read.token.is_cs() && read.token.byte().to_ascii_lowercase() == byte {
                    tokens[index] = read.token;
                    break;
                }
                if index == 0 && read.meaning.command == cmd::SPACE {
                    continue;
                }
                tokens[index] = read.token;
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
            if !read.token.is_space() {
                break read;
            }
        };
        if !read.token.is_cs() || read.token == FROZEN_RELAX {
            return Err(Error::Syntax);
        }
        Ok(read.token.id())
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
