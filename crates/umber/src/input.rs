use crate::{Engine, Error, Result, token::*};

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
    fn load(&mut self, endline: i32) -> Result<()> {
        if self.loaded && self.offset == self.bytes.len() {
            return Err(Error::Eof);
        }
        self.loaded = true;
        self.line.clear();
        let start = self.offset;
        while self.offset < self.bytes.len() && !matches!(self.bytes[self.offset], b'\n' | b'\r') {
            self.offset += 1;
        }
        let mut end = self.offset;
        while end > start && matches!(self.bytes[end - 1], b' ' | b'\t') {
            end -= 1;
        }
        self.line.extend_from_slice(&self.bytes[start..end]);
        if self.offset < self.bytes.len() {
            let b = self.bytes[self.offset];
            self.offset += 1;
            if b == b'\r' && self.bytes.get(self.offset) == Some(&b'\n') {
                self.offset += 1;
            }
        }
        if (0..=255).contains(&endline) {
            self.line.push(endline as u8);
        }
        self.position = 0;
        self.state = 0;
        Ok(())
    }
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
            let b = self.translated(cats);
            self.position += 1;
            let cat = cats[b as usize].value as u8;
            match cat {
                0 => {
                    if self.position == self.line.len() {
                        self.state = 2;
                        return Ok(Token::cs(1));
                    }
                    let first = self.translated(cats);
                    if cats[first as usize].value == 11 {
                        let start = self.position;
                        self.position += 1;
                        while self.position < self.line.len() {
                            let c = self.translated(cats);
                            if cats[c as usize].value != 11 {
                                break;
                            }
                            self.position += 1;
                        }
                        self.state = 2;
                        return Ok(Token::cs(names.intern(&self.line[start..self.position])));
                    }
                    self.position += 1;
                    self.state = if cats[first as usize].value == 10 {
                        2
                    } else {
                        1
                    };
                    return Ok(Token::cs(2 + u32::from(first)));
                }
                5 => {
                    self.position = self.line.len();
                    let old = self.state;
                    self.state = 0;
                    if old == 0 {
                        return Ok(par);
                    }
                    if old == 1 {
                        return Ok(Token::character(10, b' '));
                    }
                }
                9 => {}
                10 => {
                    let old = self.state;
                    self.state = 2;
                    if old == 1 {
                        return Ok(Token::character(10, b' '));
                    }
                }
                13 => {
                    self.state = 1;
                    return Ok(Token::cs(258 + u32::from(b)));
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
    pub(crate) fn next_raw(&mut self) -> Result<Raw> {
        loop {
            let Some(source) = self.sources.last_mut() else {
                let token = self.file.read(
                    &self.catcodes,
                    self.endline.value,
                    &mut self.names,
                    self.par,
                )?;
                if self.names.spellings.len() > self.bindings.len() {
                    self.bindings
                        .resize(self.names.spellings.len(), Binding::default());
                }
                return Ok(Raw {
                    token,
                    origin: None,
                    suppress: false,
                });
            };
            match source {
                Source::Inline {
                    tokens,
                    position,
                    len,
                    suppress,
                } => {
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
                Source::Tokens(cursor) => {
                    if cursor.next == cursor.end {
                        self.advance_source()?;
                        continue;
                    }
                    // SAFETY: cursor windows cover initialized immutable tokens.
                    // Definition chunks live with Engine. Argument windows are
                    // retained by the owning packet beneath their source; child
                    // capture retains a chunk before the next read can retire it.
                    let token = unsafe { cursor.next.read() };
                    // SAFETY: next < end within the same backing allocation.
                    cursor.next = unsafe { cursor.next.add(1) };
                    let origin = Span {
                        store: cursor.span.store,
                        start: cursor.position,
                        end: cursor.position + 1,
                    };
                    cursor.position += 1;
                    if token.0 & PARAM != 0 && !token.is_cs() {
                        let packet = cursor.macro_packet.ok_or(Error::Syntax)?;
                        let argument = (token.0 & 255) as usize - 1;
                        self.push_sequence(SequenceRef::Argument(packet, argument), None, None)?;
                        continue;
                    }
                    return Ok(Raw {
                        token,
                        origin: Some(origin),
                        suppress: false,
                    });
                }
            }
        }
    }
    pub(crate) fn span_for(&self, sequence: SequenceRef, index: usize) -> Option<Span> {
        match sequence {
            SequenceRef::Body(id) => (index == 0)
                .then_some(self.definitions[id as usize].body)
                .filter(|s| !s.empty()),
            SequenceRef::Argument(packet, arg) => {
                self.packets[packet as usize].arguments[arg].span(index)
            }
        }
    }
    fn window(&self, span: Span, start: u32) -> (*const Token, *const Token) {
        if span.store == DEFS {
            self.arena.window(start, span.end)
        } else {
            let tokens = &self.captures[span.store as usize].tokens;
            assert!(start < span.end && span.end as usize <= tokens.len());
            // SAFETY: packet ownership keeps this vector immutable and allocated.
            unsafe {
                (
                    tokens.as_ptr().add(start as usize),
                    tokens.as_ptr().add(span.end as usize),
                )
            }
        }
    }
    pub(crate) fn push_sequence(
        &mut self,
        sequence: SequenceRef,
        owns_packet: Option<u32>,
        macro_packet: Option<u32>,
    ) -> Result<()> {
        let Some(span) = self.span_for(sequence, 0) else {
            if let Some(packet) = owns_packet {
                self.release_packet(packet);
            }
            return Ok(());
        };
        if self.sources.len() > 10000 {
            return Err(Error::Capacity);
        }
        let (next, end) = self.window(span, span.start);
        self.sources.push(Source::Tokens(Cursor {
            sequence,
            span_index: 0,
            span,
            position: span.start,
            next,
            end,
            owns_packet,
            macro_packet,
        }));
        Ok(())
    }
    fn advance_source(&mut self) -> Result<()> {
        let Some(Source::Tokens(cursor)) = self.sources.last() else {
            return Err(Error::Syntax);
        };
        let (sequence, next_index, old_span, position) = (
            cursor.sequence,
            cursor.span_index + 1,
            cursor.span,
            cursor.position,
        );
        if position < old_span.end {
            let (next, end) = self.window(old_span, position);
            if let Some(Source::Tokens(c)) = self.sources.last_mut() {
                c.next = next;
                c.end = end;
            }
        } else if let Some(span) = self.span_for(sequence, next_index) {
            let (next, end) = self.window(span, span.start);
            if let Some(Source::Tokens(c)) = self.sources.last_mut() {
                c.span = span;
                c.position = span.start;
                c.span_index = next_index;
                c.next = next;
                c.end = end;
            }
        } else {
            self.pop_source();
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
                    c.position == c.span.end
                        && self.span_for(c.sequence, c.span_index + 1).is_none()
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
    pub(crate) fn token_at(&self, store: u32, position: u32) -> Token {
        if store == DEFS {
            self.arena.get(position)
        } else {
            self.captures[store as usize].tokens[position as usize]
        }
    }
}
