//! One typed internal-quantity and formatting path for both expansion consumers.
use crate::{
    Engine, Result,
    numeric::{Glue, Scaled, print_scaled},
    token::*,
};
#[derive(Clone, Copy)]
pub(crate) enum Quantity {
    Integer(i32),
    Dimension(Scaled),
    Glue(Glue),
}
impl Engine<'_> {
    /// Read quad/x-height from the restored current descriptor; never ask a host.
    pub(crate) fn font_unit(&self, ex: bool) -> Scaled {
        self.font_parameter(
            self.document.fonts.current.value as usize,
            if ex { 5 } else { 6 },
        )
    }
    pub(crate) fn quantity_if_glue(&mut self, read: Read) -> Result<Option<Quantity>> {
        if matches!(
            read.meaning.command,
            cmd::SKIP | cmd::SKIP_REF | cmd::GLUE_PARAM
        ) {
            let target = self.glue_target(read)?;
            Ok(Some(Quantity::Glue(
                self.document.layout_values.glues[target].value,
            )))
        } else {
            Ok(None)
        }
    }
    pub(crate) fn quantity(&mut self, read: Read) -> Result<Option<Quantity>> {
        if let Some(value) = self.page_quantity(read)? {
            return Ok(Some(value));
        }
        if matches!(read.meaning.command, cmd::FONT_DIMEN | cmd::FONT_METRIC) {
            return Ok(Some(Quantity::Dimension(self.font_dimension(read)?)));
        }
        if matches!(
            read.meaning.command,
            cmd::DIMEN | cmd::DIMEN_REF | cmd::DIMEN_PARAM
        ) {
            let target = self.dimen_target(read)?;
            return Ok(Some(Quantity::Dimension(
                self.document.layout_values.dimens[target].value,
            )));
        }
        if let Some(value) = self.quantity_if_glue(read)? {
            return Ok(Some(value));
        }
        if matches!(
            read.meaning.command,
            cmd::FONT_INT
                | cmd::PAR_SHAPE
                | cmd::SPACE_FACTOR
                | cmd::SF_CODE
                | cmd::COUNT
                | cmd::COUNT_REF
                | cmd::CATCODE
                | cmd::ENDLINE
                | cmd::INTEGER_PARAM
                | cmd::CASE_CODE
                | cmd::CHAR_GIVEN
                | cmd::NUMEXPR
        ) {
            Ok(Some(Quantity::Integer(self.internal_int(read)?)))
        } else {
            Ok(None)
        }
    }
}
/// Width plus two glue components fit comfortably in this fixed caller buffer.
pub(crate) fn quantity_text(value: Quantity, out: &mut [u8; 128]) -> usize {
    let mut length = 0;
    let mut emit = |byte| {
        out[length] = byte;
        length += 1;
    };
    match value {
        Quantity::Integer(_) => unreachable!("integers retain their direct emission path"),
        Quantity::Dimension(v) => {
            print_scaled(v, &mut emit);
            for &b in b"pt" {
                emit(b);
            }
        }
        Quantity::Glue(g) => {
            print_scaled(g.width, &mut emit);
            for &b in b"pt" {
                emit(b);
            }
            for (amount, order, prefix) in [
                (g.stretch, g.stretch_order, b" plus ".as_slice()),
                (g.shrink, g.shrink_order, b" minus ".as_slice()),
            ] {
                if amount.0 != 0 {
                    for &b in prefix {
                        emit(b);
                    }
                    print_scaled(amount, &mut emit);
                    for &b in order.suffix() {
                        emit(b);
                    }
                }
            }
        }
    }
    length
}
