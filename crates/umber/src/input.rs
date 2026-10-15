use crate::{Engine, Error, Result, file::FileInput, scalars::Target, token::*};

// Installed token sources, including generated text, share one nesting limit.
const MAX_SOURCES: usize = 10000;

// The innermost token window: the top source's cursor, or the file lookahead
// when no source is installed. Inline sources have none. Taking the two fields
// separately leaves the rest of the engine borrowable.
#[inline(always)]
pub(crate) fn token_window<'s>(
    sources: &'s mut [Source],
    file: &'s mut FileInput,
) -> Option<(&'s mut *const Token, *const Token)> {
    match sources.last_mut() {
        None => Some((&mut file.next, file.end)),
        Some(source) => source.cursor_mut().map(|cursor| (&mut cursor.next, cursor.end)),
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
                let file = &mut self.file;
                let token = if file.next != file.end {
                    // SAFETY: the lookahead window holds initialized tokens
                    // until refill or rewind replaces it.
                    let token = unsafe { file.next.read() };
                    file.next = unsafe { file.next.add(1) };
                    token
                } else {
                    self.refill()?
                };
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
    #[inline(never)]
    fn refill(&mut self) -> Result<Token> {
        let token = self.file.refill(
            self.scalars.catcodes(),
            self.scalars[Target::ENDLINE].value,
            &mut self.names,
            self.par,
        )?;
        // Only the committed lexer inserts names.
        if self.names.len() > self.bindings.len() {
            self.grow_bindings(self.names.len());
        }
        Ok(token)
    }
    // File lookahead is discarded before a category write takes effect, while
    // the tables that produced it remain in force.
    #[inline(never)]
    pub(crate) fn set_category(&mut self, target: Target, value: Integer) {
        self.file
            .rewind(self.scalars.catcodes(), &mut self.names, self.par);
        self.scalars[target] = value;
    }
    #[inline(always)]
    pub(crate) fn token_window(&mut self) -> Option<(&mut *const Token, *const Token)> {
        token_window(&mut self.sources, &mut self.file)
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
    pub(crate) fn push_inline(&mut self, tokens: [Token; 2], len: u8, suppress: bool) {
        self.sources.push(Source::Inline {
            tokens,
            position: 0,
            len,
            suppress,
        });
    }
    #[inline]
    pub(crate) fn back(&mut self, token: Token) {
        self.push_inline([token, Token(0)], 1, false);
    }
    pub(crate) fn insert_pair(&mut self, first: Token, second: Token) {
        self.push_inline([first, second], 2, false);
    }
}
