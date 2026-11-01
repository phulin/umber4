//! Root typed forms before their immediate selector and field scans.
use super::*;

impl Engine<'_> {
    pub(super) fn math_form(&mut self, operation: i32) -> Result<()> {
        let fields = Fields::default();
        let kind = match operation {
            0 => Kind::Over(fields),
            1 => Kind::Under(fields),
            2 => Kind::Radical {
                fields,
                delimiter: Delimiter::default(),
            },
            _ => Kind::Accent {
                fields,
                accent: MathChar {
                    family: 0,
                    character: 0,
                },
            },
        };
        let atom = self.append_math(kind)?;
        if operation == 2 {
            let delimiter = Delimiter::from_code(self.scan_delimiter_code()?);
            self.math
                .as_mut()
                .unwrap()
                .set_radical_delimiter(atom, delimiter);
        } else if operation == 3 {
            let code = self.scan_math_char_code()?;
            let accent = self.math_character(code);
            self.math.as_mut().unwrap().set_math_accent(atom, accent);
        }
        self.scan_math_field(FieldTarget {
            atom,
            slot: FieldSlot::Nucleus,
        })
    }
}
