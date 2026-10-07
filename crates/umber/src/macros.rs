use crate::{Engine, Error, Result, token::*};
use std::rc::Rc;

impl Engine<'_> {
    #[inline]
    pub(crate) fn take_packet(&mut self, count: usize) -> Result<u32> {
        let id = self.free_packets.pop();
        if let Some(id) = id
            && self.packets[id as usize].arguments.len() == count
        {
            return Ok(id);
        }
        self.prepare_packet(id, count)
    }
    #[inline(never)]
    fn prepare_packet(&mut self, id: Option<u32>, count: usize) -> Result<u32> {
        let id = if let Some(id) = id {
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
    pub(crate) fn capture_buffer(&mut self) -> Result<u32> {
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
        if self.captures.len() >= SINGLE as usize {
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
        let first = self.packets[packet as usize].arguments[argument].first;
        if first.empty() && raw.origin.is_none() {
            self.packets[packet as usize].arguments[argument].first = Span {
                store: SINGLE,
                start: raw.token.0,
                end: raw.token.0 + 1,
            };
            return Ok(());
        }
        if first.store == SINGLE && !first.empty() {
            self.promote_single(packet, argument, builder)?;
        }
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
    #[inline(always)]
    fn capture_span(
        &mut self,
        packet: u32,
        argument: usize,
        span: Span,
        builder: &mut Option<u32>,
    ) -> Result<()> {
        let first = self.packets[packet as usize].arguments[argument].first;
        if first.store == SINGLE && !first.empty() {
            self.promote_single(packet, argument, builder)?;
        }
        let sequence = &mut self.packets[packet as usize].arguments[argument];
        let last = sequence.rest.last_mut().unwrap_or(&mut sequence.first);
        if !last.empty() && last.store == span.store && last.end == span.start {
            last.end = span.end;
            return Ok(());
        }
        if span.store != DEFS {
            self.retain_capture(packet, span.store);
        }
        self.packets[packet as usize].arguments[argument].append(span);
        Ok(())
    }
    #[inline(never)]
    fn promote_single(
        &mut self,
        packet: u32,
        argument: usize,
        builder: &mut Option<u32>,
    ) -> Result<()> {
        let token = Token(
            self.packets[packet as usize].arguments[argument]
                .first
                .start,
        );
        let span = self.materialize_token(token, builder)?;
        self.packets[packet as usize].arguments[argument].first = span;
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
    #[inline(always)]
    pub(crate) fn finish_capture(&mut self, packet: u32, builder: Option<u32>) {
        if let Some(id) = builder {
            self.publish_capture(packet, id);
        }
    }
    #[inline(never)]
    fn publish_capture(&mut self, packet: u32, id: u32) {
        self.captures[id as usize].building = false;
        self.packets[packet as usize]
            .owners
            .push((id, Rc::clone(&self.captures[id as usize].tokens)));
    }
    #[inline(always)]
    pub(crate) fn begin_macro(&mut self, id: u32) -> Result<()> {
        let definition = &self.definitions[id as usize];
        if definition.prefix.empty() && definition.delimiters.is_empty() {
            let body = definition.body;
            self.retire_exhausted();
            return self.push_body(id, body);
        }
        if definition.uses_parameters {
            self.begin_macro_with_arguments::<true>(id)
        } else {
            self.begin_macro_with_arguments::<false>(id)
        }
    }
    #[inline(always)]
    fn capture_group<const RETAIN: bool>(
        &mut self,
        packet: u32,
        argument: usize,
        long: bool,
        builder: &mut Option<u32>,
    ) -> Result<()> {
        let mut depth = 1;
        loop {
            if let Some(Source::Tokens(cursor)) = self.sources.last_mut() {
                let first = cursor.next;
                let mut next = first;
                let parameter = loop {
                    if next == cursor.end {
                        break None;
                    }
                    // SAFETY: this window consists of initialized immutable
                    // tokens in one allocation, retained by the source. No
                    // source or backing storage changes during this scan.
                    let token = unsafe { next.read() };
                    if token.0 & PARAM != 0 && !token.is_cs() {
                        break Some(token.byte() - 1);
                    }
                    if !long && token == self.par {
                        return Err(Error::Argument);
                    }
                    // SAFETY: next is before the end of the same allocation.
                    next = unsafe { next.add(1) };
                    match token.0 >> 8 {
                        1 => depth += 1,
                        2 => {
                            depth -= 1;
                            if depth == 0 {
                                break None;
                            }
                        }
                        _ => {}
                    }
                };
                let parent = cursor.macro_packet;
                // SAFETY: a parameter marker was read strictly before end.
                cursor.next = if parameter.is_some() {
                    unsafe { next.add(1) }
                } else {
                    next
                };
                if next != first {
                    // SAFETY: all pointers belong to this one source window.
                    let start = cursor.window_end_position
                        - unsafe { cursor.end.offset_from(first) } as u32;
                    let end = start + unsafe { next.offset_from(first) } as u32;
                    let span = Span {
                        store: cursor.span.store,
                        start,
                        end: end - u32::from(depth == 0),
                    };
                    // Retain before the scalar reader can retire this source.
                    if RETAIN && !span.empty() {
                        if depth == 0
                            && span.end - span.start == 1
                            && self.packets[packet as usize].arguments[argument]
                                .first
                                .empty()
                        {
                            // SAFETY: the sole captured token is initialized in
                            // this retained window. Its word has value semantics
                            // and remains valid after the source is retired.
                            let token = unsafe { first.read() };
                            self.packets[packet as usize].arguments[argument].first = Span {
                                store: SINGLE,
                                start: token.0,
                                end: token.0 + 1,
                            };
                        } else {
                            self.capture_span(packet, argument, span, builder)?;
                        }
                    }
                    if depth == 0 {
                        return Ok(());
                    }
                }
                if let Some(parent_argument) = parameter {
                    let parent = parent.ok_or(Error::Syntax)?;
                    if let Some(raw) = self.single_argument(parent, parent_argument) {
                        if self.capture_group_token::<RETAIN>(
                            packet, argument, long, builder, &mut depth, raw,
                        )? {
                            return Ok(());
                        }
                    } else {
                        self.push_sequence(
                            SequenceRef::Argument(parent, parent_argument),
                            None,
                            None,
                        )?;
                    }
                } else {
                    self.advance_source()?;
                }
                continue;
            }
            if self.sources.is_empty() {
                self.capture_file_characters::<RETAIN, false>(
                    packet,
                    argument,
                    builder,
                    Token(u32::MAX),
                )?;
            }
            // Special file categories and short insertions use the ordinary
            // reader. Source transitions resume directly at the next window.
            let raw = self.next_raw()?;
            if self
                .capture_group_token::<RETAIN>(packet, argument, long, builder, &mut depth, raw)?
            {
                return Ok(());
            }
        }
    }
    #[inline(never)]
    pub(crate) fn capture_file_characters<const RETAIN: bool, const EXCEPT: bool>(
        &mut self,
        packet: u32,
        argument: usize,
        builder: &mut Option<u32>,
        except: Token,
    ) -> Result<()> {
        if !RETAIN {
            while self
                .file
                .read_character_except::<EXCEPT>(self.scalars.catcodes(), except)
                .is_some()
            {}
            return Ok(());
        }
        let Some(first) = self
            .file
            .read_character_except::<EXCEPT>(self.scalars.catcodes(), except)
        else {
            return Ok(());
        };
        self.capture_raw(
            packet,
            argument,
            Raw {
                token: first,
                origin: None,
                suppress: false,
            },
            builder,
        )?;
        let Some(mut token) = self
            .file
            .read_character_except::<EXCEPT>(self.scalars.catcodes(), except)
        else {
            return Ok(());
        };
        if self.packets[packet as usize].arguments[argument]
            .first
            .store
            == SINGLE
        {
            self.promote_single(packet, argument, builder)?;
        }
        // capture_raw acquired a builder for a non-inline first word; promotion
        // acquired one for an inline word. It remains uniquely owned until the
        // invocation publishes it, so Vec growth cannot move published tokens.
        let id = builder.unwrap();
        let tokens = Rc::get_mut(&mut self.captures[id as usize].tokens).unwrap();
        let start = tokens.len() as u32;
        loop {
            if tokens.len() >= u32::MAX as usize {
                return Err(Error::Capacity);
            }
            tokens.push(token);
            let Some(next) = self
                .file
                .read_character_except::<EXCEPT>(self.scalars.catcodes(), except)
            else {
                break;
            };
            token = next;
        }
        self.packets[packet as usize].arguments[argument].append(Span {
            store: id,
            start,
            end: tokens.len() as u32,
        });
        Ok(())
    }
    #[inline(always)]
    fn capture_group_token<const RETAIN: bool>(
        &mut self,
        packet: u32,
        argument: usize,
        long: bool,
        builder: &mut Option<u32>,
        depth: &mut usize,
        raw: Raw,
    ) -> Result<bool> {
        if !long && raw.token == self.par {
            return Err(Error::Argument);
        }
        if raw.token.is_begin() {
            *depth += 1;
        }
        if raw.token.is_end() {
            *depth -= 1;
            if *depth == 0 {
                return Ok(true);
            }
        }
        if RETAIN {
            self.capture_raw(packet, argument, raw, builder)?;
        }
        Ok(false)
    }
    #[inline(never)]
    fn begin_macro_with_arguments<const RETAIN: bool>(&mut self, id: u32) -> Result<()> {
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
        let packet = if RETAIN { self.take_packet(count)? } else { 0 };
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
                    self.capture_group::<RETAIN>(packet, argument, long, &mut builder)?;
                } else if RETAIN {
                    self.capture_raw(packet, argument, first, &mut builder)?;
                }
            } else {
                self.capture_delimited::<RETAIN>(id, packet, argument, &mut builder)?;
            }
        }
        if RETAIN {
            self.finish_capture(packet, builder);
        }
        self.retire_exhausted();
        if RETAIN {
            self.push_sequence(SequenceRef::Body(id), Some(packet), Some(packet))
        } else {
            self.push_body(id, self.definitions[id as usize].body)
        }
    }
    pub(crate) fn same_meaning(&self, a: Meaning, b: Meaning) -> bool {
        if a == b {
            return true;
        }
        if a.command != b.command || !a.is_macro() {
            return false;
        }
        let a = &self.definitions[a.value as usize];
        let b = &self.definitions[b.value as usize];
        a.long == b.long
            && a.signature.end - a.signature.start == b.signature.end - b.signature.start
            && self.arena.equal_ranges(
                a.signature.start,
                b.signature.start,
                a.signature.end - a.signature.start,
            )
    }
}
