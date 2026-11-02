//! Canonical nullfont image; complete font/resource records use tag 1.
use super::*;
use crate::format::wire::{Reader, Writer};
pub(crate) fn write(e: &Engine<'_>, w: &mut Writer) -> Result<()> {
    let fonts = &e.document.fonts;
    let font = fonts.descriptors[0];
    if fonts.len != 1
        || fonts.current.value != 0
        || font.scaled.is_some()
        || font.count != 7
        || font.params != 0
        || fonts.parameter_end != 7
        || font.hyphen != 45
        || font.skew != -1
        || Some(font.identifier as u32) != e.names.lookup(b"nullfont")
        || (0..7).any(|i| fonts.parameters[i] != Scaled::ZERO)
    {
        return Err(Error::Unsupported);
    }
    if fonts.current.level != 0 {
        return Err(Error::Checkpoint);
    }
    w.u8(0);
    Ok(())
}
pub(crate) fn read(
    e: &mut Engine<'_>,
    r: &mut Reader<'_>,
    _name_count: usize,
    extras: &[(&[u8], &[u8])],
) -> Result<()> {
    match r.u8()? {
        0 => {}
        1 => return Err(Error::Unsupported),
        _ => return Err(Error::Format),
    }
    for &(name, bytes) in extras {
        e.font_catalog.register_format_resource(name, bytes)?;
    }
    Ok(())
}
