use crate::{Engine, Error, Result, token::*, values::Builder};

pub(crate) enum Internal {
    Integer(i32),
    Tokens(Option<crate::values::Owner>),
}

/// A list destination. Token builders take each character run in place;
/// other destinations receive every token in reading order.
pub(crate) trait Emit<'a> {
    fn token(&mut self, engine: &mut Engine<'a>, token: Token) -> Result<()>;
    #[inline(always)]
    fn builder(&mut self) -> Option<&mut Builder> {
        None
    }
}
impl<'a, F: FnMut(&mut Engine<'a>, Token) -> Result<()>> Emit<'a> for F {
    #[inline(always)]
    fn token(&mut self, engine: &mut Engine<'a>, token: Token) -> Result<()> {
        self(engine, token)
    }
}
impl<'a> Emit<'a> for Builder {
    #[inline(always)]
    fn token(&mut self, _: &mut Engine<'a>, token: Token) -> Result<()> {
        self.push(token)
    }
    #[inline(always)]
    fn builder(&mut self) -> Option<&mut Builder> {
        Some(self)
    }
}

// A character that neither changes depth nor introduces a parameter.
#[inline(always)]
fn body_character(token: Token) -> bool {
    // Space, letter, and other-character words form one common range.
    #[cfg(feature = "native-profile")]
    if (10 << 8..13 << 8).contains(&token.0) {
        return true;
    }
    token.0 < 16 << 8 && !matches!(token.cat(), 1 | 2 | 6)
}

impl<'a> Engine<'a> {
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
        emit: &mut impl Emit<'a>,
    ) -> Result<bool> {
        let mut depth = 1;
        let mut uses_parameters = false;
        loop {
            self.emit_characters(emit)?;
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
                                        emit.token(self, owner.tokens.get(position))?;
                                    }
                                }
                            }
                            Internal::Integer(value) => self.emit_number(value, emit)?,
                        },
                        cmd::NUMBER => self.with_expansion(|engine| {
                            let value = engine.scan_int()?;
                            engine.emit_number(value, emit)
                        })?,
                        cmd::STRING | cmd::MEANING | cmd::ROMAN | cmd::DETOKENIZE => {
                            self.with_expansion(|engine| {
                                let mut nonempty = false;
                                engine.conversion::<true>(read, |engine, byte| {
                                    nonempty = true;
                                    emit.token(engine, Token::string_character(byte))
                                })?;
                                if nonempty {
                                    engine.check_source_limit()?;
                                }
                                Ok(())
                            })?;
                        }
                        cmd::UNEXPANDED => {
                            self.opening_brace()?;
                            self.scan_raw_body(emit)?;
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
                    emit.token(self, next.token)?;
                } else if !next.token.is_cs()
                    && next.token.cat() == 12
                    && (b'1'..b'1' + count).contains(&next.token.byte())
                {
                    emit.token(self, Token(PARAM | u32::from(next.token.byte() - b'0')))?;
                    uses_parameters = true;
                } else {
                    return Err(Error::Syntax);
                }
            } else {
                emit.token(self, read.token)?;
            }
        }
    }
    pub(crate) fn scan_raw_body(&mut self, emit: &mut impl Emit<'a>) -> Result<()> {
        let mut depth = 1;
        loop {
            self.emit_characters(emit)?;
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
            emit.token(self, token)?;
        }
    }
    pub(crate) fn emit_number(&mut self, value: i32, emit: &mut impl Emit<'a>) -> Result<()> {
        // Preserve the general expansion path's source limit even though this
        // scanner needs no additional source for the generated characters.
        self.check_source_limit()?;
        if value < 0 {
            emit.token(self, Token::other(b'-'))?;
        }
        self.decimal_digits(value.unsigned_abs(), &mut |engine, byte| {
            emit.token(engine, Token::other(byte))
        })
    }
    // These destination emitters append only: they preserve sources and the
    // immutable window backing. A new output chunk can move arena descriptors,
    // while the boxed input chunk itself stays at its original address.
    #[inline]
    fn emit_characters(&mut self, emit: &mut impl Emit<'a>) -> Result<()> {
        if self.sources.is_empty() {
            if let Some(tokens) = emit.builder() {
                return self.file.read_characters(self.scalars.catcodes(), tokens);
            }
            while let Some(token) = self.file.read_character(self.scalars.catcodes()) {
                emit.token(self, token)?;
            }
            return Ok(());
        }
        let Some(cursor) = self.sources.last().and_then(Source::cursor) else {
            return Ok(());
        };
        let (mut next, end) = (cursor.next, cursor.end);
        let read = || {
            if next == end {
                return None;
            }
            // SAFETY: this installed window contains initialized immutable
            // tokens; its source retains the backing throughout emission.
            let token = unsafe { next.read() };
            if !body_character(token) {
                return None;
            }
            // SAFETY: next is within this window and advances at most to end.
            next = unsafe { next.add(1) };
            Some(token)
        };
        let result = match emit.builder() {
            Some(tokens) => tokens.extend_with(read),
            None => std::iter::from_fn(read).try_for_each(|token| emit.token(self, token)),
        };
        if let Some(cursor) = self.sources.last_mut().and_then(Source::cursor_mut) {
            cursor.next = next;
        }
        result
    }
    pub(crate) fn store_body(&mut self) -> Result<crate::values::Owner> {
        let mut tokens = self.values.builder()?;
        self.scan_raw_body(&mut tokens)?;
        Ok(tokens.finish(None))
    }
}
