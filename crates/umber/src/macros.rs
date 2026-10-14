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
        if let Some(span) = raw.origin {
            return self.capture_span(packet, argument, span, builder);
        }
        if first.store == SINGLE && !first.empty() {
            self.promote_single(packet, argument, builder)?;
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
        builder: &mut Option<u32>,
    ) -> Result<()> {
        let first = self.packets[packet as usize].arguments[argument].first;
        if first.store == SINGLE && !first.empty() {
            self.promote_single(packet, argument, builder)?;
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
        if !owners.iter().any(|old| old.0 == id) {
            owners.push((
                id,
                if id & DURABLE != 0 {
                    Backing::Durable {
                        _owner: self.values.get(id & !DURABLE),
                    }
                } else {
                    Backing::Capture {
                        _owner: Rc::clone(&self.captures[id as usize].tokens),
                    }
                },
            ));
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
        self.packets[packet as usize].owners.push((
            id,
            Backing::Capture {
                _owner: Rc::clone(&self.captures[id as usize].tokens),
            },
        ));
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
        let token = if token.0 & PARAM != 0 && !token.is_cs() {
            let Some(parent) = cursor.owns_packet else {
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
        if !(3 << 8..PARAM).contains(&token.0)
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
        self.packets[packet as usize].arguments[argument].first = Span {
            store: SINGLE,
            start: token.0,
            end: token.0 + 1,
        };
        true
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
