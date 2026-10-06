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
            if read.token.is_begin() {
                break;
            }
            if read.token.is_end() {
                return Err(Error::Syntax);
            }
            if read.meaning.command == 6 {
                let next = self.next_token()?;
                if next.token.is_begin() {
                    self.arena.push(next.token)?;
                    brace_tail = Some(next.token);
                    break;
                }
                if parameters == 9 || next.token != Token::other(b'1' + parameters) {
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
                self.arena.push(Token(PARAM | read.meaning.value as u32))?;
                parameters += 1;
                delimiter_start = self.arena.len;
            } else {
                self.arena.push(read.token)?;
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
        self.arena.push(Token(PARAM | 255))?;
        let body_start = self.arena.len;
        let mut depth = 1usize;
        loop {
            let read = if expanded {
                self.next_expanded()?
            } else {
                self.next_token()?
            };
            if read.token.is_begin() {
                depth += 1;
            }
            if read.token.is_end() {
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
                    self.arena.push(next.token)?;
                } else if !next.token.is_cs()
                    && next.token.cat() == 12
                    && (b'1'..b'1' + parameters).contains(&next.token.byte())
                {
                    self.arena
                        .push(Token(PARAM | u32::from(next.token.byte() - b'0')))?;
                } else {
                    return Err(Error::Syntax);
                }
            } else {
                self.arena.push(read.token)?;
            }
        }
        if let Some(token) = brace_tail {
            self.arena.push(token)?;
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
        if self.definitions.len() > i32::MAX as usize {
            return Err(Error::Capacity);
        }
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
    pub(crate) fn take_packet(&mut self, count: usize) -> Result<u32> {
        let id = if let Some(id) = self.free_packets.pop() {
            id
        } else {
            if self.packets.len() >= u32::MAX as usize {
                return Err(Error::Capacity);
            }
            let id = self.packets.len() as u32;
            self.packets.push(Packet::default());
            id
        };
        self.packets[id as usize]
            .arguments
            .resize_with(count, Sequence::default);
        Ok(id)
    }
    pub(crate) fn release_packet(&mut self, id: u32) {
        self.packets[id as usize].clear();
        self.free_packets.push(id);
    }
    fn capture_buffer(&mut self) -> Result<u32> {
        if let Some((id, capture)) = self
            .captures
            .iter_mut()
            .enumerate()
            .find(|(_, c)| !c.building && Rc::strong_count(&c.tokens) == 1)
        {
            Rc::get_mut(&mut capture.tokens).unwrap().clear();
            capture.building = true;
            return Ok(id as u32);
        }
        if self.captures.len() >= DEFS as usize {
            return Err(Error::Capacity);
        }
        let id = self.captures.len() as u32;
        self.captures.push(Capture {
            tokens: Rc::new(Vec::with_capacity(16)),
            building: true,
        });
        Ok(id)
    }
    #[inline(always)]
    pub(crate) fn capture_raw(
        &mut self,
        packet: u32,
        argument: usize,
        raw: Raw,
        builder: &mut Option<u32>,
    ) -> Result<()> {
        let span = if let Some(span) = raw.origin {
            let sequence = &mut self.packets[packet as usize].arguments[argument];
            let last = sequence.rest.last_mut().unwrap_or(&mut sequence.first);
            if !last.empty() && last.store == span.store && last.end == span.start {
                last.end = span.end;
                return Ok(());
            }
            if span.store != DEFS {
                self.retain_capture(packet, span.store);
            }
            span
        } else {
            self.materialize_token(raw.token, builder)?
        };
        self.packets[packet as usize].arguments[argument].append(span);
        Ok(())
    }
    #[inline(never)]
    fn retain_capture(&mut self, packet: u32, id: u32) {
        let owners = &mut self.packets[packet as usize].owners;
        if !owners.iter().any(|(old, _)| *old == id) {
            owners.push((id, Rc::clone(&self.captures[id as usize].tokens)));
        }
    }
    #[inline(never)]
    fn materialize_token(&mut self, token: Token, builder: &mut Option<u32>) -> Result<Span> {
        let id = if let Some(id) = *builder {
            id
        } else {
            let id = self.capture_buffer()?;
            *builder = Some(id);
            id
        };
        let tokens = Rc::get_mut(&mut self.captures[id as usize].tokens).unwrap();
        if tokens.len() >= u32::MAX as usize {
            return Err(Error::Capacity);
        }
        let start = tokens.len() as u32;
        tokens.push(token);
        Ok(Span {
            store: id,
            start,
            end: start + 1,
        })
    }
    pub(crate) fn finish_capture(&mut self, packet: u32, builder: Option<u32>) {
        if let Some(id) = builder {
            self.captures[id as usize].building = false;
            self.packets[packet as usize]
                .owners
                .push((id, Rc::clone(&self.captures[id as usize].tokens)));
        }
    }
    #[inline(always)]
    pub(crate) fn begin_macro(&mut self, id: u32) -> Result<()> {
        let definition = &self.definitions[id as usize];
        if definition.prefix.empty() && definition.delimiters.is_empty() {
            let body = definition.body;
            self.retire_exhausted();
            return self.push_body(id, body);
        }
        self.begin_macro_with_arguments(id)
    }
    #[inline(never)]
    fn begin_macro_with_arguments(&mut self, id: u32) -> Result<()> {
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
        let packet = self.take_packet(count)?;
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
                        self.capture_raw(packet, argument, raw, &mut builder)?;
                    }
                } else {
                    self.capture_raw(packet, argument, first, &mut builder)?;
                }
            } else {
                let length = (delimiter.end - delimiter.start) as usize;
                let mut matched = 0;
                let mut depth = 0;
                let mut seen = 0;
                let mut first_group = false;
                let mut group_end = None;
                loop {
                    let raw = self.next_raw()?;
                    seen += 1;
                    if seen == 1 {
                        first_group = raw.token.is_begin();
                    }
                    self.capture_raw(packet, argument, raw, &mut builder)?;
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
                        if depth == 0 && first_group && group_end.is_none() {
                            group_end = Some(seen);
                        }
                    }
                }
                let sequence = &mut self.packets[packet as usize].arguments[argument];
                sequence.trim_back(length);
                if group_end == Some(seen - length) {
                    sequence.trim_front();
                    sequence.trim_back(1);
                }
            }
        }
        self.finish_capture(packet, builder);
        self.retire_exhausted();
        self.push_sequence(SequenceRef::Body(id), Some(packet), Some(packet))
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
