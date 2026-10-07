use crate::{Engine, Error, Result, token::*};

pub(crate) enum Internal {
    Integer(i32),
    Tokens(Span),
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
            Ok(Internal::Tokens(self.token_registers[index].span))
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
                            Internal::Tokens(span) => {
                                for position in span.start..span.end {
                                    emit(self, self.arena.get(position))?;
                                }
                            }
                            Internal::Integer(value) => self.insert_number(value)?,
                        },
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
    // These destination emitters append only: they preserve sources and the
    // immutable window backing. A new output chunk can move arena descriptors,
    // while the boxed input chunk itself stays at its original address.
    #[inline]
    fn emit_characters(
        &mut self,
        emit: &mut impl FnMut(&mut Self, Token) -> Result<()>,
    ) -> Result<()> {
        let Some(Source::Tokens(cursor)) = self.sources.last() else {
            return Ok(());
        };
        let (mut next, end) = (cursor.next, cursor.end);
        while next != end {
            // SAFETY: this installed window contains initialized immutable
            // tokens; its source retains the backing throughout emission.
            let token = unsafe { next.read() };
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
    pub(crate) fn store_body(&mut self) -> Result<Span> {
        let start = self.arena.len;
        self.scan_raw_body(|engine, token| engine.arena.push(token))?;
        Ok(Span {
            store: DEFS,
            start,
            end: self.arena.len,
        })
    }
}
