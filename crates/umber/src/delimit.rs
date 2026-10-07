use crate::{Engine, Error, Result, token::*};

impl Engine<'_> {
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
