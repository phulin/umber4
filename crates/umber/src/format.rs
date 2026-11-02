//! Portable initial semantic state, independent of source-bound checkpoints.
use crate::{Engine, Error, InputFile, InputResolver, Result, token::*};
#[path = "format_tables.rs"]
mod tables;
#[path = "format_validate.rs"]
mod validate;
#[path = "format_values.rs"]
mod values;
#[path = "format_wire.rs"]
pub(crate) mod wire;
use wire::{Reader, Writer};
const MAGIC: &[u8; 8] = b"UMBERFMT";
const VERSION: u32 = 1;
const PROFILE: u32 = 1;

impl<'a> Engine<'a> {
    fn format_state_ready(&self) -> bool {
        self.scanner == crate::scanner::Scanner::Normal
            && self.document.text.mode == crate::text::Mode::Vertical
            && self.document.level == 0
            && self.document.save.is_empty()
            && self.document.conditions.is_empty()
            && self.document.after_assignment.is_none()
            && self.document.box_frames.is_empty()
            && self.document.math_frames.is_empty()
            && self.document.alignment_frames.is_empty()
            && self.document.alignment_layouts.is_empty()
            && !self.alignment_reader.active()
            && self.document.output_frame.is_none()
            && self.document.text.horizontal.is_empty()
            && self.document.text.vertical.is_empty()
            && self.document.page.current.is_empty()
            && self.document.page.output.is_empty()
            && self.document.page.dead_cycles == 0
            && self.document.page.insert_penalties == 0
    }
    /// Encode initial semantics, excluding input cursors and all job output.
    /// Loaded fonts, hyphenation data and live box registers currently return Unsupported.
    pub fn encode_format(&self) -> Result<Vec<u8>> {
        if !self.quiescent
            || !self.format_state_ready()
            || self
                .document
                .sources
                .iter()
                .any(|s| !matches!(s, Source::FileBoundary) && !self.source_exhausted(s))
        {
            return Err(Error::Checkpoint);
        }
        let values = values::Values::collect(self);
        let mut w = Writer::default();
        w.bytes(MAGIC);
        w.u32(VERSION);
        w.u32(PROFILE);
        w.u64(0);
        w.section(1, |w| write_names(self, w))?;
        w.section(2, |w| crate::fonts::format::write(self, w))?;
        w.section(3, |w| values.write(self, w))?;
        w.section(4, |w| tables::write_bindings(self, &values, w))?;
        w.section(5, |w| {
            crate::scalars::format::write(&self.document.scalars, w)
        })?;
        w.section(6, |w| tables::write_layout(self, w))?;
        w.section(7, |w| {
            crate::hyphenation::format::write(&self.document.hyphenation, w)
        })?;
        w.section(8, |w| crate::boxes::format::write(self, w))?;
        w.finish()
    }
    /// Load a self-contained semantic image with new unnamed document input.
    /// Image bytes need not outlive this call. The host may set_calendar before advance.
    pub fn from_format(image: &[u8], source: &'a [u8]) -> Result<Self> {
        Self::with_format_inputs(
            image,
            InputFile {
                name: b"",
                bytes: source,
            },
            None,
            &[],
        )
    }
    /// Load with immutable named input and upfront extra TFM resources.
    /// Equal-name/equal-byte TFMs deduplicate; conflicting bytes return Resource.
    pub fn with_format_inputs(
        image: &[u8],
        input: InputFile<'a>,
        resolver: Option<&'a dyn InputResolver>,
        extra_tfms: &[(&[u8], &[u8])],
    ) -> Result<Self> {
        let mut r = Reader::new(image)?;
        if r.bytes(8)? != MAGIC
            || r.u32()? != VERSION
            || r.u32()? != PROFILE
            || r.u64()? != image.len() as u64
        {
            return Err(Error::Format);
        }
        let mut e = Self::new(input.bytes);
        e.document.file.name = input.name;
        e.input_resolver = resolver;
        let mut section = r.section(1)?;
        read_names(&mut e, &mut section)?;
        section.finish()?;
        let mut section = r.section(2)?;
        let names = e.names.len();
        crate::fonts::format::read(&mut e, &mut section, names, extra_tfms)?;
        section.finish()?;
        let mut section = r.section(3)?;
        let values = values::read(&mut e, &mut section)?;
        section.finish()?;
        let mut section = r.section(4)?;
        tables::read_bindings(&mut e, &values, &mut section)?;
        section.finish()?;
        let mut section = r.section(5)?;
        e.document.scalars = crate::scalars::format::read(&mut section, e.document.fonts.len)?;
        section.finish()?;
        let mut section = r.section(6)?;
        tables::read_layout(&mut e, &mut section)?;
        section.finish()?;
        let mut section = r.section(7)?;
        e.document.hyphenation = crate::hyphenation::format::read(&mut section)?;
        section.finish()?;
        let mut section = r.section(8)?;
        crate::boxes::format::read(&mut e, &mut section)?;
        section.finish()?;
        r.finish()?;
        // Calendar and magnification preparation belong to this job, not setup.
        e.set_calendar(0, 1, 1, 1970);
        e.document.layout_values.prepared_mag = None;
        // Native start_input prepares the first physical line using the saved
        // endline policy before main_control starts the everyjob token source.
        e.document
            .file
            .load(e.document.scalars[crate::scalars::Target::ENDLINE].value)?;
        if let Some(owner) = e.document.token_registers[263].owner.clone() {
            e.push_tokens(owner, 0)?;
        }
        Ok(e)
    }
    pub(crate) fn finish_format_build(&mut self) -> Result<()> {
        if !self.format_state_ready() {
            return Err(Error::Checkpoint);
        }
        // Native final cleanup abandons all setup input after dump, including a
        // live invocation that supplied the dump command. Release packet owners
        // through the ordinary source-pop path before exporting persistent roots.
        while !self.document.sources.is_empty() {
            self.pop_source();
        }
        self.document.complete = true;
        Ok(())
    }
}
fn write_names(e: &Engine<'_>, w: &mut Writer) -> Result<()> {
    w.count(e.names.len() - NAMED_BASE as usize)?;
    for i in NAMED_BASE as usize..e.names.len() {
        w.blob(e.names.spelling(i))?;
    }
    Ok(())
}
fn read_names(e: &mut Engine<'_>, r: &mut Reader<'_>) -> Result<()> {
    let n = r.count(6)?;
    let names = n + NAMED_BASE as usize;
    if names < e.names.len() || names >= CS as usize {
        return Err(Error::Format);
    }
    for i in 0..n {
        let name = r.blob()?;
        if name.len() < 2 || e.intern(name)? as usize != NAMED_BASE as usize + i {
            return Err(Error::Format);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "format_tests.rs"]
mod tests;
