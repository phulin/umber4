use crate::{Engine, Error, Result, token::*, values::Builder};

pub(crate) enum Internal {
    Quantity(crate::quantities::Quantity),
    Tokens(Option<crate::values::Owner>),
    Font(Token),
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

// Window runs up to this length copy word by word; past it, a wide scan and
// bulk copy repay their call.
const SHORT_RUN: usize = 16;

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

// SAFETY: next and stop lie within one retained immutable window.
#[inline(always)]
unsafe fn next_body_character(next: &mut *const Token, stop: *const Token) -> Option<Token> {
    if *next == stop {
        return None;
    }
    // SAFETY: next precedes stop in the window.
    let token = unsafe { next.read() };
    if !body_character(token) {
        return None;
    }
    // SAFETY: next advances at most to stop.
    *next = unsafe { next.add(1) };
    Some(token)
}

// SAFETY: next and end delimit one retained immutable window. Words are
// tested four at a time, then singly; returns the end of the run copied.
#[inline(never)]
unsafe fn copy_body_run(
    next: *const Token,
    end: *const Token,
    tokens: &mut Builder,
) -> Result<*const Token> {
    let mut run_end = next;
    // SAFETY: unequal endpoints belong to the same retained allocation.
    while unsafe { end.offset_from(run_end) } >= 4 {
        // SAFETY: four initialized immutable words remain in this window.
        let words = unsafe { run_end.cast::<[Token; 4]>().read() };
        // Spaces, letters, and other characters form one range; a block of
        // them needs no exact test.
        let other = words.map(|token| u32::from(token.0.wrapping_sub(10 << 8) >= 3 << 8));
        if other[0] | other[1] | other[2] | other[3] != 0 {
            let stops = words.map(|token| u32::from(!body_character(token)));
            if stops[0] | stops[1] | stops[2] | stops[3] != 0 {
                break;
            }
        }
        // SAFETY: all four words precede end in the same window.
        run_end = unsafe { run_end.add(4) };
    }
    // SAFETY: each word is read only before end.
    while run_end != end && body_character(unsafe { run_end.read() }) {
        // SAFETY: run_end advances at most to end.
        run_end = unsafe { run_end.add(1) };
    }
    // SAFETY: the run lies within the window, which the builder copies into
    // its own reservation.
    tokens
        .extend(unsafe { std::slice::from_raw_parts(next, run_end.offset_from(next) as usize) })?;
    Ok(run_end)
}

impl<'a> Engine<'a> {
    pub(crate) fn toks_index(&mut self, read: Read) -> Result<usize> {
        match read.meaning.command {
            cmd::TOKS => self.scan_index(),
            cmd::TOKS_REF => Ok(read.meaning.value as usize),
            cmd::TOKS_PARAM => Ok(256 + read.meaning.value as usize),
            _ => Err(Error::Syntax),
        }
    }
    pub(crate) fn the_value(&mut self) -> Result<Internal> {
        let read = self.next_expanded()?;
        if matches!(
            read.meaning.command,
            cmd::FONT | cmd::SELECT_FONT | cmd::FONT_FAMILY
        ) {
            return Ok(Internal::Font(Token::font(self.font_ident(read)?)?));
        }
        if matches!(
            read.meaning.command,
            cmd::TOKS | cmd::TOKS_REF | cmd::TOKS_PARAM
        ) {
            let index = self.toks_index(read)?;
            Ok(Internal::Tokens(
                self.document.token_registers[index].owner.clone(),
            ))
        } else {
            self.quantity(read)?
                .map(Internal::Quantity)
                .ok_or(Error::Number)
        }
    }
    // The opening brace has already been consumed. Both emitters write in their
    // final destination; progress and the emitter live on the ordinary Rust stack.
    pub(crate) fn scan_body<const EXPANDED: bool>(
        &mut self,
        parameters: Option<u8>,
        emit: &mut impl Emit<'a>,
    ) -> Result<bool> {
        let scanner = if parameters.is_some() {
            crate::scanner::Scanner::Defining
        } else {
            crate::scanner::Scanner::Absorbing
        };
        self.with_scanner(scanner, |engine| {
            engine.scan_body_inner::<EXPANDED>(parameters, emit)
        })
    }
    fn scan_body_inner<const EXPANDED: bool>(
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
                    if !read.meaning.expandable() || read.meaning.is_protected_macro() {
                        break read;
                    }
                    match read.meaning.command {
                        cmd::THE => match self.the_value()? {
                            Internal::Font(token) => emit.token(self, token)?,
                            Internal::Tokens(owner) => {
                                if let Some(owner) = owner {
                                    for token in owner.tokens.iter() {
                                        emit.token(self, token)?;
                                    }
                                }
                            }
                            Internal::Quantity(crate::quantities::Quantity::Integer(value)) => {
                                self.emit_number(value, emit)?
                            }
                            Internal::Quantity(value) => {
                                let mut text = [0; 128];
                                let length = crate::quantities::quantity_text(value, &mut text);
                                for &byte in &text[..length] {
                                    emit.token(self, Token::string_character(byte))?;
                                }
                            }
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
                            self.scan_raw_text(emit)?;
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
        self.with_scanner(crate::scanner::Scanner::Absorbing, |engine| {
            engine.scan_raw_body_inner(emit)
        })
    }
    pub(crate) fn scan_raw_text(&mut self, emit: &mut impl Emit<'a>) -> Result<()> {
        self.with_scanner(crate::scanner::Scanner::Absorbing, |engine| {
            engine.opening_brace()?;
            engine.scan_raw_body_inner(emit)
        })
    }
    fn scan_raw_body_inner(&mut self, emit: &mut impl Emit<'a>) -> Result<()> {
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
        if self.document.sources.is_empty() {
            if let Some(tokens) = emit.builder() {
                let cats = self.document.scalars.catcodes();
                return self
                    .document
                    .file
                    .read_characters::<false>(cats, Token(u32::MAX), tokens);
            }
            while let Some(token) = self
                .document
                .file
                .read_character(self.document.scalars.catcodes())
            {
                emit.token(self, token)?;
            }
            return Ok(());
        }
        let Some(cursor) = self.document.sources.last().and_then(Source::cursor) else {
            return Ok(());
        };
        let (mut next, end) = (cursor.next, cursor.end);
        // SAFETY: this installed window contains initialized immutable tokens;
        // its source retains the backing throughout emission.
        let result = match emit.builder() {
            Some(tokens) => unsafe {
                // Short runs copy word by word. One that continues past them
                // is located by a wide scan and copied in bulk.
                let stop = if end.offset_from(next) > SHORT_RUN as isize {
                    next.add(SHORT_RUN)
                } else {
                    end
                };
                let mut result = tokens.extend_with(|| next_body_character(&mut next, stop));
                if next == stop && stop != end && result.is_ok() {
                    result = copy_body_run(next, end, tokens).map(|run_end| next = run_end);
                }
                result
            },
            None => std::iter::from_fn(|| unsafe { next_body_character(&mut next, end) })
                .try_for_each(|token| emit.token(self, token)),
        };
        if let Some(cursor) = self
            .document
            .sources
            .last_mut()
            .and_then(Source::cursor_mut)
        {
            cursor.next = next;
        }
        result
    }
    pub(crate) fn store_output_body(&mut self) -> Result<Option<crate::values::Owner>> {
        let mut tokens = self.values.builder(crate::values::Class::Durable)?;
        tokens.push(Token::character(1, b'{'))?;
        self.scan_raw_body(&mut tokens)?;
        if tokens.len() == 1 {
            return Ok(None);
        }
        tokens.push(Token::character(2, b'}'))?;
        Ok(Some(tokens.finish()))
    }
    pub(crate) fn store_body(&mut self) -> Result<crate::values::Owner> {
        let mut tokens = self.values.builder(crate::values::Class::Durable)?;
        self.scan_raw_body(&mut tokens)?;
        Ok(tokens.finish())
    }
}
