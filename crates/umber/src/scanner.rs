//! Command-lifetime scanner validity; no context survives an outer handoff.
use crate::{Engine, Error, Result, token::*};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Scanner {
    #[default]
    Normal,
    Defining,
    Matching,
    Absorbing,
    Skipping,
    Aligning,
}

impl Engine<'_> {
    pub(crate) fn with_scanner<T>(
        &mut self,
        scanner: Scanner,
        action: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        let saved = std::mem::replace(&mut self.scanner, scanner);
        let result = action(self);
        self.scanner = saved;
        result
    }
    #[inline]
    pub(crate) fn check_outer(&self, meaning: Meaning) -> Result<()> {
        if self.scanner != Scanner::Normal
            && (meaning.is_outer_macro()
                || matches!(meaning.command, cmd::END_WRITE | cmd::END_TEMPLATE))
        {
            Err(Error::Outer)
        } else {
            Ok(())
        }
    }
    #[inline]
    pub(crate) fn check_raw_outer(&self, raw: Raw) -> Result<()> {
        if self.scanner != Scanner::Normal && !raw.suppress && raw.token.is_cs() {
            self.check_outer(self.resolve(raw.token))?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "scanner_tests.rs"]
mod tests;
