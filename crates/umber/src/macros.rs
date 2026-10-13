use crate::{Engine, Error, Result, token::*};
use std::rc::Rc;

// The source retains an immutable initialized window in one allocation, with
// next at or before end. Four-word reads only peek within that window; advance
// together when all are characters, otherwise find the first structural word
// with the scalar tail. The caller handles that word at the returned cursor.
// Equal endpoints may be empty and are never dereferenced.
#[inline(never)]
unsafe fn group_character_end(mut next: *const Token, end: *const Token) -> *const Token {
    // SAFETY: unequal endpoints belong to the same retained allocation.
    while next != end && unsafe { end.offset_from(next) } >= 4 {
        // SAFETY: four initialized immutable words remain in this window.
        let words = unsafe { next.cast::<[Token; 4]>().read() };
        let structural = words.map(|token| u32::from(!(3 << 8..PARAM).contains(&token.0)));
        if structural[0] | structural[1] | structural[2] | structural[3] != 0 {
            break;
        }
        // SAFETY: all four words precede end in the same retained window.
        next = unsafe { next.add(4) };
    }
    while next != end {
        // SAFETY: the caller supplied this retained immutable window.
        let token = unsafe { next.read() };
        if !(3 << 8..PARAM).contains(&token.0) {
            break;
        }
        // SAFETY: advance at most one past the same initialized window.
        next = unsafe { next.add(1) };
    }
    next
}

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
        if self.captures.len() >= DURABLE as usize {
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
            self.retain_backing(packet, span.store);
            span
        } else {
            self.materialize_token(raw.token, builder)?
        };
        self.packets[packet as usize].arguments[argument].append(span);
        Ok(())
    }
    #[inline(always)]
    pub(crate) fn capture_span(
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
        self.retain_backing(packet, span.store);
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
    fn retain_backing(&mut self, packet: u32, id: u32) {
        let owners = &mut self.packets[packet as usize].owners;
        if !owners.iter().any(|old| old.id() == id) {
            owners.push(if id & DURABLE != 0 {
                Backing::Durable(self.values.get(id & !DURABLE))
            } else {
                Backing::Capture {
                    id,
                    _owner: Rc::clone(&self.captures[id as usize].tokens),
                }
            });
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
        self.packets[packet as usize].owners.push(Backing::Capture {
            id,
            _owner: Rc::clone(&self.captures[id as usize].tokens),
        });
    }
    #[inline(always)]
    pub(crate) fn begin_macro(&mut self, id: u32) -> Result<()> {
        let owner = self.values.get(id);
        let definition = owner.definition();
        if definition.prefix.empty() && definition.parameter_count == 0 {
            self.retire_exhausted();
            return self.push_body(owner, None);
        }
        if definition.uses_parameters {
            self.begin_macro_with_arguments::<true>(owner)
        } else {
            self.begin_macro_with_arguments::<false>(owner)
        }
    }
    #[inline(always)]
    fn capture_single_group(&mut self, packet: u32, argument: usize) -> bool {
        let Some(Source::Tokens(cursor)) = self.sources.last_mut() else {
            return false;
        };
        let first = cursor.next;
        if first == cursor.end {
            return false;
        }
        // SAFETY: first is initialized in the retained immutable window. The
        // second pointer is at most one past it; read only when before end.
        let second = unsafe { first.add(1) };
        if second == cursor.end || !unsafe { second.read() }.is_end() {
            return false;
        }
        // SAFETY: first precedes second in the same retained window.
        let token = unsafe { first.read() };
        let token = if token.0 & PARAM != 0 && !token.is_cs() {
            let Some(parent) = cursor.macro_packet else {
                return false;
            };
            let sequence = &self.packets[parent as usize].arguments[(token.byte() - 1) as usize];
            if sequence.first.store != SINGLE {
                return false;
            }
            // SINGLE is constructed only for a sequence containing one word.
            Token(sequence.first.start)
        } else {
            token
        };
        if !(3 << 8..PARAM).contains(&token.0) {
            return false;
        }
        // SAFETY: second was initialized in this window. Looking up an argument
        // only borrowed packet metadata; this cursor and its backing stay live.
        cursor.next = unsafe { second.add(1) };
        // Initial grouped capture has an empty argument. A character word
        // has value semantics and needs no backing owner after retirement.
        self.packets[packet as usize].arguments[argument].first = Span {
            store: SINGLE,
            start: token.0,
            end: token.0 + 1,
        };
        true
    }
    #[inline(always)]
    fn capture_group<const RETAIN: bool>(
        &mut self,
        packet: u32,
        argument: usize,
        long: bool,
        builder: &mut Option<u32>,
    ) -> Result<()> {
        if RETAIN && self.capture_single_group(packet, argument) {
            return Ok(());
        }
        let mut depth = 1;
        loop {
            if let Some(Source::Tokens(cursor)) = self.sources.last_mut() {
                let first = cursor.next;
                let mut next = first;
                let parameter = loop {
                    if next == cursor.end {
                        break None;
                    }
                    // SAFETY: this source retains the immutable window;
                    // the helper only reads and returns a cursor within it.
                    next = unsafe { group_character_end(next, cursor.end) };
                    if next == cursor.end {
                        break None;
                    }
                    // SAFETY: next precedes end in this retained window.
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
                        self.push_sequence(SequenceRef::Argument(parent, parent_argument), None)?;
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
    fn begin_macro_with_arguments<const RETAIN: bool>(
        &mut self,
        owner: crate::values::Owner,
    ) -> Result<()> {
        let definition = owner.definition();
        let prefix = definition.prefix;
        for position in prefix.start..prefix.end {
            let expected = owner.tokens.get(position);
            if self.next_raw()?.token != expected {
                return Err(Error::Argument);
            }
        }
        let count = definition.parameter_count as usize;
        if count == 0 {
            self.retire_exhausted();
            return self.push_body(owner, None);
        }
        let packet = if RETAIN { self.take_packet(count)? } else { 0 };
        let mut builder = None;
        let long = definition.long;
        for argument in 0..count {
            let delimiter = definition.parameters.as_ref().unwrap().delimiters[argument];
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
                self.capture_delimited::<RETAIN>(&owner, packet, argument, &mut builder)?;
            }
        }
        if RETAIN {
            self.finish_capture(packet, builder);
        }
        self.retire_exhausted();
        self.push_body(owner, RETAIN.then_some(packet))
    }
    pub(crate) fn same_meaning(&self, a: Meaning, b: Meaning) -> bool {
        if a == b {
            return true;
        }
        if a.command != b.command || !a.is_macro() {
            return false;
        }
        let a_owner = self.values.get(a.value as u32);
        let b_owner = self.values.get(b.value as u32);
        let a = a_owner.definition();
        let b = b_owner.definition();
        let a_signature = a.signature();
        let b_signature = b.signature();
        a.long == b.long
            && a_signature.end - a_signature.start == b_signature.end - b_signature.start
            && a_owner.tokens.equal(
                a_signature.start,
                &b_owner.tokens,
                b_signature.start,
                a_signature.end - a_signature.start,
            )
    }
}
