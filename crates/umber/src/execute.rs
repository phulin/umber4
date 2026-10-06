use crate::{Engine, Error, Result, token::*};

#[derive(Clone, Copy)]
pub(crate) enum Target {
    Count(usize),
    Catcode(usize),
    Endline,
}
pub(crate) enum Save {
    Boundary(bool),
    Meaning(usize, Binding),
    Integer(Target, Integer),
}

impl Engine<'_> {
    pub(crate) fn execute(&mut self, mut read: Read) -> Result<()> {
        let mut global = false;
        let mut long = false;
        while matches!(read.meaning.command, cmd::GLOBAL | cmd::LONG) {
            if read.meaning.command == cmd::GLOBAL {
                global = true;
            } else {
                long = true;
            }
            loop {
                read = self.next_expanded()?;
                if !matches!(read.meaning.command, cmd::SPACE | cmd::RELAX) {
                    break;
                }
            }
        }
        if long
            && !matches!(
                read.meaning.command,
                cmd::DEF | cmd::GDEF | cmd::EDEF | cmd::XDEF
            )
        {
            return Err(Error::Syntax);
        }
        match read.meaning.command {
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
                long,
                global || matches!(read.meaning.command, cmd::GDEF | cmd::XDEF),
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
                self.assign_meaning(id, right.meaning, global);
                Ok(())
            }
            cmd::COUNT_DEF => {
                let id = self.scan_cs()?;
                // TeX makes the destination relax before scanning its number;
                // an immediate occurrence of this name then stops lookahead.
                self.assign_meaning(id, Meaning::RELAX, global);
                self.scan_equals()?;
                let register = self.scan_index()?;
                self.assign_meaning(
                    id,
                    Meaning {
                        command: cmd::COUNT_REF,
                        value: register as i32,
                    },
                    global,
                );
                Ok(())
            }
            cmd::COUNT | cmd::COUNT_REF | cmd::CATCODE | cmd::ENDLINE => {
                let target = self.assignment_target(read)?;
                self.scan_equals()?;
                let value = self.scan_int()?;
                if matches!(target, Target::Catcode(_)) && !(0..=15).contains(&value) {
                    return Err(Error::Number);
                }
                self.assign_integer(target, value, global);
                Ok(())
            }
            cmd::ARITH => {
                let operation = read.meaning.value;
                let register = self.nonspace(true)?;
                if !matches!(
                    register.meaning.command,
                    cmd::COUNT | cmd::COUNT_REF | cmd::ENDLINE
                ) {
                    return Err(Error::Unsupported);
                }
                let target = self.assignment_target(register)?;
                self.scan_keyword(b"by")?;
                let operand = self.scan_int()?;
                let old = self.integer(target).value;
                let value = match operation {
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
                };
                self.assign_integer(target, value, global);
                Ok(())
            }
            cmd::MESSAGE if !global => {
                self.opening_brace()?;
                let mut depth = 1;
                let mut bytes = Vec::new();
                loop {
                    let token = self.next_expanded()?.token;
                    if token.is_begin() {
                        depth += 1;
                    }
                    if token.is_end() {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    self.print_token(token, &mut bytes);
                }
                self.messages.push(bytes);
                Ok(())
            }
            _ => Err(Error::Unsupported),
        }
    }
    fn assignment_target(&mut self, read: Read) -> Result<Target> {
        Ok(match read.meaning.command {
            cmd::COUNT => Target::Count(self.scan_index()?),
            cmd::COUNT_REF => Target::Count(read.meaning.value as usize),
            cmd::CATCODE => Target::Catcode(self.scan_index()?),
            cmd::ENDLINE => Target::Endline,
            _ => return Err(Error::Syntax),
        })
    }
    fn integer(&self, target: Target) -> Integer {
        match target {
            Target::Count(i) => self.counts[i],
            Target::Catcode(i) => self.catcodes[i],
            Target::Endline => self.endline,
        }
    }
    fn set_integer(&mut self, target: Target, value: Integer) {
        match target {
            Target::Count(i) => self.counts[i] = value,
            Target::Catcode(i) => self.catcodes[i] = value,
            Target::Endline => self.endline = value,
        }
    }
    pub(crate) fn assign_integer(&mut self, target: Target, value: i32, global: bool) {
        let old = self.integer(target);
        if !global && old.value == value {
            return;
        }
        if !global && self.level > 0 && old.level != self.level {
            self.save.push(Save::Integer(target, old));
        }
        self.set_integer(
            target,
            Integer {
                value,
                level: if global { 0 } else { self.level },
            },
        );
    }
    pub(crate) fn assign_meaning(&mut self, id: usize, meaning: Meaning, global: bool) {
        let old = self.bindings[id];
        if !global && old.meaning() == meaning {
            return;
        }
        if !global && self.level > 0 && old.level != self.level {
            self.save.push(Save::Meaning(id, old));
        }
        self.bindings[id] = Binding::new(meaning, if global { 0 } else { self.level });
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
                Save::Meaning(id, old) => {
                    if self.bindings[id].level != 0 {
                        self.bindings[id] = old;
                    }
                }
                Save::Integer(target, old) => {
                    if self.integer(target).level != 0 {
                        self.set_integer(target, old);
                    }
                }
            }
        }
    }
    fn print_token(&self, token: Token, output: &mut Vec<u8>) {
        if !token.is_cs() {
            Self::print_byte(token.byte(), output);
            if token.cat() == 6 {
                Self::print_byte(token.byte(), output);
            }
            return;
        }
        let id = token.id();
        if (258..514).contains(&id) {
            Self::print_byte((id - 258) as u8, output);
            return;
        }
        if id == 1 {
            output.extend_from_slice(br"\csname\endcsname ");
            return;
        }
        output.push(b'\\');
        let name = &self.names.spellings[id];
        for &byte in name {
            Self::print_byte(byte, output);
        }
        if name.len() != 1 || self.catcodes[name[0] as usize].value == 11 {
            output.push(b' ');
        }
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
