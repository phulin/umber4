use crate::{Engine, Error, Result, token::*};

impl Engine<'_> {
    #[inline(never)]
    fn capture_delimited_tokens(
        &mut self,
        packet: u32,
        argument: usize,
        builder: &mut Option<u32>,
        delimiter_start: u32,
        depth: i32,
    ) -> Result<u32> {
        let Some(Source::Tokens(cursor)) = self.sources.last_mut() else {
            return Ok(0);
        };
        // Promote an existing inline word through the scalar path before any
        // lookahead, preserving consumption if promotion fails at the next word.
        if self.packets[packet as usize].arguments[argument]
            .first
            .store
            == SINGLE
        {
            return Ok(0);
        }
        let (first, end) = (cursor.next, cursor.end);
        let mut next = first;
        if next == end {
            return Ok(0);
        }
        // SAFETY: next is inside an initialized immutable source window. Its
        // owner retains the allocation throughout this scan.
        let mut token = unsafe { next.read() };
        if token.0 >= 16 << 8 || matches!(token.cat(), 1 | 2) {
            return Ok(0);
        }
        let except = if depth == 0 {
            self.arena.get(delimiter_start)
        } else {
            Token(u32::MAX)
        };
        loop {
            if token == except {
                break;
            }
            // SAFETY: next advances at most to this allocation's end.
            next = unsafe { next.add(1) };
            if next == end {
                break;
            }
            // SAFETY: next is inside the same retained immutable window. Neither
            // matching nor capture installs or retires a source during the scan.
            token = unsafe { next.read() };
            if token.0 >= 16 << 8 || matches!(token.cat(), 1 | 2) {
                break;
            }
        }
        cursor.next = next;
        if next == first {
            return Ok(0);
        }
        // SAFETY: all three pointers belong to this installed source window.
        let count = unsafe { next.offset_from(first) } as u32;
        let start = cursor.window_end_position - unsafe { end.offset_from(first) } as u32;
        let span = Span {
            store: cursor.span.store,
            start,
            end: start + count,
        };
        // Retain before the ordinary reader can retire this source. With no
        // inline word to promote, capturing this immutable range cannot fail.
        self.capture_span(packet, argument, span, builder)?;
        Ok(count)
    }
    #[inline(never)]
    fn capture_delimited_file<const RETAIN: bool>(
        &mut self,
        packet: u32,
        argument: usize,
        builder: &mut Option<u32>,
        start: u32,
        depth: i32,
    ) -> Result<()> {
        let except = if depth == 0 {
            self.arena.get(start)
        } else {
            Token(u32::MAX)
        };
        self.capture_file_characters::<RETAIN, true>(packet, argument, builder, except)
    }
    #[inline(never)]
    pub(crate) fn capture_delimited<const RETAIN: bool>(
        &mut self,
        id: u32,
        packet: u32,
        argument: usize,
        builder: &mut Option<u32>,
    ) -> Result<()> {
        let delimiter = self.definitions[id as usize].delimiters[argument];
        let long = self.definitions[id as usize].long;
        let length = (delimiter.end - delimiter.start) as usize;
        let mut matched = 0;
        let mut depth = 0;
        let mut seen = 0;
        let mut first_group = false;
        let mut group_end = None;
        loop {
            if self.sources.is_empty() && (matched == 0 || depth > 0) {
                let position = self.file.position();
                self.capture_delimited_file::<RETAIN>(
                    packet,
                    argument,
                    builder,
                    delimiter.start,
                    depth,
                )?;
                if RETAIN {
                    // The run stays within the prepared line. It has no braces,
                    // so only the count used to recognize one outer group changes.
                    seen += self.file.position() - position;
                }
            } else if RETAIN && (matched == 0 || depth > 0) {
                seen += self.capture_delimited_tokens(
                    packet,
                    argument,
                    builder,
                    delimiter.start,
                    depth,
                )? as usize;
            }
            let raw = self.next_raw()?;
            if RETAIN {
                seen += 1;
                if seen == 1 {
                    first_group = raw.token.is_begin();
                }
                self.capture_raw(packet, argument, raw, builder)?;
            }
            if depth == 0 {
                loop {
                    if self.arena.get(delimiter.start + matched as u32) == raw.token {
                        matched += 1;
                        break;
                    }
                    if matched == 0 {
                        break;
                    }
                    matched = self.definitions[id as usize].failures[argument][matched - 1];
                }
                if matched == length {
                    break;
                }
            }
            // Tokens previously matched against a delimiter become argument
            // content without another par check on fallback. The current token
            // is checked only when it is unmatched.
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
                if RETAIN && depth == 0 && first_group && group_end.is_none() {
                    group_end = Some(seen);
                }
            }
        }
        if RETAIN {
            let sequence = &mut self.packets[packet as usize].arguments[argument];
            sequence.trim_back(length);
            if group_end == Some(seen - length) {
                sequence.trim_front();
                sequence.trim_back(1);
            }
        }
        Ok(())
    }
}
