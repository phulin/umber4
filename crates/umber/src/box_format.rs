//! Affine ordinary box-register forests in dependency order.
use super::*;
use crate::format::wire::{Reader, Writer};
pub(crate) fn write(e: &Engine<'_>, w: &mut Writer) -> Result<()> {
    let mut roots = [NodeId::NULL; 256];
    for (i, root) in roots.iter_mut().enumerate() {
        let slot = &e.document.boxes.0[i];
        if slot.level != 0 {
            return Err(Error::Checkpoint);
        }
        if slot.list.head() != slot.list.tail() {
            return Err(Error::Format);
        }
        *root = slot.list.head();
    }
    crate::nodes::format::write(e, w, &roots)
}
pub(crate) fn read(e: &mut Engine<'_>, r: &mut Reader<'_>) -> Result<()> {
    let mut nodes = crate::nodes::NodeStore::default();
    let roots = crate::nodes::format::read(e, r, &mut nodes)?;
    let mut boxes = Registers::default();
    for (i, list) in roots.into_iter().enumerate() {
        boxes.0[i] = Slot { list, level: 0 };
    }
    // No fallible operation separates publication of the paired fresh roots.
    e.document.nodes = nodes;
    e.document.boxes = boxes;
    Ok(())
}

#[cfg(test)]
#[path = "box_format_tests.rs"]
mod tests;
