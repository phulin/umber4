//! Empty pattern image; startup still freezes further pattern assignments.
use super::*;
use crate::format::wire::{Reader, Writer};
pub(crate) fn write(h: &Hyphenation, w: &mut Writer) -> Result<()> {
    if h.patterns.is_some() || h.exceptions.is_some() {
        return Err(Error::Unsupported);
    }
    w.u8(0);
    Ok(())
}
pub(crate) fn read(r: &mut Reader<'_>) -> Result<Hyphenation> {
    match r.u8()? {
        0 => {}
        1 => return Err(Error::Unsupported),
        _ => return Err(Error::Format),
    }
    Ok(Hyphenation {
        frozen: true,
        ..Hyphenation::default()
    })
}
