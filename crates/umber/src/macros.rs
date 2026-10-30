use crate::{Engine, Error, Result, token::*};

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
            self.packets.push(self.values.packet());
            id
        };
        self.values
            .prepare_packet(&mut self.packets[id as usize], count);
        Ok(id)
    }
    pub(crate) fn release_packet(&mut self, id: u32) {
        self.packets[id as usize].clear();
        self.free_packets.push(id);
    }
    #[inline(always)]
    pub(crate) fn capture_raw(
        &mut self,
        packet: u32,
        argument: usize,
        raw: Raw,
        builder: &mut Option<crate::values::Builder>,
    ) -> Result<()> {
        let first = self.packets[packet as usize].arguments[argument].first;
        if first.empty() && raw.origin.is_none() {
            self.packets[packet as usize].arguments[argument].first = Span::single(raw.token);
            return Ok(());
        }
        if let Some(span) = raw.origin {
            return self.capture_span(packet, argument, span, builder);
        }
        if let Some(token) = first.single_token() {
            self.promote_single(packet, argument, token, builder)?;
        }
        let span = self.materialize_token(raw.token, builder)?;
        self.packets[packet as usize].arguments[argument].append(span);
        Ok(())
    }
    #[inline(always)]
    pub(crate) fn capture_span(
        &mut self,
        packet: u32,
        argument: usize,
        span: Span,
        builder: &mut Option<crate::values::Builder>,
    ) -> Result<()> {
        let first = self.packets[packet as usize].arguments[argument].first;
        if let Some(token) = first.single_token() {
            self.promote_single(packet, argument, token, builder)?;
        }
        if self.packets[packet as usize].arguments[argument].append(span) {
            // Appending metadata leaves the input owner alive through this call.
            self.retain_backing(packet, span.store);
        }
        Ok(())
    }
    #[inline(never)]
    fn promote_single(
        &mut self,
        packet: u32,
        argument: usize,
        token: Token,
        builder: &mut Option<crate::values::Builder>,
    ) -> Result<()> {
        let span = self.materialize_token(token, builder)?;
        self.packets[packet as usize].arguments[argument].first = span;
        Ok(())
    }
    #[inline(never)]
    fn retain_backing(&mut self, packet: u32, id: u32) {
        let owners = &mut self.packets[packet as usize].owners;
        if !owners.iter().any(|owner| owner.id == id) {
            owners.push(self.values.get(id));
        }
    }
    #[inline(never)]
    fn materialize_token(
        &mut self,
        token: Token,
        builder: &mut Option<crate::values::Builder>,
    ) -> Result<Span> {
        if builder.is_none() {
            *builder = Some(self.values.builder(crate::values::Class::Transient)?);
        }
        let tokens = builder.as_mut().unwrap();
        let start = tokens.len();
        tokens.push(token)?;
        Ok(Span {
            store: tokens.id(),
            start,
            end: start + 1,
        })
    }
    #[inline(always)]
    pub(crate) fn finish_capture(&mut self, packet: u32, builder: Option<crate::values::Builder>) {
        if let Some(tokens) = builder {
            self.packets[packet as usize].owners.push(tokens.finish());
        }
    }
    #[inline(always)]
    pub(crate) fn begin_macro(&mut self, id: u32) -> Result<()> {
        let owner = self.values.get(id);
        let definition = owner.definition();
        if definition.prefix_end == 0 && definition.parameters.is_empty() {
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
    pub(crate) fn capture_single_group(
        &mut self,
        packet: u32,
        argument: usize,
        long: bool,
    ) -> bool {
        let Some(cursor) = self.sources.last().and_then(Source::cursor) else {
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
        let token = if token.is_parameter() {
            let Some(parent) = cursor.owns_packet else {
                return false;
            };
            let sequence = &self.packets[parent as usize].arguments[(token.byte() - 1) as usize];
            let Some(token) = sequence.first.single_token() else {
                return false;
            };
            token
        } else {
            token
        };
        if !token.is_plain_character()
            && (token.is_begin() || token.is_end() || (!long && token == self.par))
        {
            return false;
        }
        // SAFETY: second was initialized in this window. Looking up an argument
        // only borrowed packet metadata; this cursor and its backing stay live.
        let Some(cursor) = self.sources.last_mut().and_then(Source::cursor_mut) else {
            return false;
        };
        cursor.next = unsafe { second.add(1) };
        // Initial grouped capture has an empty argument. Character words and
        // interned control-sequence identities need no token backing owner.
        self.packets[packet as usize].arguments[argument].first = Span::single(token);
        true
    }
    // Returns the number of captured tokens. Skipped spaces advance the line
    // without a token, so the caller cannot count line positions instead.
    #[inline(never)]
    pub(crate) fn capture_file_characters<const RETAIN: bool, const EXCEPT: bool>(
        &mut self,
        packet: u32,
        argument: usize,
        builder: &mut Option<crate::values::Builder>,
        except: Token,
    ) -> Result<usize> {
        if !RETAIN {
            let mut count = 0;
            while self
                .file
                .read_character_except::<EXCEPT>(self.scalars.catcodes(), except)
                .is_some()
            {
                count += 1;
            }
            return Ok(count);
        }
        let Some(first) = self
            .file
            .read_character_except::<EXCEPT>(self.scalars.catcodes(), except)
        else {
            return Ok(0);
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
        let Some(token) = self
            .file
            .read_character_except::<EXCEPT>(self.scalars.catcodes(), except)
        else {
            return Ok(1);
        };
        if let Some(token) = self.packets[packet as usize].arguments[argument]
            .first
            .single_token()
        {
            self.promote_single(packet, argument, token, builder)?;
        }
        // Both paths acquired the invocation's unpublished builder. Chunk
        // growth preserves its initialized words and their final coordinates.
        let tokens = builder.as_mut().unwrap();
        let start = tokens.len();
        tokens.push(token)?;
        self.file
            .read_characters::<EXCEPT>(self.scalars.catcodes(), except, tokens)?;
        let end = tokens.len();
        self.packets[packet as usize].arguments[argument].append(Span {
            store: tokens.id(),
            start,
            end,
        });
        Ok(1 + (end - start) as usize)
    }
    #[inline(never)]
    fn begin_macro_with_arguments<const RETAIN: bool>(
        &mut self,
        owner: crate::values::Owner,
    ) -> Result<()> {
        let definition = owner.definition();
        for position in 0..definition.prefix_end {
            let expected = owner.tokens.get(position);
            if self.next_raw()?.token != expected {
                return Err(Error::Argument);
            }
        }
        let count = definition.parameters.len();
        if count == 0 {
            self.retire_exhausted();
            return self.push_body(owner, None);
        }
        let packet = if RETAIN { self.take_packet(count)? } else { 0 };
        let mut builder = None;
        let long = definition.long;
        for argument in 0..count {
            let delimiter = definition.parameters[argument].delimiter;
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
                    if !RETAIN || !self.capture_single_group(packet, argument, long) {
                        self.capture_argument::<false, RETAIN>(
                            &owner,
                            packet,
                            argument,
                            &mut builder,
                        )?;
                    }
                } else if RETAIN {
                    self.capture_raw(packet, argument, first, &mut builder)?;
                }
            } else {
                self.capture_argument::<true, RETAIN>(&owner, packet, argument, &mut builder)?;
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
        a.long == b.long
            && a_owner.tokens.len == b_owner.tokens.len
            && a_owner
                .tokens
                .equal(0, &b_owner.tokens, 0, a_owner.tokens.len)
    }
}
