//! Paragraph-initial language and zero-width language whatsits.
use crate::{
    Engine, Error, Result,
    nodes::Kind,
    scalars::{Scalars, Target},
    text::Mode,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Language {
    pub number: u8,
    pub left: u8,
    pub right: u8,
}
impl Default for Language {
    fn default() -> Self {
        Self {
            number: 0,
            left: 1,
            right: 1,
        }
    }
}
impl Language {
    pub fn normalize(number: i32) -> u8 {
        if (1..=255).contains(&number) {
            number as u8
        } else {
            0
        }
    }
    pub fn capture(scalars: &Scalars, number: i32) -> Self {
        Self {
            number: Self::normalize(number),
            left: scalars[Target::LEFT_HYPHEN_MIN].value.clamp(1, 63) as u8,
            right: scalars[Target::RIGHT_HYPHEN_MIN].value.clamp(1, 63) as u8,
        }
    }
}
impl Engine<'_> {
    pub(crate) fn fix_language(&mut self) -> Result<()> {
        if self.document.text.mode == Mode::Horizontal {
            let number = self.document.scalars[Target::LANGUAGE].value;
            if Language::normalize(number) != self.document.text.language {
                self.append_language(number)?;
            }
        }
        Ok(())
    }
    pub(crate) fn set_language(&mut self) -> Result<()> {
        if !self.document.text.mode.is_horizontal() {
            return Err(Error::Syntax);
        }
        // Native new_whatsit publishes the owner before scanning the operand.
        // A scanner error must leave that node reachable until restore.
        let node = self
            .document
            .nodes
            .alloc(Kind::Language(Language::default()))?;
        let id = node.id();
        self.document
            .nodes
            .append(&mut self.document.text.horizontal, node);
        let number = self.scan_int()?;
        let language = Language::capture(&self.document.scalars, number);
        *self.document.nodes.kind_mut(id) = Kind::Language(language);
        self.document.text.language = language.number;
        Ok(())
    }
    fn append_language(&mut self, number: i32) -> Result<()> {
        let language = Language::capture(&self.document.scalars, number);
        self.append_horizontal(Kind::Language(language))?;
        self.document.text.language = language.number;
        Ok(())
    }
}

#[cfg(test)]
#[path = "language_tests.rs"]
mod tests;
