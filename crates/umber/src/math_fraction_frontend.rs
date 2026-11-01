//! Rooted generalized-fraction construction; all measurement stays in the core.
use super::*;

impl Engine<'_> {
    pub(super) fn math_fraction(&mut self, operation: i32) -> Result<()> {
        let duplicate = self
            .document
            .math_frames
            .last()
            .unwrap()
            .incomplete
            .is_some();
        let id = if !duplicate {
            let frame = self.document.math_frames.last_mut().unwrap();
            let incomplete = self
                .math
                .as_mut()
                .unwrap()
                .start_fraction(&mut frame.list, &mut self.document.nodes)?;
            let id = incomplete.tail().unwrap();
            frame.incomplete = Some(incomplete);
            Some(id)
        } else {
            None
        };
        // Even an ambiguous second fraction scans all of its own operands.
        // The already rooted numerator and denominator are never replaced.
        let (left, right) = if operation >= 3 {
            (self.scan_math_delimiter()?, self.scan_math_delimiter()?)
        } else {
            (Delimiter::default(), Delimiter::default())
        };
        let thickness = match operation % 3 {
            0 => RuleThickness::Explicit(self.scan_dimen()?),
            1 => RuleThickness::Default,
            _ => RuleThickness::Explicit(Scaled::ZERO),
        };
        let Some(id) = id else {
            return Err(Error::Syntax);
        };
        self.math
            .as_mut()
            .unwrap()
            .set_fraction_parameters(id, left, right, thickness);
        Ok(())
    }
}
