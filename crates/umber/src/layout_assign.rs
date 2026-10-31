//! Typed layout bindings use the same save-level rules as integer bindings.
use crate::{
    Engine, Result,
    execute::Save,
    layout_values::{DimenSlot, GlueSlot},
    numeric::{Glue, Scaled},
    token::*,
};
impl Engine<'_> {
    pub(crate) fn assign_dimen(&mut self, index: usize, value: Scaled, global: bool) {
        let slot = &mut self.document.layout_values.dimens[index];
        let old = *slot;
        if !global && old.value == value {
            return;
        }
        if !global && self.document.level > 0 && old.level != self.document.level {
            self.document.save.push(Save::Dimension(index, old));
        }
        *slot = DimenSlot {
            value,
            level: if global { 0 } else { self.document.level },
        };
    }
    pub(crate) fn assign_glue(&mut self, index: usize, value: Glue, global: bool) {
        // A newly scanned spec is not pointer-identical to an equal old spec in
        // upstream eq_define. Inline storage must not skip that save-level write.
        let slot = &mut self.document.layout_values.glues[index];
        let old = *slot;
        if !global && self.document.level > 0 && old.level != self.document.level {
            self.document.save.push(Save::Glue(index, old));
        }
        *slot = GlueSlot {
            value: value.canonical(),
            level: if global { 0 } else { self.document.level },
        };
    }
    pub(crate) fn layout_assignment(&mut self, read: Read, global: bool) -> Result<()> {
        if matches!(
            read.meaning.command,
            cmd::DIMEN | cmd::DIMEN_REF | cmd::DIMEN_PARAM
        ) {
            let target = self.dimen_target(read)?;
            self.scan_equals()?;
            let value = self.scan_dimen()?;
            self.assign_dimen(target, value, global);
        } else {
            let target = self.glue_target(read)?;
            self.scan_equals()?;
            let value = self.scan_glue()?;
            self.assign_glue(target, value, global);
        }
        Ok(())
    }
    pub(crate) fn layout_arithmetic(
        &mut self,
        read: Read,
        operation: i32,
        global: bool,
    ) -> Result<()> {
        if matches!(
            read.meaning.command,
            cmd::DIMEN | cmd::DIMEN_REF | cmd::DIMEN_PARAM
        ) {
            let target = self.dimen_target(read)?;
            self.scan_keyword(b"by")?;
            let old = self.document.layout_values.dimens[target].value;
            let value = match operation {
                0 => old.advance(self.scan_dimen()?),
                1 => old.multiply(self.scan_int()?)?,
                2 => old.divide(self.scan_int()?)?,
                _ => unreachable!(),
            };
            self.assign_dimen(target, value, global);
        } else {
            let target = self.glue_target(read)?;
            self.scan_keyword(b"by")?;
            let old = self.document.layout_values.glues[target].value;
            let value = if operation == 0 {
                old.advance(self.scan_glue()?)
            } else {
                old.arithmetic(self.scan_int()?, operation == 2)?
            };
            self.assign_glue(target, value, global);
        }
        Ok(())
    }
}
