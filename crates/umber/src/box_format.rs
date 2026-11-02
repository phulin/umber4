//! Void-register image; live ordinary box forests require the complete codec.
use super::*;
use crate::format::wire::{Reader, Writer};
pub(crate) fn write(e: &Engine<'_>, w: &mut Writer) -> Result<()> {
    for i in 0..256 {
        if !e.document.boxes.0[i].list.is_empty() {
            return Err(Error::Unsupported);
        }
        if e.document.boxes.0[i].level != 0 {
            return Err(Error::Checkpoint);
        }
    }
    w.u8(0);
    Ok(())
}
pub(crate) fn read(_e: &mut Engine<'_>, r: &mut Reader<'_>) -> Result<()> {
    match r.u8()? {
        0 => Ok(()),
        1 => Err(Error::Unsupported),
        _ => Err(Error::Format),
    }
}
