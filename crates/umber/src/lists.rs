use crate::{Engine, Error, Result, token::*};

pub(crate) enum Internal {
    Integer(i32),
    Tokens(Option<crate::values::Owner>),
}

impl Engine<'_> {
    pub(crate) fn toks_index(&mut self, read: Read) -> Result<usize> {
        match read.meaning.command {
            cmd::TOKS => self.scan_index(),
            cmd::TOKS_REF => Ok(read.meaning.value as usize),
            _ => Err(Error::Syntax),
        }
    }
    pub(crate) fn the_value(&mut self) -> Result<Internal> {
        let read = self.next_expanded()?;
        if matches!(read.meaning.command, cmd::TOKS | cmd::TOKS_REF) {
            let index = self.toks_index(read)?;
            Ok(Internal::Tokens(self.token_registers[index].owner.clone()))
        } else {
            Ok(Internal::Integer(self.internal_int(read)?))
        }
    }
    // The opening brace has already been consumed. Both emitters write in their
    // final destination; progress and the emitter live on the ordinary Rust stack.
    pub(crate) fn scan_body<const EXPANDED: bool>(
        &mut self,
        parameters: Option<u8>,
        mut emit: impl FnMut(&mut Self, Token) -> Result<()>,
    ) -> Result<bool> {
        let mut depth = 1;
        let mut uses_parameters = false;
        loop {
            self.emit_characters(&mut emit)?;
            let read = if EXPANDED {
                loop {
                    let read = self.next_token()?;
                    if !read.meaning.expandable() || read.meaning.command == cmd::PROTECTED_MACRO {
                        break read;
                    }
                    match read.meaning.command {
                        cmd::THE => match self.the_value()? {
                            Internal::Tokens(owner) => {
                                if let Some(owner) = owner {
                                    for position in 0..owner.tokens.len {
                                        emit(self, owner.tokens.get(position))?;
                                    }
                                }
                            }
                            Internal::Integer(value) => self.emit_number(value, &mut emit)?,
                        },
                        cmd::NUMBER => self.with_expansion(|engine| {
                            let value = engine.scan_int()?;
                            engine.emit_number(value, &mut emit)
                        })?,
                        cmd::STRING | cmd::MEANING | cmd::ROMAN | cmd::DETOKENIZE => {
                            self.with_expansion(|engine| {
                                let mut nonempty = false;
                                engine.conversion::<true>(read, |engine, byte| {
                                    nonempty = true;
                                    let token = if byte == b' ' {
                                        Token::character(10, byte)
                                    } else {
                                        Token::other(byte)
                                    };
                                    emit(engine, token)
                                })?;
                                if nonempty && engine.sources.len() > 10000 {
                                    return Err(Error::Capacity);
                                }
                                Ok(())
                            })?;
                        }
                        cmd::UNEXPANDED => {
                            self.opening_brace()?;
                            self.scan_raw_body(&mut emit)?;
                        }
                        _ if read.meaning.is_macro() => {
                            self.begin_macro(read.meaning.value as u32)?;
                        }
                        _ => self.expand(read)?,
                    }
                }
            } else {
                self.next_token()?
            };
            if read.token.is_begin() {
                depth += 1;
            }
            if read.token.is_end() {
                depth -= 1;
                if depth == 0 {
                    return Ok(uses_parameters);
                }
            }
            if let Some(count) = parameters
                && read.meaning.command == 6
            {
                let next = if EXPANDED {
                    self.next_expanded()?
                } else {
                    self.next_token()?
                };
                if next.meaning.command == 6 {
                    emit(self, next.token)?;
                } else if !next.token.is_cs()
                    && next.token.cat() == 12
                    && (b'1'..b'1' + count).contains(&next.token.byte())
                {
                    emit(self, Token(PARAM | u32::from(next.token.byte() - b'0')))?;
                    uses_parameters = true;
                } else {
                    return Err(Error::Syntax);
                }
            } else {
                emit(self, read.token)?;
            }
        }
    }
    pub(crate) fn scan_raw_body(
        &mut self,
        mut emit: impl FnMut(&mut Self, Token) -> Result<()>,
    ) -> Result<()> {
        let mut depth = 1;
        loop {
            self.emit_characters(&mut emit)?;
            let token = self.next_raw()?.token;
            if token.is_begin() {
                depth += 1;
            }
            if token.is_end() {
                depth -= 1;
                if depth == 0 {
                    return Ok(());
                }
            }
            emit(self, token)?;
        }
    }
    fn emit_number(
        &mut self,
        value: i32,
        emit: &mut impl FnMut(&mut Self, Token) -> Result<()>,
    ) -> Result<()> {
        // Preserve the general expansion path's source limit even though this
        // scanner needs no additional source for the generated characters.
        if self.sources.len() > 10000 {
            return Err(Error::Capacity);
        }
        if value < 0 {
            emit(self, Token::other(b'-'))?;
        }
        self.emit_digits(value.unsigned_abs(), emit)
    }
    fn emit_digits(
        &mut self,
        value: u32,
        emit: &mut impl FnMut(&mut Self, Token) -> Result<()>,
    ) -> Result<()> {
        // At most ten ordinary calls emit directly in reading order. Decimal
        // characters cannot alter brace depth or parameter interpretation.
        if value >= 10 {
            self.emit_digits(value / 10, emit)?;
        }
        emit(self, Token::other(b'0' + (value % 10) as u8))
    }
    // These destination emitters append only: they preserve sources and the
    // immutable window backing. A new output chunk can move arena descriptors,
    // while the boxed input chunk itself stays at its original address.
    #[inline]
    fn emit_characters(
        &mut self,
        emit: &mut impl FnMut(&mut Self, Token) -> Result<()>,
    ) -> Result<()> {
        if self.sources.is_empty() {
            while let Some(token) = self.file.read_character(self.scalars.catcodes()) {
                emit(self, token)?;
            }
            return Ok(());
        }
        let Some(Source::Tokens(cursor)) = self.sources.last() else {
            return Ok(());
        };
        let (mut next, end) = (cursor.next, cursor.end);
        while next != end {
            // SAFETY: this installed window contains initialized immutable
            // tokens; its source retains the backing throughout emission.
            let token = unsafe { next.read() };
            #[cfg(feature = "native-profile")]
            // Space, letter, and other-character words form one common range.
            // The remaining categories retain the structural/parameter check.
            if !(10 << 8..13 << 8).contains(&token.0)
                && (token.0 >= 16 << 8 || matches!(token.cat(), 1 | 2 | 6))
            {
                break;
            }
            #[cfg(not(feature = "native-profile"))]
            if token.0 >= 16 << 8 || matches!(token.cat(), 1 | 2 | 6) {
                break;
            }
            // SAFETY: next is within this window and advances at most to end.
            next = unsafe { next.add(1) };
            let result = emit(self, token);
            if result.is_err() {
                if let Some(Source::Tokens(cursor)) = self.sources.last_mut() {
                    cursor.next = next;
                }
                return result;
            }
        }
        if let Some(Source::Tokens(cursor)) = self.sources.last_mut() {
            cursor.next = next;
        }
        Ok(())
    }
    pub(crate) fn store_body(&mut self) -> Result<crate::values::Owner> {
        let mut tokens = self.values.builder();
        self.scan_raw_body(|_, token| tokens.push(token))?;
        self.values.publish(tokens, None)
    }
}
