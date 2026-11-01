//! Ordered file boundaries and retained token-source cursors.
use crate::{Engine, Error, Result, token::*};

// Installed token sources, including files and generated text, share one limit.
const MAX_SOURCES: usize = 10000;

impl Engine<'_> {
    #[inline(always)]
    pub(crate) fn check_source_limit(&self) -> Result<()> {
        if self.document.sources.len() > MAX_SOURCES {
            return Err(Error::Capacity);
        }
        Ok(())
    }
    #[inline]
    pub(crate) fn next_raw(&mut self) -> Result<Raw> {
        let raw = self.next_raw_with::<true>()?;
        self.check_raw_outer(raw)?;
        Ok(raw)
    }
    #[inline(always)]
    pub(crate) fn next_token(&mut self) -> Result<Read> {
        let raw = self.next_raw_with::<true>()?;
        Ok(Read {
            token: raw.token,
            meaning: self.raw_meaning(raw)?,
        })
    }
    // Consumers of non-inserting file lookup receive only the effective
    // meaning, so a missing spelling cannot escape as a token identity.
    pub(crate) fn next_lookup_meaning(&mut self) -> Result<Meaning> {
        let raw = self.next_raw_with::<false>()?;
        self.raw_meaning(raw)
    }
    // Debug scanner recursion keeps a reader call boundary instead of
    // duplicating its unoptimized temporaries in every expansion frame.
    #[cfg_attr(not(debug_assertions), inline(always))]
    fn next_raw_with<const INTERN: bool>(&mut self) -> Result<Raw> {
        loop {
            if self.at_file_input() {
                let cats = self.document.scalars.catcodes();
                let endline = self.document.scalars[crate::scalars::Target::ENDLINE].value;
                let result = self.document.file.read_with::<INTERN>(
                    cats,
                    endline,
                    &mut self.names,
                    self.document.par,
                    self.document.force_eof,
                );
                let token = match result {
                    Err(Error::Eof) => {
                        self.document.force_eof = false;
                        let nested =
                            matches!(self.document.sources.last(), Some(Source::FileBoundary));
                        if nested {
                            self.pop_source();
                        }
                        if !nested || self.scanner != crate::scanner::Scanner::Normal {
                            return Err(Error::Eof);
                        }
                        continue;
                    }
                    other => other?,
                };
                if INTERN && self.names.len() > self.document.bindings.len() {
                    self.grow_bindings(self.names.len());
                }
                return Ok(Raw {
                    token,
                    origin: None,
                    suppress: false,
                });
            }
            let source = self.document.sources.last_mut().unwrap();
            if let Source::WriteBoundary { position } = source {
                let token = match *position {
                    0 => Token::character(2, b'}'),
                    1 => FROZEN_END_WRITE,
                    _ => return Err(Error::Syntax),
                };
                *position += 1;
                return Ok(Raw {
                    token,
                    origin: None,
                    suppress: false,
                });
            }
            if let Source::Inline {
                tokens,
                position,
                len,
                suppress,
                ..
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
        self.document.sources.push(Source::Tokens(Cursor {
            role: SourceRole::Other,
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
        let Some(Source::Tokens(cursor)) = self.document.sources.last() else {
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
        if let Some(Source::Tokens(cursor)) = self.document.sources.last_mut() {
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
        match self.document.sources.pop() {
            Some(Source::Tokens(cursor)) => {
                if let Some(packet) = cursor.owns_packet {
                    self.release_packet(packet);
                }
            }
            Some(Source::FileBoundary) => {
                self.document.file = self
                    .document
                    .suspended_files
                    .pop()
                    .expect("paired file boundary");
            }
            _ => {}
        }
    }
    pub(crate) fn current_source_exhausted(&self) -> bool {
        match self.document.sources.last() {
            Some(Source::Inline { position, len, .. }) => position == len,
            Some(Source::Tokens(c)) => {
                c.next == c.end
                    && c.end_position == c.span_end
                    && self.span_for(c.argument, c.span_index + 1).is_none()
            }
            _ => false,
        }
    }
    pub(crate) fn retire_exhausted(&mut self) {
        while self.current_source_exhausted() {
            self.pop_source();
        }
    }
    #[inline]
    pub(crate) fn push_inline(&mut self, tokens: [Token; 2], len: u8, suppress: bool) {
        self.document.sources.push(Source::Inline {
            role: SourceRole::Inserted,
            tokens,
            position: 0,
            len,
            suppress,
        });
    }
    #[inline]
    pub(crate) fn back(&mut self, token: Token) {
        self.push_backed(token, false);
    }
    pub(crate) fn push_backed(&mut self, token: Token, suppress: bool) {
        self.push_inline([token, Token(0)], 1, suppress);
        if let Some(Source::Inline { role, .. }) = self.document.sources.last_mut() {
            *role = SourceRole::BackedUp;
        }
    }
    pub(crate) fn insert_relax(&mut self, original: Token) {
        self.back(original);
        self.push_inline([FROZEN_RELAX, Token(0)], 1, false);
    }
    pub(crate) fn insert_pair(&mut self, first: Token, second: Token) {
        self.push_inline([first, second], 2, false);
    }
}
