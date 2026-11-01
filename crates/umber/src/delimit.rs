use crate::{Engine, Error, Result, token::*};

// The source retains an immutable initialized window in one allocation, with
// next at or before end. Four-word reads only peek within that window; advance
// together when all are characters, otherwise find the first structural word
// with the scalar tail. The caller handles that word at the returned cursor.
// Equal endpoints may be empty and are never dereferenced.
#[inline(never)]
unsafe fn character_end<const EXCEPT: bool>(
    mut next: *const Token,
    end: *const Token,
    except: Token,
) -> *const Token {
    // SAFETY: unequal endpoints belong to the same retained allocation.
    while next != end && unsafe { end.offset_from(next) } >= 4 {
        // SAFETY: four initialized immutable words remain in this window.
        let words = unsafe { next.cast::<[Token; 4]>().read() };
        let structural = words
            .map(|token| u32::from(!token.is_plain_character() || (EXCEPT && token == except)));
        if structural[0] | structural[1] | structural[2] | structural[3] != 0 {
            break;
        }
        // SAFETY: all four words precede end in the same retained window.
        next = unsafe { next.add(4) };
    }
    while next != end {
        // SAFETY: the caller supplied this retained immutable window.
        let token = unsafe { next.read() };
        if !token.is_plain_character() || (EXCEPT && token == except) {
            break;
        }
        // SAFETY: advance at most one past the same initialized window.
        next = unsafe { next.add(1) };
    }
    next
}

impl Engine<'_> {
    #[inline(always)]
    fn capture_token_run<const RETAIN: bool, const EXCEPT: bool>(
        &mut self,
        packet: u32,
        argument: usize,
        builder: &mut Option<crate::values::Builder>,
        except: Token,
    ) -> Result<usize> {
        let Some(cursor) = self
            .document
            .sources
            .last_mut()
            .and_then(Source::cursor_mut)
        else {
            return Ok(0);
        };
        // Promotion stays on the scalar path so a failed append consumes only
        // the failing word. A published range is retained before source retirement.
        if RETAIN
            && self.packets[packet as usize].arguments[argument]
                .first
                .store
                == SINGLE
        {
            return Ok(0);
        }
        let first = cursor.next;
        // SAFETY: this source retains an initialized immutable window; scanning
        // returns a pointer within it and neither installs nor retires a source.
        let next = unsafe { character_end::<EXCEPT>(first, cursor.end, except) };
        cursor.next = next;
        // SAFETY: first, next and end belong to this retained allocation.
        let count = unsafe { next.offset_from(first) } as usize;
        if RETAIN && count != 0 {
            let start = cursor.end_position - unsafe { cursor.end.offset_from(first) } as u32;
            let span = Span {
                store: cursor.store,
                start,
                end: start + count as u32,
            };
            self.capture_span(packet, argument, span, builder)?;
        }
        Ok(count)
    }
    #[inline(never)]
    pub(crate) fn capture_argument<const DELIMITED: bool, const RETAIN: bool>(
        &mut self,
        owner: &crate::values::Value,
        packet: u32,
        argument: usize,
        builder: &mut Option<crate::values::Builder>,
    ) -> Result<()> {
        let definition = owner.definition();
        let delimiter = if DELIMITED {
            definition.parameters[argument].delimiter
        } else {
            ArenaRange::default()
        };
        // Resolve the immutable fallback range once per argument, rather than
        // reloading its parameter offset after each nested scanner call.
        let failure = if DELIMITED {
            definition.failure(argument)
        } else {
            &[]
        };
        let first = if DELIMITED {
            owner.tokens.get(delimiter.start)
        } else {
            Token(u32::MAX)
        };
        let long = definition.long;
        let length = (delimiter.end - delimiter.start) as usize;
        let mut matched = 0;
        let mut depth = usize::from(!DELIMITED);
        let mut seen = 0;
        let mut first_group = false;
        let mut group_end = None;
        loop {
            if (!DELIMITED || matched == 0 || depth > 0)
                && (self.at_file_input() || !DELIMITED || RETAIN)
            {
                let except = if DELIMITED && depth == 0 {
                    first
                } else {
                    Token(u32::MAX)
                };
                if self.at_file_input() {
                    seen += self.capture_file_characters::<RETAIN, DELIMITED>(
                        packet, argument, builder, except,
                    )?;
                } else if !DELIMITED || RETAIN {
                    seen += self.capture_token_run::<RETAIN, DELIMITED>(
                        packet, argument, builder, except,
                    )?;
                }
            }
            if !DELIMITED
                && let Some(cursor) = self.document.sources.last().and_then(Source::cursor)
                && cursor.next == cursor.end
            {
                self.advance_source()?;
                continue;
            }
            // The grouped-window path reports a non-long paragraph before
            // advancing that window, as its existing bulk scanner does.
            if !DELIMITED && !long
                && let Some(cursor) = self.document.sources.last().and_then(Source::cursor)
                && cursor.next != cursor.end
                // SAFETY: next precedes end in the retained immutable window.
                && unsafe { cursor.next.read() } == self.document.par
                // An outer paragraph must be consumed and rejected by the
                // ordinary reader before the non-long paragraph check.
                && !self.resolve(self.document.par).is_outer_macro()
            {
                return Err(Error::Argument);
            }
            let raw = self.next_raw()?;
            if DELIMITED && RETAIN {
                seen += 1;
                if seen == 1 {
                    first_group = raw.token.is_begin();
                }
                self.capture_raw(packet, argument, raw, builder)?;
            }
            if DELIMITED && depth == 0 {
                loop {
                    let expected = if matched == 0 {
                        first
                    } else {
                        owner.tokens.get(delimiter.start + matched as u32)
                    };
                    if expected == raw.token {
                        matched += 1;
                        break;
                    }
                    if matched == 0 {
                        break;
                    }
                    let fallback = failure[matched - 1];
                    matched = fallback;
                }
                if matched == length {
                    break;
                }
            }
            // On delimiter fallback, already matched tokens become argument
            // content without another paragraph check. Check the current word
            // only when it is unmatched or inside a group.
            if !long && raw.token == self.document.par && (!DELIMITED || depth > 0 || matched == 0)
            {
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
                if !DELIMITED && depth == 0 {
                    return Ok(());
                }
                if RETAIN && depth == 0 && first_group && group_end.is_none() {
                    group_end = Some(seen);
                }
            }
            if !DELIMITED && RETAIN {
                self.capture_raw(packet, argument, raw, builder)?;
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
