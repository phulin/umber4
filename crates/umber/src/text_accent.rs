//! TeX make_accent: direct glyphs, intervening assignments and protective kerns.
use crate::{
    Engine, Error, Result,
    nodes::{DetachedNode, KernKind, Kind, List},
    numeric::Scaled,
    packing::PackSpec,
    token::cmd,
};

#[derive(Default)]
struct Work {
    accent: List,
    base: List,
    output: List,
    #[cfg(test)]
    fail_after: Option<usize>,
}
impl Work {
    fn admit(&mut self, engine: &mut Engine<'_>, node: DetachedNode) -> Result<DetachedNode> {
        #[cfg(test)]
        if let Some(remaining) = &mut self.fail_after {
            if *remaining == 0 {
                engine
                    .document
                    .nodes
                    .release_node(node, &mut engine.page_release);
                return Err(Error::Capacity);
            }
            *remaining -= 1;
        }
        #[cfg(not(test))]
        let _ = engine;
        Ok(node)
    }
    fn alloc(&mut self, engine: &mut Engine<'_>, kind: Kind) -> Result<DetachedNode> {
        let node = engine.document.nodes.alloc(kind)?;
        self.admit(engine, node)
    }
    fn release(&mut self, engine: &mut Engine<'_>) {
        for list in [&mut self.accent, &mut self.base, &mut self.output] {
            engine
                .document
                .nodes
                .release(std::mem::take(list), &mut engine.page_release);
        }
    }
}
impl Engine<'_> {
    pub(crate) fn make_text_accent(&mut self) -> Result<()> {
        self.text_accent_owned(&mut Work::default())
    }
    fn text_accent_owned(&mut self, work: &mut Work) -> Result<()> {
        let result = self.text_accent_inner(work);
        work.release(self);
        result
    }
    fn text_accent_inner(&mut self, work: &mut Work) -> Result<()> {
        if !self.document.text.mode.is_horizontal() {
            return Err(Error::Syntax);
        }
        let character = self.scan_index()? as u8;
        let font = self.document.fonts.current.value as u32;
        let Some(accent) = self.character_metrics(font, character) else {
            // No lookahead, assignments or SF reset when new_character fails.
            return Ok(());
        };
        let node = work.alloc(self, Kind::Character { font, character })?;
        self.document.nodes.append(&mut work.accent, node);
        let x = self.font_parameter(font as usize, 5).0;
        let s = f64::from(self.font_parameter(font as usize, 1).0) / 65536.0;
        let read = self.do_assignments()?;
        let base_character = match read.meaning.command {
            11 | 12 | cmd::CHAR_GIVEN => Some(read.meaning.value as u8),
            cmd::CHAR => Some(self.scan_index()? as u8),
            _ => {
                self.back_read(read)?;
                None
            }
        };
        let font = self.document.fonts.current.value as u32;
        if let Some(character) = base_character
            && let Some(base) = self.character_metrics(font, character)
        {
            let node = work.alloc(self, Kind::Character { font, character })?;
            self.document.nodes.append(&mut work.base, node);
            let t = f64::from(self.font_parameter(font as usize, 1).0) / 65536.0;
            if base.height.0 != x {
                let boxed = self.hpack(std::mem::take(&mut work.accent), PackSpec::Natural)?;
                let boxed = work.admit(self, boxed)?;
                let id = boxed.id();
                self.document.nodes.append(&mut work.accent, boxed);
                self.document.nodes.box_mut(id).shift = Scaled(x.wrapping_sub(base.height.0));
            }
            let displacement = f64::from(base.width.0.wrapping_sub(accent.width.0)) / 2.0
                + f64::from(base.height.0) * t
                - f64::from(x) * s;
            // Web2C round uses signed nearest rounding and saturates at
            // +/-max_integer. Negation/subtraction for the second kern are
            // subsequent word operations, including their native wrapping.
            let delta = displacement
                .round()
                .clamp(-f64::from(i32::MAX), f64::from(i32::MAX)) as i32;
            let first = work.alloc(
                self,
                Kind::Kern {
                    width: Scaled(delta),
                    kind: KernKind::Accent,
                },
            )?;
            self.document.nodes.append(&mut work.output, first);
            self.document
                .nodes
                .splice(&mut work.output, std::mem::take(&mut work.accent));
            let second = work.alloc(
                self,
                Kind::Kern {
                    width: Scaled(accent.width.0.wrapping_neg().wrapping_sub(delta)),
                    kind: KernKind::Accent,
                },
            )?;
            self.document.nodes.append(&mut work.output, second);
            self.document
                .nodes
                .splice(&mut work.output, std::mem::take(&mut work.base));
        } else {
            self.document
                .nodes
                .splice(&mut work.output, std::mem::take(&mut work.accent));
        }
        self.document.nodes.splice(
            &mut self.document.text.horizontal,
            std::mem::take(&mut work.output),
        );
        self.document.text.space_factor = 1000;
        Ok(())
    }
}

#[cfg(test)]
#[path = "text_accent_tests.rs"]
mod tests;
