//! Delimiter scanning and paired semantic groups; conversion stays deferred.
use super::*;

impl Engine<'_> {
    pub(super) fn scan_delimiter_code(&mut self) -> Result<i32> {
        let code = self.scan_int()?;
        if !(0..=0x7ff_ffff).contains(&code) {
            return Err(Error::Number);
        }
        Ok(code)
    }
    pub(super) fn scan_math_delimiter(&mut self) -> Result<Delimiter> {
        let read = loop {
            let read = self.nonspace(true)?;
            if read.meaning.command != cmd::RELAX {
                break read;
            }
        };
        let code = match read.meaning.command {
            11 | 12 => self.document.scalars[Target::delcode(read.meaning.value as usize)].value,
            cmd::DELIMITER => self.scan_delimiter_code()?,
            _ => -1,
        };
        if code < 0 {
            self.back_read(read)?;
            return Err(Error::Syntax);
        }
        Ok(Delimiter::from_code(code))
    }
    pub(super) fn math_left_right(&mut self, right: bool) -> Result<()> {
        if right && self.current_group() != Some(GroupKind::MathLeft) {
            if self.current_group() == Some(GroupKind::MathShift) {
                self.scan_math_delimiter()?;
                return Err(Error::Syntax);
            }
            return Err(Error::Group);
        }
        // Root the fresh boundary before operand scanning, as native does.
        // Failures release only this temporary, never a suspended formula.
        let mut boundary = MathList::default();
        let id = self.math.as_mut().unwrap().append(
            &mut boundary,
            Kind::Boundary {
                side: if right { Side::Right } else { Side::Left },
                delimiter: Delimiter::default(),
            },
            &mut self.document.nodes,
        )?;
        let delimiter = match self.scan_math_delimiter() {
            Ok(delimiter) => delimiter,
            Err(error) => {
                self.math
                    .as_mut()
                    .unwrap()
                    .release(boundary, &mut self.document.nodes);
                return Err(error);
            }
        };
        self.math
            .as_mut()
            .unwrap()
            .set_boundary_delimiter(id, delimiter);
        if !right {
            if let Err(error) = self.open_group(GroupKind::MathLeft) {
                self.math
                    .as_mut()
                    .unwrap()
                    .release(boundary, &mut self.document.nodes);
                return Err(error);
            }
            let enclosing_mode = std::mem::replace(&mut self.document.text.mode, Mode::Math);
            self.document.math_frames.push(Frame {
                list: boundary,
                incomplete: None,
                context: Context::LeftRight,
                level: self.document.level,
                enclosing_mode,
            });
            return Ok(());
        }
        let frame = self.document.math_frames.pop().expect("left/right owner");
        let Context::LeftRight = frame.context else {
            unreachable!("left/right group")
        };
        debug_assert_eq!(frame.level, self.document.level);
        self.document.text.mode = frame.enclosing_mode;
        let list =
            self.math
                .as_mut()
                .unwrap()
                .finish_list(frame.list, frame.incomplete, Some(boundary));
        if let Err(error) = self.close_group(GroupKind::MathLeft) {
            self.math
                .as_mut()
                .unwrap()
                .release(list, &mut self.document.nodes);
            return Err(error);
        }
        self.append_math(Kind::Atom(Atom::new(Class::Inner, Field::SubMath(list))))?;
        Ok(())
    }
}
