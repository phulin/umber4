use crate::{Engine, Error, Result, names::Names, token::*};

// Installed token sources, including generated text, share one nesting limit.
const MAX_SOURCES: usize = 10000;

pub(crate) struct FileInput<'a> {
    bytes: &'a [u8],
    offset: usize,
    pub line: Vec<u8>,
    position: usize,
    state: u8,
    loaded: bool,
}
impl<'a> FileInput<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            offset: 0,
            line: Vec::new(),
            position: 0,
            state: 0,
            loaded: false,
        }
    }
    #[cfg_attr(not(feature = "native-profile"), inline(never))]
    fn load(&mut self, endline: i32) -> Result<()> {
        if self.loaded && self.offset == self.bytes.len() {
            return Err(Error::Eof);
        }
        self.loaded = true;
        self.line.clear();
        let start = self.offset;
        let remaining = &self.bytes[start..];
        let mut offset =
            start + memchr::memchr2(b'\n', b'\r', remaining).unwrap_or(remaining.len());
        let mut end = offset;
        while end > start && self.bytes[end - 1] == b' ' {
            end -= 1;
        }
        self.line.extend_from_slice(&self.bytes[start..end]);
        if offset < self.bytes.len() {
            let b = self.bytes[offset];
            offset += 1;
            if b == b'\r' && self.bytes.get(offset) == Some(&b'\n') {
                offset += 1;
            }
        }
        self.offset = offset;
        if (0..=255).contains(&endline) {
            self.line.push(endline as u8);
        }
        self.position = 0;
        self.state = 0;
        Ok(())
    }
    #[inline(always)]
    fn classified(&mut self, cats: &[Integer; 256]) -> (u8, u8) {
        let byte = self.line[self.position];
        let cat = cats[byte as usize].value as u8;
        if cat != 7 {
            return (byte, cat);
        }
        let byte = self.translated(cats);
        (byte, cats[byte as usize].value as u8)
    }
    #[inline(never)]
    fn translated(&mut self, cats: &[Integer; 256]) -> u8 {
        let p = self.position;
        loop {
            let b = self.line[p];
            if cats[b as usize].value != 7
                || self.line.get(p + 1) != Some(&b)
                || p + 2 >= self.line.len()
            {
                return b;
            }
            let hex = |c: u8| match c {
                b'0'..=b'9' => Some(c - b'0'),
                b'a'..=b'f' => Some(c - b'a' + 10),
                _ => None,
            };
            let c = self.line[p + 2];
            let (decoded, end) =
                if let (Some(a), Some(b)) = (hex(c), self.line.get(p + 3).copied().and_then(hex)) {
                    (a * 16 + b, p + 4)
                } else if c < 128 {
                    (c ^ 64, p + 3)
                } else {
                    return b;
                };
            self.line[p] = decoded;
            self.line.drain(p + 1..end);
        }
    }
    #[inline]
    pub fn read_character(&mut self, cats: &[Integer; 256]) -> Option<Token> {
        self.read_character_except::<false>(cats, Token(u32::MAX))
    }
    pub fn position(&self) -> usize {
        self.position
    }
    #[inline]
    pub fn literal_digit(&mut self, cats: &[Integer; 256]) -> Option<i32> {
        let &byte = self.line.get(self.position)?;
        let digit = byte.wrapping_sub(b'0');
        if digit >= 10 || cats[byte as usize].value != 12 {
            return None;
        }
        self.position += 1;
        self.state = 1;
        Some(i32::from(digit))
    }
    #[inline]
    pub fn read_character_except<const EXCEPT: bool>(
        &mut self,
        cats: &[Integer; 256],
        except: Token,
    ) -> Option<Token> {
        if let Some(&byte) = self.line.get(self.position) {
            let cat = cats[byte as usize].value as u8;
            if matches!(cat, 11 | 12) {
                let token = Token::character(cat, byte);
                if EXCEPT && token == except {
                    return None;
                }
                self.position += 1;
                self.state = 1;
                return Some(token);
            }
        }
        None
    }
    pub fn read(
        &mut self,
        cats: &[Integer; 256],
        endline: i32,
        names: &mut Names,
        par: Token,
    ) -> Result<Token> {
        loop {
            if self.position == self.line.len() {
                self.load(endline)?;
                continue;
            }
            let (b, cat) = self.classified(cats);
            self.position += 1;
            match cat {
                0 => {
                    if self.position == self.line.len() {
                        self.state = 2;
                        return Ok(Token::cs(NULL_CS));
                    }
                    let (first, cat) = self.classified(cats);
                    if cat == 11 {
                        let start = self.position;
                        let mut hash = Names::hash_byte(Names::HASH_START, first);
                        self.position += 1;
                        while self.position < self.line.len() {
                            let (byte, cat) = self.classified(cats);
                            if cat != 11 {
                                break;
                            }
                            hash = Names::hash_byte(hash, byte);
                            self.position += 1;
                        }
                        self.state = 2;
                        return Ok(if self.position - start == 1 {
                            Token::single(first)
                        } else {
                            Token::cs(names.intern_hashed(&self.line[start..self.position], hash)?)
                        });
                    }
                    self.position += 1;
                    self.state = if cat == 10 { 2 } else { 1 };
                    return Ok(Token::single(first));
                }
                5 => {
                    self.position = self.line.len();
                    let old = self.state;
                    self.state = 0;
                    if old == 0 {
                        return Ok(par);
                    }
                    if old == 1 {
                        return Ok(Token::SPACE);
                    }
                }
                9 => {}
                10 => {
                    let old = self.state;
                    self.state = 2;
                    if old == 1 {
                        return Ok(Token::SPACE);
                    }
                }
                13 => {
                    self.state = 1;
                    return Ok(Token::active(b));
                }
                14 => {
                    self.position = self.line.len();
                }
                15 => return Err(Error::Syntax),
                _ => {
                    self.state = 1;
                    return Ok(Token::character(cat, b));
                }
            }
        }
    }
}

