//! Immediate math-field operands return to the ordinary driver for groups.
use super::*;
impl Engine<'_> {
    pub(super) fn math_character_code(&mut self, read: Read) -> Result<Option<i32>> {
        let code = match read.meaning.command {
            cmd::MATH_GIVEN => read.meaning.value,
            cmd::MATH_CHAR => self.scan_math_char_code()?,
            cmd::DELIMITER => self.scan_delimiter_code()? >> 12,
            _ => {
                let character = if read.meaning.command == cmd::CHAR {
                    self.scan_index()? as u8
                } else {
                    read.meaning.value as u8
                };
                let code = self.document.scalars[Target::mathcode(character as usize)].value;
                if code == 0x8000 {
                    self.back(Token::active(character));
                    return Ok(None);
                }
                code
            }
        };
        Ok(Some(code))
    }
    pub(super) fn math_character_field(&self, code: i32) -> Field {
        let c = self.math_character(code);
        Field::Character {
            family: c.family,
            character: c.character,
        }
    }
    pub(super) fn scan_math_char_code(&mut self) -> Result<i32> {
        let code = self.scan_int()?;
        if !(0..=0x7fff).contains(&code) {
            return Err(Error::Number);
        }
        Ok(code)
    }
    pub(super) fn math_character(&self, code: i32) -> MathChar {
        let fam = self.document.scalars[Target::FAM].value;
        let family = if code >= 0x7000 && (0..16).contains(&fam) {
            fam as u8
        } else {
            ((code >> 8) & 15) as u8
        };
        MathChar {
            family,
            character: code as u8,
        }
    }
    pub(super) fn scan_math_field(&mut self, target: FieldTarget) -> Result<()> {
        loop {
            let read = self.nonspace(true)?;
            match read.meaning.command {
                cmd::RELAX => continue,
                11
                | 12
                | cmd::CHAR_GIVEN
                | cmd::CHAR
                | cmd::MATH_GIVEN
                | cmd::MATH_CHAR
                | cmd::DELIMITER => {
                    let Some(code) = self.math_character_code(read)? else {
                        continue;
                    };
                    let field = self.math_character_field(code);
                    return self.math.as_mut().unwrap().fill_field(
                        target,
                        field,
                        &mut self.document.nodes,
                    );
                }
                cmd::BEGIN => {
                    self.open_group(GroupKind::MathField)?;
                    self.document.math_frames.push(Frame {
                        list: MathList::default(),
                        incomplete: None,
                        context: Context::Field(target),
                        level: self.document.level,
                    });
                    return Ok(());
                }
                _ => return Err(Error::Syntax),
            }
        }
    }
    pub(super) fn math_script(&mut self, superscript: bool) -> Result<()> {
        let tail = self.document.math_frames.last().unwrap().list.tail();
        let atom = match tail {
            Some(id) if self.math.as_ref().unwrap().scripts_allowed(id) => id,
            _ => self.append_math(Kind::Atom(Atom::new(Class::Ord, Field::Empty)))?,
        };
        let target = FieldTarget {
            atom,
            slot: if superscript {
                FieldSlot::Sup
            } else {
                FieldSlot::Sub
            },
        };
        if !self.math.as_mut().unwrap().field_empty(target) {
            return Err(Error::Syntax);
        }
        self.scan_math_field(target)
    }
}
