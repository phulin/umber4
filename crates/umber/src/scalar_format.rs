use super::*;
use crate::{
    Error, Result,
    format::wire::{Reader, Writer},
};
pub(crate) fn write(s: &Scalars, w: &mut Writer) -> Result<()> {
    w.count(SLOTS)?;
    for slot in &s.0 {
        if slot.level != 0 {
            return Err(Error::Checkpoint);
        }
        w.i32(slot.value);
    }
    Ok(())
}
pub(crate) fn read(r: &mut Reader<'_>, fonts: usize) -> Result<Scalars> {
    if r.count(4)? != SLOTS {
        return Err(Error::Format);
    }
    let mut s = Scalars::default();
    for slot in &mut s.0 {
        slot.value = r.i32()?;
    }
    for byte in 0..256 {
        if !(0..16).contains(&s[Target::catcode(byte)].value)
            || !(0..=32768).contains(&s[Target::mathcode(byte)].value)
            || !(0..=32767).contains(&s[Target::sfcode(byte)].value)
            || s[Target::delcode(byte)].value > 0xFF_FFFF
            || (0..2).any(|case| !(0..=255).contains(&s[Target::casecode(case, byte)].value))
        {
            return Err(Error::Format);
        }
    }
    for style in 0..3 {
        for family in 0..16 {
            let font = s[Target::family(style, family)].value;
            if font < 0 || font as usize >= fonts {
                return Err(Error::Format);
            }
        }
    }
    Ok(s)
}
