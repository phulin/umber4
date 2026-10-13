use crate::{Engine, Error, Result, scalars::Target, token::*};
pub(crate) enum Save {
    Boundary(bool),
    Meaning(usize, Binding, Option<crate::values::Owner>),
    Integer(Target, Integer),
    Tokens(usize, TokenValue),
    After(Token),
}

impl Engine<'_> {
    pub(crate) fn execute(&mut self, read: Read) -> Result<()> {
        if matches!(read.meaning.command, cmd::RELAX | cmd::SPACE | cmd::PAR) {
            return Ok(());
        }
        let (read, prefix) = if matches!(
            read.meaning.command,
            cmd::GLOBAL | cmd::LONG | cmd::PROTECTED
        ) {
            self.assignment_prefixes(read)?
        } else {
            (read, 0)
        };
        let global = prefix & 1 != 0;
        if prefix & 6 != 0
            && !matches!(
                read.meaning.command,
                cmd::DEF | cmd::GDEF | cmd::EDEF | cmd::XDEF
            )
        {
            return Err(Error::Syntax);
        }
        let assignment_global = match self.scalars[Target::GLOBALDEFS].value {
            n if n > 0 => true,
            n if n < 0 => false,
            _ => global || matches!(read.meaning.command, cmd::GDEF | cmd::XDEF),
        };
        let result = match read.meaning.command {
            cmd::RELAX | cmd::SPACE | cmd::PAR if !global => Ok(()),
            cmd::BEGIN | cmd::BEGIN_GROUP if !global => {
                self.level = self.level.checked_add(1).ok_or(Error::Capacity)?;
                self.save
                    .push(Save::Boundary(read.meaning.command == cmd::BEGIN_GROUP));
                Ok(())
            }
            cmd::END_GROUP_CHAR | cmd::END_GROUP if !global => {
                self.close_group(read.meaning.command == cmd::END_GROUP)
            }
            cmd::DEF | cmd::GDEF | cmd::EDEF | cmd::XDEF => self.define(
                matches!(read.meaning.command, cmd::EDEF | cmd::XDEF),
                prefix & 2 != 0,
                assignment_global,
                prefix & 4 != 0,
            ),
            cmd::LET => {
                let id = self.scan_cs()?;
                let mut right = self.nonspace(false)?;
                if right.token == Token::other(b'=') {
                    right = self.next_token()?;
                    if right.meaning.command == cmd::SPACE {
                        right = self.next_token()?;
                    }
                }
                self.assign_meaning(id, right.meaning, assignment_global);
                Ok(())
            }
            cmd::FUTURELET => {
                let id = self.scan_cs()?;
                let first = self.next_token()?;
                let second = self.next_token()?;
                self.assign_meaning(id, second.meaning, assignment_global);
                self.back(second.token);
                self.back(first.token);
                Ok(())
            }
            cmd::COUNT_DEF | cmd::CHAR_DEF | cmd::TOKS_DEF => {
                let id = self.scan_cs()?;
                self.assign_meaning(id, Meaning::RELAX, assignment_global);
                self.scan_equals()?;
                let value = self.scan_index()? as i32;
                let command = match read.meaning.command {
                    cmd::COUNT_DEF => cmd::COUNT_REF,
                    cmd::TOKS_DEF => cmd::TOKS_REF,
                    _ => cmd::CHAR_GIVEN,
                };
                self.assign_meaning(id, Meaning { command, value }, assignment_global);
                Ok(())
            }
            cmd::TOKS | cmd::TOKS_REF => {
                let index = self.toks_index(read)?;
                self.scan_equals()?;
                let first = loop {
                    let read = self.nonspace(true)?;
                    if read.meaning.command != cmd::RELAX {
                        break read;
                    }
                };
                let owner = if matches!(first.meaning.command, cmd::TOKS | cmd::TOKS_REF) {
                    let from = self.toks_index(first)?;
                    self.token_registers[from].owner.clone()
                } else if first.meaning.command == cmd::BEGIN {
                    Some(self.store_body()?)
                } else {
                    return Err(Error::Syntax);
                };
                self.assign_tokens(index, owner, assignment_global);
                Ok(())
            }
            cmd::COUNT
            | cmd::COUNT_REF
            | cmd::CATCODE
            | cmd::ENDLINE
            | cmd::INTEGER_PARAM
            | cmd::CASE_CODE => {
                let target = self.assignment_target(read)?;
                self.scan_equals()?;
                let value = self.scan_int()?;
                if read.meaning.command == cmd::CATCODE && !(0..=15).contains(&value) {
                    return Err(Error::Number);
                }
                if read.meaning.command == cmd::CASE_CODE && !(0..=255).contains(&value) {
                    return Err(Error::Number);
                }
                self.update_integer(target, assignment_global, |_| Ok(value))
            }
            cmd::ARITH => {
                let operation = read.meaning.value;
                let register = self.nonspace(true)?;
                if !matches!(
                    register.meaning.command,
                    cmd::COUNT | cmd::COUNT_REF | cmd::ENDLINE | cmd::INTEGER_PARAM
                ) {
                    return Err(Error::Unsupported);
                }
                let target = self.assignment_target(register)?;
                self.scan_keyword(b"by")?;
                let operand = self.scan_int()?;
                self.update_integer(target, assignment_global, |old| {
                    Ok(match operation {
                        0 => old.wrapping_add(operand),
                        1 => {
                            let product = i64::from(old) * i64::from(operand);
                            if product.abs() > i64::from(i32::MAX) {
                                return Err(Error::Overflow);
                            }
                            product as i32
                        }
                        2 => old.checked_div(operand).ok_or(Error::Overflow)?,
                        _ => unreachable!(),
                    })
                })
            }
            cmd::AFTER_ASSIGN if !global => {
                self.after_assignment = Some(self.next_raw()?.token);
                Ok(())
            }
            cmd::AFTER_GROUP if !global => {
                let token = self.next_raw()?.token;
                if self.level > 0 {
                    self.save.push(Save::After(token));
                }
                Ok(())
            }
            cmd::CHANGE_CASE if !global => self.change_case(read.meaning.value as usize),
            cmd::MESSAGE if !global => {
                self.opening_brace()?;
                let mut bytes = Vec::new();
                self.scan_body::<true>(None, |engine, token| {
                    engine.print_token(token, &mut bytes);
                    Ok(())
                })?;
                self.messages.push(bytes);
                Ok(())
            }
            _ => Err(Error::Unsupported),
        };
        if result.is_ok()
            && let Some(token) = self.after_assignment
            && matches!(
                read.meaning.command,
                cmd::DEF
                    | cmd::GDEF
                    | cmd::EDEF
                    | cmd::XDEF
                    | cmd::LET
                    | cmd::FUTURELET
                    | cmd::COUNT_DEF
                    | cmd::CHAR_DEF
                    | cmd::TOKS_DEF
                    | cmd::COUNT
                    | cmd::COUNT_REF
                    | cmd::CATCODE
                    | cmd::ENDLINE
                    | cmd::INTEGER_PARAM
                    | cmd::CASE_CODE
                    | cmd::ARITH
                    | cmd::TOKS
                    | cmd::TOKS_REF
            )
        {
            self.after_assignment = None;
            self.back(token);
        }
        result
    }
    #[inline(never)]
    fn assignment_prefixes(&mut self, mut read: Read) -> Result<(Read, u8)> {
        let mut prefix = 0;
        loop {
            prefix |= match read.meaning.command {
                cmd::GLOBAL => 1,
                cmd::LONG => 2,
                cmd::PROTECTED => 4,
                _ => return Ok((read, prefix)),
            };
            loop {
                read = self.next_expanded()?;
                if !matches!(read.meaning.command, cmd::SPACE | cmd::RELAX) {
                    break;
                }
            }
        }
    }
    #[inline]
    fn assignment_target(&mut self, read: Read) -> Result<Target> {
        Ok(match read.meaning.command {
            cmd::COUNT => Target::count(self.scan_index()?),
            cmd::COUNT_REF => Target::count(read.meaning.value as usize),
            cmd::CATCODE => Target::catcode(self.scan_index()?),
            cmd::ENDLINE => Target::ENDLINE,
            cmd::INTEGER_PARAM => Target::parameter(read.meaning.value as usize),
            cmd::CASE_CODE => Target::casecode(read.meaning.value as usize, self.scan_index()?),
            _ => return Err(Error::Syntax),
        })
    }
    #[inline]
    fn update_integer(
        &mut self,
        target: Target,
        global: bool,
        update: impl FnOnce(i32) -> Result<i32>,
    ) -> Result<()> {
        // Resolve the slot once. Split the mutable save stack from the scalar
        // tables so the same slot handles the read, saved value, and write.
        let level = self.level;
        let save = &mut self.save;
        let slot = &mut self.scalars[target];
        let old = *slot;
        let value = update(old.value)?;
        if !global && old.value == value {
            return Ok(());
        }
        if !global && level > 0 && old.level != level {
            save.push(Save::Integer(target, old));
        }
        *slot = Integer {
            value,
            level: if global { 0 } else { level },
        };
        Ok(())
    }
    pub(crate) fn assign_meaning(&mut self, id: usize, meaning: Meaning, global: bool) {
        let old = self.bindings[id];
        if !global && old.meaning() == meaning {
            return;
        }
        let owner = meaning
            .is_macro()
            .then(|| self.values.get(meaning.value as u32));
        let old_owner = std::mem::replace(&mut self.binding_owners[id], owner);
        if !global && self.level > 0 && old.level != self.level {
            self.save.push(Save::Meaning(id, old, old_owner));
        }
        self.bindings[id] = Binding::new(meaning, if global { 0 } else { self.level });
    }
    fn assign_tokens(&mut self, index: usize, owner: Option<crate::values::Owner>, global: bool) {
        let slot = &mut self.token_registers[index];
        if !global && slot.owner.as_ref().map(|v| v.id) == owner.as_ref().map(|v| v.id) {
            return;
        }
        let value = TokenValue {
            owner,
            level: if global { 0 } else { self.level },
        };
        let old = std::mem::replace(slot, value);
        if !global && self.level > 0 && old.level != self.level {
            self.save.push(Save::Tokens(index, old));
        }
    }
    fn close_group(&mut self, kind: bool) -> Result<()> {
        if self.level == 0 {
            return Err(Error::Group);
        }
        loop {
            match self.save.pop().ok_or(Error::Group)? {
                Save::Boundary(saved) => {
                    if saved != kind {
                        return Err(Error::Group);
                    }
                    self.level -= 1;
                    return Ok(());
                }
                Save::After(token) => self.back(token),
                Save::Tokens(index, old) => {
                    if self.token_registers[index].level != 0 {
                        self.token_registers[index] = old;
                    }
                }
                Save::Meaning(id, old, owner) => {
                    if self.bindings[id].level != 0 {
                        self.bindings[id] = old;
                        self.binding_owners[id] = owner;
                    }
                }
                Save::Integer(target, old) => {
                    let slot = &mut self.scalars[target];
                    if slot.level != 0 {
                        *slot = old;
                    }
                }
            }
        }
    }
    fn print_token(&mut self, token: Token, output: &mut Vec<u8>) {
        self.token_text(token, true, |_, byte| {
            Self::print_byte(byte, output);
            Ok(())
        })
        .unwrap();
    }
    fn print_byte(byte: u8, output: &mut Vec<u8>) {
        if (32..=126).contains(&byte) {
            output.push(byte);
        } else {
            output.extend_from_slice(b"^^");
            if byte < 128 {
                output.push(byte ^ 64);
            } else {
                let hex = b"0123456789abcdef";
                output.push(hex[(byte >> 4) as usize]);
                output.push(hex[(byte & 15) as usize]);
            }
        }
    }
}
