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
        let mut offset = start;
        while offset < self.bytes.len() && !matches!(self.bytes[offset], b'\n' | b'\r') {
            offset += 1;
        }
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
                        return Ok(Token::cs(1));
                    }
                    let (first, cat) = self.classified(cats);
                    if cat == 11 {
                        let start = self.position;
                        self.position += 1;
                        while self.position < self.line.len() {
                            let (_, cat) = self.classified(cats);
                            if cat != 11 {
                                break;
                            }
                            self.position += 1;
                        }
                        self.state = 2;
                        return Ok(Token::cs(names.intern(&self.line[start..self.position])?));
                    }
                    self.position += 1;
                    self.state = if cat == 10 { 2 } else { 1 };
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
                    self.scalars.catcodes(),
                    self.scalars[crate::scalars::Target::ENDLINE].value,
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
                    if token.0 & PARAM != 0 && !token.is_cs() {
                        let packet = cursor.macro_packet.ok_or(Error::Syntax)?;
                        let argument = token.byte() - 1;
                        if let Some(raw) = self.single_argument(packet, argument) {
                            return Ok(raw);
                        }
                        self.push_sequence(SequenceRef::Argument(packet, argument), None, None)?;
                        continue;
                    }
                    // SAFETY: next and end belong to the same window. Derive
                    // coordinates only for capture; resolved reads discard this
                    // metadata, leaving a single pointer advance on their path.
                    let position = cursor.window_end_position
                        - unsafe { cursor.end.offset_from(cursor.next) } as u32
                        - 1;
                    let origin = Span {
                        store: cursor.span.store,
                        start: position,
                        end: position + 1,
                    };
                    return Ok(Raw {
                        token,
                        origin: Some(origin),
                        suppress: false,
                    });
                }
            }
        }
    }
    #[inline(always)]
    pub(crate) fn single_argument(&self, packet: u32, argument: u8) -> Option<Raw> {
        let sequence = &self.packets[packet as usize].arguments[argument as usize];
        let span = sequence.first;
        if span.end - span.start != 1 || !sequence.rest.is_empty() {
            return None;
        }
        if span.store == SINGLE {
            return Some(Raw {
                token: Token(span.start),
                origin: None,
                suppress: false,
            });
        }
        let token = if span.store == DEFS {
            self.arena.get(span.start)
        } else {
            self.captures[span.store as usize].tokens[span.start as usize]
        };
        // Argument tokens contain no internal parameter markers: their scanner
        // already substituted them. The body keeps this backing alive until the
        // next read; a child capture retains it before that read can retire it.
        Some(Raw {
            token,
            origin: Some(span),
            suppress: false,
        })
    }
    pub(crate) fn span_for(&self, sequence: SequenceRef, index: usize) -> Option<Span> {
        match sequence {
            SequenceRef::List => None,
            SequenceRef::Body(id) => (index == 0)
                .then_some(self.definitions[id as usize].body)
                .filter(|s| !s.empty()),
            SequenceRef::Argument(packet, arg) => {
                self.packets[packet as usize].arguments[arg as usize].span(index)
            }
        }
    }
    fn window(&self, span: Span, start: u32) -> (*const Token, *const Token, u32) {
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
                    span.end,
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
        if span.store == SINGLE {
            if self.sources.len() > 10000 {
                return Err(Error::Capacity);
            }
            self.back(Token(span.start));
            if let Some(packet) = owns_packet {
                self.release_packet(packet);
            }
            return Ok(());
        }
        self.push_span(sequence, span, owns_packet, macro_packet)
    }
    #[inline]
    pub(crate) fn push_body(&mut self, id: u32, body: Span) -> Result<()> {
        if body.empty() {
            return Ok(());
        }
        if self.sources.len() > 10000 {
            return Err(Error::Capacity);
        }
        // Definition bodies always belong to the immutable token arena.
        let (next, end, window_end_position) = self.arena.window(body.start, body.end);
        self.sources.push(Source::Tokens(Cursor {
            sequence: SequenceRef::Body(id),
            span_index: 0,
            span: body,
            window_end_position,
            next,
            end,
            owns_packet: None,
            macro_packet: None,
        }));
        Ok(())
    }
    pub(crate) fn push_tokens(&mut self, span: Span) -> Result<()> {
        if span.empty() {
            return Ok(());
        }
        self.push_span(SequenceRef::List, span, None, None)
    }
    #[inline]
    fn push_span(
        &mut self,
        sequence: SequenceRef,
        span: Span,
        owns_packet: Option<u32>,
        macro_packet: Option<u32>,
    ) -> Result<()> {
        if self.sources.len() > 10000 {
            return Err(Error::Capacity);
        }
        let (next, end, window_end_position) = self.window(span, span.start);
        self.sources.push(Source::Tokens(Cursor {
            sequence,
            span_index: 0,
            span,
            window_end_position,
            next,
            end,
            owns_packet,
            macro_packet,
        }));
        Ok(())
    }
    pub(crate) fn advance_source(&mut self) -> Result<()> {
        let Some(Source::Tokens(cursor)) = self.sources.last() else {
            return Err(Error::Syntax);
        };
        let (sequence, next_index, old_span, position) = (
            cursor.sequence,
            cursor.span_index + 1,
            cursor.span,
            cursor.window_end_position,
        );
        if position < old_span.end {
            let (next, end, window_end_position) = self.window(old_span, position);
            if let Some(Source::Tokens(c)) = self.sources.last_mut() {
                c.next = next;
                c.end = end;
                c.window_end_position = window_end_position;
            }
        } else if let Some(span) = self.span_for(sequence, next_index) {
            let (next, end, window_end_position) = self.window(span, span.start);
            if let Some(Source::Tokens(c)) = self.sources.last_mut() {
                c.span = span;
                c.window_end_position = window_end_position;
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
                    c.next == c.end
                        && c.window_end_position == c.span.end
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
}
