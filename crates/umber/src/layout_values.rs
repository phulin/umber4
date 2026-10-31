//! Typed COW dimension/glue bindings; no node ownership is stored in these slots.
use crate::{
    numeric::{Glue, Scaled},
    tables::Table,
};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DimenSlot {
    pub value: Scaled,
    pub level: u16,
}
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GlueSlot {
    pub value: Glue,
    pub level: u16,
}

pub(crate) const REGISTERS: usize = 256;
#[derive(Clone)]
pub(crate) struct LayoutValues {
    pub dimens: Table<DimenSlot>,
    pub glues: Table<GlueSlot>,
    pub prepared_mag: Option<i32>,
    pub shape: ShapeSlot,
}
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct LineShape {
    pub indent: Scaled,
    pub width: Scaled,
}
#[derive(Clone, Default)]
pub(crate) struct ShapeSlot {
    pub lines: Option<Table<LineShape>>,
    pub level: u16,
}
impl Default for LayoutValues {
    fn default() -> Self {
        Self {
            dimens: Table::new(REGISTERS),
            glues: Table::new(REGISTERS),
            prepared_mag: None,
            shape: ShapeSlot::default(),
        }
    }
}

// Inventory order in token.rs is the stable index within each typed table.
// Consumers use these names rather than repeating the register-region offset.
#[allow(dead_code)] // The following text/paragraph/page slices consume these IDs.
#[derive(Clone, Copy)]
#[repr(usize)]
pub(crate) enum DimenParam {
    HSize,
    VSize,
    MaxDepth,
    ParIndent,
    HangIndent,
    EmergencyStretch,
    LineSkipLimit,
    HFuzz,
    VFuzz,
}
#[allow(dead_code)]
impl DimenParam {
    pub const fn index(self) -> usize {
        REGISTERS + self as usize
    }
}
#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(usize)]
pub(crate) enum GlueParam {
    Baseline,
    Line,
    Paragraph,
    Top,
    Left,
    Right,
    ParFill,
    Space,
    XSpace,
}
#[allow(dead_code)]
impl GlueParam {
    pub const fn index(self) -> usize {
        REGISTERS + self as usize
    }
}
