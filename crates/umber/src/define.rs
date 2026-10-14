use crate::{Engine, Error, Result, token::*};

impl Engine<'_> {
    #[inline(never)]
    pub(crate) fn define(
        &mut self,
        expanded: bool,
        long: bool,
        global: bool,
        protected: bool,
    ) -> Result<()> {
        let cs = self.scan_cs()?;
        let mut tokens = self.values.builder()?;
        let mut prefix_end = None;
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
                    tokens.push(next.token)?;
                    brace_tail = Some(next.token);
                    break;
                }
                if parameters == 9 || next.token != Token::other(b'1' + parameters) {
                    return Err(Error::Syntax);
                }
                if parameters == 0 {
                    prefix_end = Some(tokens.len());
                }
                tokens.push(Token(PARAM | read.meaning.value as u32))?;
                parameters += 1;
            } else {
                tokens.push(read.token)?;
            }
        }
        let prefix_end = prefix_end.unwrap_or(tokens.len());
        tokens.push(Token(PARAM | 255))?;
        let body_start = tokens.len();
        let uses_parameters = if expanded {
            self.scan_body::<true>(Some(parameters), &mut tokens)?
        } else {
            self.scan_body::<false>(Some(parameters), &mut tokens)?
        };
        if let Some(token) = brace_tail {
            tokens.push(token)?;
        }
        let body = ArenaRange {
            start: body_start,
            end: tokens.len(),
        };
        // Each parameter marker already lives in the immutable signature.
        // Walk between markers to build metadata directly in its final slice.
        let mut delimiter_start = prefix_end + 1;
        let parameter_data = (0..parameters)
            .map(|_| {
                let start = delimiter_start;
                let mut end = start;
                while end < body_start - 1 {
                    let token = tokens.get(end);
                    if token.is_parameter() {
                        break;
                    }
                    end += 1;
                }
                delimiter_start = end + 1;
                let span = ArenaRange { start, end };
                let len = (span.end - span.start) as usize;
                let mut failure = vec![0; len].into_boxed_slice();
                let mut matched = 0;
                for index in 1..len {
                    let token = tokens.get(span.start + index as u32);
                    while matched > 0 && tokens.get(span.start + matched as u32) != token {
                        matched = failure[matched - 1];
                    }
                    if tokens.get(span.start + matched as u32) == token {
                        matched += 1;
                    }
                    failure[index] = matched;
                }
                Parameter {
                    delimiter: span,
                    failure,
                }
            })
            .collect();
        let owner = tokens.finish(Some(Definition {
            prefix_end,
            body,
            parameters: parameter_data,
            long,
            uses_parameters,
        }));
        let id = owner.id;
        self.assign_meaning(
            cs,
            Meaning {
                command: if protected {
                    cmd::PROTECTED_MACRO
                } else {
                    cmd::MACRO
                },
                value: id as i32,
            },
            global,
        );
        Ok(())
    }
}