impl Engine<'_> {
    #[inline(always)]
    pub(crate) fn check_source_limit(&self) -> Result<()> {
        if self.sources.len() > MAX_SOURCES {
            return Err(Error::Capacity);
        }
        Ok(())
    }
    // Debug scanner recursion keeps a reader call boundary instead of
    // duplicating its unoptimized temporaries in every expansion frame.
    #[cfg_attr(not(debug_assertions), inline(always))]
    pub(crate) fn next_raw(&mut self) -> Result<Raw> {
        loop {
            let Some(source) = self.sources.last_mut() else {
                let token = self.file.read(
                    self.scalars.catcodes(),
                    self.scalars[crate::scalars::Target::ENDLINE].value,
                    &mut self.names,
                    self.par,
                )?;
                if self.names.len() > self.bindings.len() {
                    self.grow_bindings(self.names.len());
                }
                return Ok(Raw {
                    token,
                    origin: None,
                    suppress: false,
                });
            };
            if let Source::Inline {
                tokens,
                position,
                len,
                suppress,
            } = source
            {
                if *position == *len {
                    self.pop_source();
                    continue;
                }
                let token = tokens[*position as usize];
                *position += 1;
                let suppressed = *suppress;
                *suppress = false;
                return Ok(Raw {
                    token,
                    origin: None,
                    suppress: suppressed,
                });
            }
            let window = source.cursor_mut().unwrap();
            if window.next == window.end {
                self.advance_source()?;
                continue;
            }
            // SAFETY: standalone sources retain their immutable value, and
            // argument windows borrow the packet below them.
            // Capture retains backing before the next read can retire a source.
            let token = unsafe { window.next.read() };
            window.next = unsafe { window.next.add(1) };
            if token.is_parameter() {
                let packet = window.owns_packet.ok_or(Error::Syntax)?;
                let argument = token.byte() - 1;
                if let Some(raw) = self.single_argument(packet, argument) {
                    return Ok(raw);
                }
                self.push_argument(packet, argument)?;
                continue;
            }
            // SAFETY: pointers belong to the same retained initialized window.
            // Resolved readers discard these coordinates; capture uses them.
            let position =
                window.end_position - unsafe { window.end.offset_from(window.next) } as u32 - 1;
            return Ok(Raw {
                token,
                origin: Some(Span {
                    store: window.store,
                    start: position,
                    end: position + 1,
                }),
                suppress: false,
            });
        }
    }
    #[inline(always)]
    pub(crate) fn single_argument(&self, packet: u32, argument: u8) -> Option<Raw> {
        let sequence = &self.packets[packet as usize].arguments[argument as usize];
        let span = sequence.first;
        if span.end - span.start != 1 || !sequence.rest.is_empty() {
            return None;
        }
        if let Some(token) = span.single_token() {
            return Some(Raw {
                token,
                origin: None,
                suppress: false,
            });
        }
        let token = self.values.token(span.store, span.start);
        // Argument tokens contain no internal parameter markers: their scanner
        // already substituted them. The body keeps this backing alive until the
        // next read; a child capture retains it before that read can retire it.
        Some(Raw {
            token,
            origin: Some(span),
            suppress: false,
        })
    }
    fn span_for(&self, argument: Option<(u32, u8)>, index: usize) -> Option<Span> {
        let (packet, arg) = argument?;
        self.packets[packet as usize].arguments[arg as usize].span(index)
    }
    pub(crate) fn push_argument(&mut self, packet: u32, argument: u8) -> Result<()> {
        let sequence = Some((packet, argument));
        let Some(span) = self.span_for(sequence, 0) else {
            return Ok(());
        };
        // Singleton arguments are returned by single_argument before this call.
        // The body below this cursor owns the complete argument packet.
        self.push_span(sequence, span, None, None)
    }
    #[inline]
    pub(crate) fn push_body(
        &mut self,
        owner: crate::values::Owner,
        packet: Option<u32>,
    ) -> Result<()> {
        let body = owner.definition().body;
        let span = Span {
            store: owner.id,
            start: body.start,
            end: body.end,
        };
        if span.empty() {
            if let Some(packet) = packet {
                self.release_packet(packet);
            }
            return Ok(());
        }
        self.push_span(None, span, packet, Some(owner))
    }
    pub(crate) fn push_tokens(&mut self, owner: crate::values::Owner, start: u32) -> Result<()> {
        let span = Span {
            store: owner.id,
            start,
            end: owner.tokens.len,
        };
        if span.empty() {
            return Ok(());
        }
        self.push_span(None, span, None, Some(owner))
    }
    #[inline(always)]
    fn push_span(
        &mut self,
        argument: Option<(u32, u8)>,
        span: Span,
        owns_packet: Option<u32>,
        owner: Option<crate::values::Owner>,
    ) -> Result<()> {
        self.check_source_limit()?;
        let (next, end, window_end_position) = match &owner {
            Some(value) => value.tokens.window(span.start, span.end),
            _ => self.values.window(span, span.start),
        };
        self.sources.push(Source::Tokens(Cursor {
            next,
            end,
            end_position: window_end_position,
            store: span.store,
            _owner: owner,
            argument,
            span_index: 0,
            span_end: span.end,
            owns_packet,
        }));
        Ok(())
    }
    pub(crate) fn advance_source(&mut self) -> Result<()> {
        let Some(Source::Tokens(cursor)) = self.sources.last() else {
            return Err(Error::Syntax);
        };
        let (mut span, mut index, mut position) = (
            Span {
                store: cursor.store,
                start: cursor.end_position,
                end: cursor.span_end,
            },
            cursor.span_index,
            cursor.end_position,
        );
        if position == span.end {
            index += 1;
            let Some(next) = self.span_for(cursor.argument, index) else {
                self.pop_source();
                return Ok(());
            };
            span = next;
            position = span.start;
        }
        let (next, end, end_position) = self.values.window(span, position);
        if let Some(Source::Tokens(cursor)) = self.sources.last_mut() {
            cursor.next = next;
            cursor.end = end;
            cursor.end_position = end_position;
            cursor.store = span.store;
            cursor.span_end = span.end;
            cursor.span_index = index;
        }
        Ok(())
    }
    pub(crate) fn pop_source(&mut self) {
        if let Some(Source::Tokens(cursor)) = self.sources.pop()
            && let Some(packet) = cursor.owns_packet
        {
            self.release_packet(packet);
        }
    }
    pub(crate) fn retire_exhausted(&mut self) {
        loop {
            let done = match self.sources.last() {
                Some(Source::Inline { position, len, .. }) => position == len,
                Some(Source::Tokens(c)) => {
                    c.next == c.end
                        && c.end_position == c.span_end
                        && self.span_for(c.argument, c.span_index + 1).is_none()
                }
                None => false,
            };
            if !done {
                break;
            }
            self.pop_source();
        }
    }
    #[inline]
    pub(crate) fn back(&mut self, token: Token) {
        self.sources.push(Source::Inline {
            tokens: [token, Token(0)],
            position: 0,
            len: 1,
            suppress: false,
        });
    }
    pub(crate) fn insert_pair(&mut self, first: Token, second: Token) {
        self.sources.push(Source::Inline {
            tokens: [first, second],
            position: 0,
            len: 2,
            suppress: false,
        });
    }
}
