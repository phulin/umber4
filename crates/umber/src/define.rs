use crate::{Engine, Error, Result, token::*};

impl Engine<'_> {
    pub(crate) fn define(
        &mut self,
        expanded: bool,
        long: bool,
        global: bool,
        protected: bool,
    ) -> Result<()> {
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
        let uses_parameters = if expanded {
            self.scan_body::<true>(Some(parameters), |engine, token| engine.arena.push(token))?
        } else {
            self.scan_body::<false>(Some(parameters), |engine, token| engine.arena.push(token))?
        };
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
            uses_parameters,
            failures,
        });
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
