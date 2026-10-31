use crate::{Engine, Error, Result, token::*};

#[derive(Clone, Copy)]
pub(crate) enum Condition {
    Pending,
    Then { case: bool },
    Else,
}

impl Engine<'_> {
    fn conditional_character(read: Read) -> (u8, i32) {
        if read.meaning.command < 16 {
            (read.meaning.command, read.meaning.value)
        } else if read.meaning == Meaning::NOEXPAND_RELAX
            && let Some(byte) = read.token.active_byte()
        {
            (13, i32::from(byte))
        } else {
            // Other control sequences match each other, but no character code.
            (cmd::RELAX, 256)
        }
    }
    pub(crate) fn conditional(&mut self, command: u8, invert: bool) -> Result<()> {
        let target = self.document.conditions.len();
        self.document.conditions.push(Condition::Pending);
        let is_case = command == cmd::IFCASE;
        let mut choice = if is_case { self.scan_int()? } else { 0 };
        let yes = match command {
            cmd::IFTRUE => true,
            cmd::IFFALSE => false,
            cmd::IFCASE => choice == 0,
            cmd::IFODD => self.scan_int()? & 1 != 0,
            cmd::IFDEFINED => self.next_token()?.meaning != Meaning::UNDEFINED,
            cmd::IFCSNAME => {
                let id = self.scan_name()?;
                self.document.bindings[id as usize].meaning() != Meaning::UNDEFINED
            }
            cmd::IFX => {
                let a = self.next_token()?.meaning;
                let b = self.next_token()?.meaning;
                self.same_meaning(a, b)
            }
            cmd::IF | cmd::IFCAT => {
                let a = Self::conditional_character(self.next_expanded()?);
                let b = Self::conditional_character(self.next_expanded()?);
                if command == cmd::IF {
                    a.1 == b.1
                } else {
                    a.0 == b.0
                }
            }
            cmd::IFNUM | cmd::IFDIM => {
                let dimension = command == cmd::IFDIM;
                let a = if dimension {
                    self.scan_dimen()?.0
                } else {
                    self.scan_int()?
                };
                let relation = self.nonspace(true)?.token;
                let b = if dimension {
                    self.scan_dimen()?.0
                } else {
                    self.scan_int()?
                };
                if relation == Token::other(b'=') {
                    a == b
                } else if relation == Token::other(b'<') {
                    a < b
                } else if relation == Token::other(b'>') {
                    a > b
                } else {
                    return Err(Error::Syntax);
                }
            }
            _ => return Err(Error::Conditional),
        } ^ invert;
        if yes {
            self.document.conditions[target] = Condition::Then { case: is_case };
            return Ok(());
        }
        loop {
            let boundary = self.skip_boundary()?;
            if self.document.conditions.len() - 1 != target {
                if boundary == cmd::FI {
                    self.document.conditions.pop();
                }
                continue;
            }
            match boundary {
                cmd::FI => {
                    self.document.conditions.pop();
                    return Ok(());
                }
                cmd::ELSE => {
                    self.document.conditions[target] = Condition::Else;
                    return Ok(());
                }
                cmd::OR if is_case => {
                    choice = choice.saturating_sub(1);
                    if choice == 0 {
                        self.document.conditions[target] = Condition::Then { case: true };
                        return Ok(());
                    }
                }
                _ => return Err(Error::Conditional),
            }
        }
    }
    pub(crate) fn condition_boundary(&mut self, read: Read) -> Result<()> {
        let phase = *self.document.conditions.last().ok_or(Error::Conditional)?;
        match phase {
            Condition::Pending => {
                self.insert_pair(FROZEN_RELAX, read.token);
                return Ok(());
            }
            _ if read.meaning.command == cmd::FI => {
                self.document.conditions.pop();
                return Ok(());
            }
            Condition::Else => return Err(Error::Conditional),
            Condition::Then { case: false } if read.meaning.command == cmd::OR => {
                return Err(Error::Conditional);
            }
            _ => {}
        }
        while self.skip_boundary()? != cmd::FI {}
        self.document.conditions.pop();
        Ok(())
    }
    fn skip_boundary(&mut self) -> Result<u8> {
        let mut depth = 0;
        loop {
            self.skip_characters();
            let meaning = self.next_token()?.meaning;
            if meaning.is_conditional() {
                depth += 1;
            } else if meaning.command == cmd::FI {
                if depth == 0 {
                    return Ok(cmd::FI);
                }
                depth -= 1;
            } else if matches!(meaning.command, cmd::ELSE | cmd::OR) && depth == 0 {
                return Ok(meaning.command);
            }
        }
    }
    #[inline]
    fn skip_characters(&mut self) {
        if self.document.sources.is_empty() {
            while self
                .document
                .file
                .read_character(self.document.scalars.catcodes())
                .is_some()
            {}
            return;
        }
        let Some(cursor) = self
            .document
            .sources
            .last_mut()
            .and_then(Source::cursor_mut)
        else {
            return;
        };
        while cursor.next != cursor.end {
            // SAFETY: the cursor retains its initialized immutable window.
            // This loop advances only; it neither allocates nor retires input.
            let token = unsafe { cursor.next.read() };
            if token.0 >= 16 << 8 {
                break;
            }
            // Ordinary characters cannot be conditional commands. Parameter
            // markers and control sequences keep the ordinary read path.
            cursor.next = unsafe { cursor.next.add(1) };
        }
    }
    pub(crate) fn scan_name(&mut self) -> Result<u32> {
        // Common names stay on the stack. A long-name buffer remains checked
        // out while nested expansion runs, so nested scanners use disjoint blocks.
        let mut prefix = [0; 64];
        let mut length = 0;
        let mut spill: Option<crate::value_metadata::Buffer<u8>> = None;
        let result = (|| loop {
            let read = self.next_expanded()?;
            if read.meaning.command == cmd::ENDCSNAME {
                return self.intern(if let Some(spill) = &spill {
                    spill
                } else {
                    &prefix[..length]
                });
            }
            if read.token.is_cs() {
                return Err(Error::Syntax);
            }
            if length < prefix.len() {
                prefix[length] = read.token.byte();
                length += 1;
            } else {
                if spill.is_none() {
                    let mut buffer = self
                        .name_spills
                        .pop()
                        .unwrap_or_else(|| self.values.name_buffer());
                    buffer.reserve(prefix.len() + 1);
                    buffer.extend_from_slice(&prefix);
                    spill = Some(buffer);
                }
                spill.as_mut().unwrap().push(read.token.byte());
            }
        })();
        if let Some(mut spill) = spill {
            spill.clear();
            self.name_spills.push(spill);
        }
        result
    }
}
