use crate::{Engine, Error, Result, token::*};
use std::rc::Rc;

impl Engine<'_> {
    pub(crate) fn define(&mut self, expanded: bool, long: bool, global: bool) -> Result<()> {
        let cs = self.scan_cs()?;
        let signature_start = self.arena.len;
        let mut prefix_end = None;
        let mut delimiter_start = self.arena.len;
        let mut delimiters = Vec::new();
        let mut parameters = 0u8;
        let mut brace_tail = None;
        loop {
            let read = self.next_token()?;
            if read.raw.token.is_begin() {
                break;
            }
            if read.raw.token.is_end() {
                return Err(Error::Syntax);
            }
            if read.meaning.command == 6 {
                let next = self.next_token()?;
                if next.raw.token.is_begin() {
                    self.arena.push(next.raw.token);
                    brace_tail = Some(next.raw.token);
                    break;
                }
                if parameters == 9 || next.raw.token != Token::other(b'1' + parameters) {
                    return Err(Error::Syntax);
                }
                if parameters == 0 {
                    prefix_end = Some(self.arena.len);
                } else {
                    delimiters.push(Span {
                        store: DEFS,
                        start: delimiter_start,
                        end: self.arena.len,
                    });
                }
                self.arena.push(Token(PARAM | read.meaning.value as u32));
                parameters += 1;
                delimiter_start = self.arena.len;
            } else {
                self.arena.push(read.raw.token);
            }
        }
        if parameters > 0 {
            delimiters.push(Span {
                store: DEFS,
                start: delimiter_start,
                end: self.arena.len,
            });
        }
        let prefix = Span {
            store: DEFS,
            start: signature_start,
            end: prefix_end.unwrap_or(self.arena.len),
        };
        self.arena.push(Token(PARAM | 255));
        let body_start = self.arena.len;
        let mut depth = 1usize;
        loop {
            let read = if expanded {
                self.next_expanded()?
            } else {
                self.next_token()?
            };
            if read.raw.token.is_begin() {
                depth += 1;
            }
            if read.raw.token.is_end() {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            if read.meaning.command == 6 {
                let next = if expanded {
                    self.next_expanded()?
                } else {
                    self.next_token()?
                };
                if next.meaning.command == 6 {
                    self.arena.push(next.raw.token);
                } else if !next.raw.token.is_cs()
                    && next.raw.token.cat() == 12
                    && (b'1'..b'1' + parameters).contains(&next.raw.token.byte())
                {
                    self.arena
                        .push(Token(PARAM | u32::from(next.raw.token.byte() - b'0')));
                } else {
                    return Err(Error::Syntax);
                }
            } else {
                self.arena.push(read.raw.token);
            }
        }
        if let Some(token) = brace_tail {
            self.arena.push(token);
        }
        let body = Span {
            store: DEFS,
            start: body_start,
            end: self.arena.len,
        };
        let signature = Span {
            store: DEFS,
            start: signature_start,
            end: self.arena.len,
        };
        let failures = delimiters
            .iter()
            .map(|span| {
                let len = (span.end - span.start) as usize;
                let mut failure = vec![0; len];
                let mut matched = 0;
                for index in 1..len {
                    let token = self.arena.get(span.start + index as u32);
                    while matched > 0 && self.arena.get(span.start + matched as u32) != token {
                        matched = failure[matched - 1];
                    }
                    if self.arena.get(span.start + matched as u32) == token {
                        matched += 1;
                    }
                    failure[index] = matched;
                }
                failure
            })
            .collect();
        let id = self.definitions.len() as u32;
        self.definitions.push(Definition {
            prefix,
            delimiters,
            body,
            signature,
            long,
            failures,
        });
        self.assign_meaning(
            cs,
            Meaning {
                command: cmd::MACRO,
                value: id as i32,
            },
            global,
        );
        Ok(())
    }
    pub(crate) fn take_packet(&mut self, count: usize) -> u32 {
        let id = if let Some(id) = self.free_packets.pop() {
            id
        } else {
            let id = self.packets.len() as u32;
            self.packets.push(Packet::default());
            id
        };
        self.packets[id as usize]
            .arguments
            .resize_with(count, Sequence::default);
        id
    }
    pub(crate) fn release_packet(&mut self, id: u32) {
        self.packets[id as usize].clear();
        self.free_packets.push(id);
    }
    fn capture_buffer(&mut self) -> u32 {
        if let Some((id, capture)) = self
            .captures
            .iter_mut()
            .enumerate()
            .find(|(_, c)| !c.building && Rc::strong_count(&c.tokens) == 1)
        {
            Rc::get_mut(&mut capture.tokens).unwrap().clear();
            capture.building = true;
            return id as u32;
        }
        let id = self.captures.len() as u32;
        self.captures.push(Capture {
            tokens: Rc::new(Vec::with_capacity(16)),
            building: true,
        });
        id
    }
    pub(crate) fn capture_raw(
        &mut self,
        packet: u32,
        argument: usize,
        raw: Raw,
        builder: &mut Option<u32>,
    ) {
        let span = if let Some(span) = raw.origin {
            if span.store != DEFS
                && !self.packets[packet as usize]
                    .owners
                    .iter()
                    .any(|(id, _)| *id == span.store)
            {
                let owner = Rc::clone(&self.captures[span.store as usize].tokens);
                self.packets[packet as usize]
                    .owners
                    .push((span.store, owner));
            }
            span
        } else {
            let id = *builder.get_or_insert_with(|| self.capture_buffer());
            let tokens = Rc::get_mut(&mut self.captures[id as usize].tokens).unwrap();
            let start = tokens.len() as u32;
            tokens.push(raw.token);
            Span {
                store: id,
                start,
                end: start + 1,
            }
        };
        self.packets[packet as usize].arguments[argument].append(span);
    }
    pub(crate) fn finish_capture(&mut self, packet: u32, builder: Option<u32>) {
        if let Some(id) = builder {
            self.captures[id as usize].building = false;
            self.packets[packet as usize]
                .owners
                .push((id, Rc::clone(&self.captures[id as usize].tokens)));
        }
    }
    pub(crate) fn begin_macro(&mut self, id: u32) -> Result<()> {
        let prefix = self.definitions[id as usize].prefix;
        for position in prefix.start..prefix.end {
            let expected = self.arena.get(position);
            if self.next_raw()?.token != expected {
                return Err(Error::Argument);
            }
        }
        let count = self.definitions[id as usize].delimiters.len();
        if count == 0 {
            self.retire_exhausted();
            return self.push_sequence(SequenceRef::Body(id), None, None);
        }
        let packet = self.take_packet(count);
        let mut builder = None;
        let long = self.definitions[id as usize].long;
        for argument in 0..count {
            let delimiter = self.definitions[id as usize].delimiters[argument];
            if delimiter.empty() {
                let first = loop {
                    let raw = self.next_raw()?;
                    if !raw.token.is_space() {
                        break raw;
                    }
                };
                if first.token.is_end() {
                    return Err(Error::Argument);
                }
                if !long && first.token == self.par {
                    return Err(Error::Argument);
                }
                if first.token.is_begin() {
                    let mut depth = 1;
                    loop {
                        let raw = self.next_raw()?;
                        if !long && raw.token == self.par {
                            return Err(Error::Argument);
                        }
                        if raw.token.is_begin() {
                            depth += 1;
                        }
                        if raw.token.is_end() {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        self.capture_raw(packet, argument, raw, &mut builder);
                    }
                } else {
                    self.capture_raw(packet, argument, first, &mut builder);
                }
            } else {
                let length = (delimiter.end - delimiter.start) as usize;
                let mut matched = 0;
                let mut depth = 0;
                loop {
                    let raw = self.next_raw()?;
                    self.capture_raw(packet, argument, raw, &mut builder);
                    if depth == 0 {
                        while matched > 0
                            && self.arena.get(delimiter.start + matched as u32) != raw.token
                        {
                            matched = self.definitions[id as usize].failures[argument][matched - 1];
                        }
                        if self.arena.get(delimiter.start + matched as u32) == raw.token {
                            matched += 1;
                        }
                        if matched == length {
                            break;
                        }
                    }
                    // Tokens previously matched against a delimiter become
                    // argument content without another par check on fallback.
                    // The current token is checked only when it is unmatched.
                    if !long && raw.token == self.par && (depth > 0 || matched == 0) {
                        return Err(Error::Argument);
                    }
                    if raw.token.is_begin() {
                        depth += 1;
                    }
                    if raw.token.is_end() {
                        if depth == 0 {
                            return Err(Error::Argument);
                        }
                        depth -= 1;
                    }
                }
                self.packets[packet as usize].arguments[argument].trim_back(length);
                self.strip_outer_group(packet, argument);
            }
        }
        self.finish_capture(packet, builder);
        self.retire_exhausted();
        self.push_sequence(SequenceRef::Body(id), Some(packet), Some(packet))
    }
    fn strip_outer_group(&mut self, packet: u32, argument: usize) {
        let seq = &self.packets[packet as usize].arguments[argument];
        if seq.first.empty() || !self.token_at(seq.first.store, seq.first.start).is_begin() {
            return;
        }
        let length = seq.len();
        let mut seen = 0;
        let mut depth = 0;
        for span in std::iter::once(&seq.first).chain(&seq.rest) {
            for p in span.start..span.end {
                let token = self.token_at(span.store, p);
                seen += 1;
                if token.is_begin() {
                    depth += 1;
                }
                if token.is_end() {
                    depth -= 1;
                }
                if depth == 0 {
                    if seen == length {
                        let seq = &mut self.packets[packet as usize].arguments[argument];
                        seq.trim_front();
                        seq.trim_back(1);
                    }
                    return;
                }
            }
        }
    }
    pub(crate) fn same_meaning(&self, a: Meaning, b: Meaning) -> bool {
        if a == b {
            return true;
        }
        if a.command != b.command || a.command != cmd::MACRO {
            return false;
        }
        let a = &self.definitions[a.value as usize];
        let b = &self.definitions[b.value as usize];
        a.long == b.long
            && a.signature.end - a.signature.start == b.signature.end - b.signature.start
            && (0..a.signature.end - a.signature.start).all(|p| {
                self.arena.get(a.signature.start + p) == self.arena.get(b.signature.start + p)
            })
    }
}
