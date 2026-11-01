use crate::{Engine, Error, Result, scalars::Target, token::*};
#[derive(Clone)]
pub(crate) enum Save {
    Boundary(bool),
    Meaning(usize, Binding, Option<crate::values::Owner>),
    Integer(Target, Integer),
    Dimension(usize, crate::layout_values::DimenSlot),
    Glue(usize, crate::layout_values::GlueSlot),
    Tokens(usize, TokenValue),
    After(Token),
    Font(Integer),
    Shape(crate::layout_values::ShapeSlot),
}

impl Engine<'_> {
    pub(crate) fn execute(&mut self, read: Read) -> Result<()> {
        if read.meaning.command == cmd::RELAX {
            return Ok(());
        }
        if self.text_command(read)? {
            return Ok(());
        }
        let (read, prefix) = if matches!(
            read.meaning.command,
            cmd::GLOBAL | cmd::LONG | cmd::PROTECTED | cmd::OUTER
        ) {
            self.assignment_prefixes(read)?
        } else {
            (read, 0)
        };
        let global = prefix & 1 != 0;
        if prefix & 14 != 0
            && !matches!(
                read.meaning.command,
                cmd::DEF | cmd::GDEF | cmd::EDEF | cmd::XDEF
            )
        {
            return Err(Error::Syntax);
        }
        let assignment_global = match self.document.scalars[Target::GLOBALDEFS].value {
            n if n > 0 => true,
            n if n < 0 => false,
            _ => global || matches!(read.meaning.command, cmd::GDEF | cmd::XDEF),
        };
        let result = match read.meaning.command {
            cmd::PAGE_DIMEN | cmd::PREV_DEPTH => self.page_assignment(read),
            cmd::PAR_SHAPE => self.shape_assignment(assignment_global),
            cmd::BEGIN | cmd::BEGIN_GROUP if !global => {
                self.document.level = self.document.level.checked_add(1).ok_or(Error::Capacity)?;
                self.document
                    .save
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
                prefix & 8 != 0,
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
            cmd::COUNT_DEF
            | cmd::CHAR_DEF
            | cmd::MATH_CHAR_DEF
            | cmd::TOKS_DEF
            | cmd::DIMEN_DEF
            | cmd::SKIP_DEF => {
                let id = self.scan_cs()?;
                self.assign_meaning(id, Meaning::RELAX, assignment_global);
                self.scan_equals()?;
                let value = if read.meaning.command == cmd::MATH_CHAR_DEF {
                    let value = self.scan_int()?;
                    if !(0..=32767).contains(&value) {
                        return Err(Error::Number);
                    }
                    value
                } else {
                    self.scan_index()? as i32
                };
                let command = match read.meaning.command {
                    cmd::COUNT_DEF => cmd::COUNT_REF,
                    cmd::TOKS_DEF => cmd::TOKS_REF,
                    cmd::DIMEN_DEF => cmd::DIMEN_REF,
                    cmd::SKIP_DEF => cmd::SKIP_REF,
                    cmd::MATH_CHAR_DEF => cmd::MATH_GIVEN,
                    _ => cmd::CHAR_GIVEN,
                };
                self.assign_meaning(id, Meaning { command, value }, assignment_global);
                Ok(())
            }
            cmd::TOKS | cmd::TOKS_REF | cmd::TOKS_PARAM => {
                let index = self.toks_index(read)?;
                self.scan_equals()?;
                let first = loop {
                    let read = self.nonspace(true)?;
                    if read.meaning.command != cmd::RELAX {
                        break read;
                    }
                };
                let owner = if matches!(
                    first.meaning.command,
                    cmd::TOKS | cmd::TOKS_REF | cmd::TOKS_PARAM
                ) {
                    let from = self.toks_index(first)?;
                    self.document.token_registers[from].owner.clone()
                } else if first.meaning.command == cmd::BEGIN {
                    Some(self.store_body()?)
                } else {
                    return Err(Error::Syntax);
                };
                self.assign_tokens(index, owner, assignment_global);
                Ok(())
            }
            cmd::DIMEN
            | cmd::DIMEN_REF
            | cmd::DIMEN_PARAM
            | cmd::SKIP
            | cmd::SKIP_REF
            | cmd::GLUE_PARAM => self.layout_assignment(read, assignment_global),
            cmd::FONT | cmd::SELECT_FONT | cmd::FONT_DIMEN | cmd::FONT_INT | cmd::SPACE_FACTOR => {
                self.font_assignment(read, assignment_global)
            }
            cmd::SF_CODE
            | cmd::MATH_CODE
            | cmd::DEL_CODE
            | cmd::COUNT
            | cmd::COUNT_REF
            | cmd::CATCODE
            | cmd::ENDLINE
            | cmd::INTEGER_PARAM
            | cmd::CASE_CODE => {
                let target = self.scalar_target(read)?;
                self.scan_equals()?;
                let value = self.scan_int()?;
                if read.meaning.command == cmd::CATCODE && !(0..=15).contains(&value) {
                    return Err(Error::Number);
                }
                if read.meaning.command == cmd::SF_CODE && !(0..=32767).contains(&value) {
                    return Err(Error::Number);
                }
                if read.meaning.command == cmd::CASE_CODE && !(0..=255).contains(&value) {
                    return Err(Error::Number);
                }
                if read.meaning.command == cmd::MATH_CODE && !(0..=32768).contains(&value) {
                    return Err(Error::Number);
                }
                if read.meaning.command == cmd::DEL_CODE && value > 0xFF_FFFF {
                    return Err(Error::Number);
                }
                self.update_integer(target, assignment_global, |_| Ok(value))
            }
            cmd::ARITH => {
                let operation = read.meaning.value;
                let register = self.nonspace(true)?;
                if matches!(
                    register.meaning.command,
                    cmd::DIMEN
                        | cmd::DIMEN_REF
                        | cmd::DIMEN_PARAM
                        | cmd::SKIP
                        | cmd::SKIP_REF
                        | cmd::GLUE_PARAM
                ) {
                    self.layout_arithmetic(register, operation, assignment_global)
                } else {
                    if !matches!(
                        register.meaning.command,
                        cmd::COUNT | cmd::COUNT_REF | cmd::ENDLINE | cmd::INTEGER_PARAM
                    ) {
                        return Err(Error::Unsupported);
                    }
                    let target = self.scalar_target(register)?;
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
            }
            cmd::AFTER_ASSIGN if !global => {
                self.document.after_assignment = Some(self.next_raw()?.token);
                Ok(())
            }
            cmd::AFTER_GROUP if !global => {
                let token = self.next_raw()?.token;
                if self.document.level > 0 {
                    self.document.save.push(Save::After(token));
                }
                Ok(())
            }
            cmd::CHANGE_CASE if !global => self.change_case(read.meaning.value as usize),
            cmd::EXTENSION if !global => self.extension(read.meaning.value),
            cmd::MESSAGE if !global => {
                self.with_scanner(crate::scanner::Scanner::Absorbing, |engine| {
                    engine.opening_brace()?;
                    let mut bytes = Vec::new();
                    engine.scan_body::<true>(None, &mut |engine: &mut Self, token: Token| {
                        engine.print_token(token, &mut bytes);
                        Ok(())
                    })?;
                    engine
                        .document
                        .diagnostics
                        .push(crate::Diagnostic::Message(bytes));
                    Ok(())
                })
            }
            _ => Err(Error::Unsupported),
        };
        if result.is_ok()
            && let Some(token) = self.document.after_assignment
            && cmd::is_assignment(read.meaning.command)
        {
            self.document.after_assignment = None;
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
                cmd::OUTER => 8,
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
    pub(crate) fn update_integer(
        &mut self,
        target: Target,
        global: bool,
        update: impl FnOnce(i32) -> Result<i32>,
    ) -> Result<()> {
        // Resolve the slot once. Split the mutable save stack from the scalar
        // tables so the same slot handles the read, saved value, and write.
        let level = self.document.level;
        let save = &mut self.document.save;
        let slot = &mut self.document.scalars[target];
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
        let old = self.document.bindings[id];
        if !global && old.meaning() == meaning {
            return;
        }
        let owner = meaning
            .is_macro()
            .then(|| self.values.get(meaning.value as u32));
        let old_owner = self.document.bindings.replace_owner(id, owner);
        if !global && self.document.level > 0 && old.level != self.document.level {
            self.document.save.push(Save::Meaning(id, old, old_owner));
        }
        self.document.bindings[id] =
            Binding::new(meaning, if global { 0 } else { self.document.level });
    }
    fn assign_tokens(&mut self, index: usize, owner: Option<crate::values::Owner>, global: bool) {
        let slot = &mut self.document.token_registers[index];
        if !global && slot.owner.as_ref().map(|v| v.id) == owner.as_ref().map(|v| v.id) {
            return;
        }
        let value = TokenValue {
            owner,
            level: if global { 0 } else { self.document.level },
        };
        let old = std::mem::replace(slot, value);
        if !global && self.document.level > 0 && old.level != self.document.level {
            self.document.save.push(Save::Tokens(index, old));
        }
    }
    fn close_group(&mut self, kind: bool) -> Result<()> {
        if self.document.level == 0 {
            return Err(Error::Group);
        }
        loop {
            match self.document.save.pop().ok_or(Error::Group)? {
                Save::Boundary(saved) => {
                    if saved != kind {
                        return Err(Error::Group);
                    }
                    self.document.level -= 1;
                    return Ok(());
                }
                Save::Font(old) => {
                    if self.document.fonts.current.level != 0 {
                        self.document.fonts.current = old;
                    }
                }
                Save::Shape(old) => {
                    if self.document.layout_values.shape.level != 0 {
                        self.document.layout_values.shape = old;
                    }
                }
                Save::After(token) => self.back(token),
                Save::Tokens(index, old) => {
                    if self.document.token_registers[index].level != 0 {
                        self.document.token_registers[index] = old;
                    }
                }
                Save::Meaning(id, old, owner) => {
                    if self.document.bindings[id].level != 0 {
                        self.document.bindings[id] = old;
                        self.document.bindings.replace_owner(id, owner);
                    }
                }
                Save::Dimension(index, old) => {
                    let slot = &mut self.document.layout_values.dimens[index];
                    if slot.level != 0 {
                        *slot = old;
                    }
                }
                Save::Glue(index, old) => {
                    let slot = &mut self.document.layout_values.glues[index];
                    if slot.level != 0 {
                        *slot = old;
                    }
                }
                Save::Integer(target, old) => {
                    let slot = &mut self.document.scalars[target];
                    if slot.level != 0 {
                        *slot = old;
                    }
                }
            }
        }
    }
    pub(crate) fn print_token(&mut self, token: Token, output: &mut Vec<u8>) {
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
