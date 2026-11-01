//! Immutable host input and file boundaries in the ordinary token-source stack.
use crate::{Engine, Error, Result, input::FileInput, token::*, value_metadata::Buffer};

/// Stable bytes supplied by an immutable host bundle, including its resolved name.
#[derive(Clone, Copy, Debug)]
pub struct InputFile<'a> {
    pub name: &'a [u8],
    pub bytes: &'a [u8],
}

/// Resolve exact TeX filename bytes. Search paths and default extensions belong
/// to the host. Returned names and contents must remain unchanged for the session.
pub trait InputResolver {
    fn resolve(&self, requested: &[u8], including: &[u8]) -> Option<InputFile<'_>>;
}

impl<'a> Engine<'a> {
    pub fn with_inputs(name: &'a [u8], source: &'a [u8], resolver: &'a dyn InputResolver) -> Self {
        let mut engine = Self::new(source);
        engine.document.file.name = name;
        engine.input_resolver = Some(resolver);
        engine
    }

    #[inline]
    pub(crate) fn at_file_input(&self) -> bool {
        self.document.sources.is_empty()
            || matches!(self.document.sources.last(), Some(Source::FileBoundary))
    }

    pub(crate) fn input_command(&mut self, read: Read) -> Result<()> {
        if read.meaning.value == 1 {
            self.document.force_eof = true;
            return Ok(());
        }
        if self.name_in_progress {
            self.check_source_limit()?;
            self.insert_relax(read.token)?;
            return Ok(());
        }
        let mut name = self
            .name_spills
            .pop()
            .unwrap_or_else(|| self.values.name_buffer());
        let result = (|| {
            self.scan_input_name(&mut name)?;
            let resolver = self.input_resolver.ok_or(Error::Resource)?;
            let file = resolver
                .resolve(&name, self.document.file.name)
                .ok_or(Error::Resource)?;
            self.check_source_limit()?;
            let mut child = FileInput::new_in(file, &self.file_lines);
            // start_input prepares the first line even when force_eof is set.
            child.load(self.document.scalars[crate::scalars::Target::ENDLINE].value)?;
            let parent = std::mem::replace(&mut self.document.file, child);
            self.document.suspended_files.push(parent);
            self.document.sources.push(Source::FileBoundary);
            Ok(())
        })();
        name.clear();
        self.name_spills.push(name);
        result
    }

    fn scan_input_name(&mut self, name: &mut Buffer<u8>) -> Result<()> {
        let first = loop {
            let read = self.nonspace(true)?;
            if read.meaning.command != cmd::RELAX {
                break read;
            }
        };
        if first.meaning.command == cmd::BEGIN {
            // pdfTeX stringifies an expanded balanced list, allowing spaces.
            self.scan_body::<true>(None, &mut |engine: &mut Self, token: Token| {
                engine.token_text(token, true, |_, byte| {
                    if byte != b'"' {
                        name.push(byte);
                    }
                    Ok(())
                })
            })?;
            return Ok(());
        }
        self.name_in_progress = true;
        let result = (|| {
            let mut read = first;
            let mut quoted = false;
            loop {
                if read.meaning.command > 12 || !(0..=255).contains(&read.meaning.value) {
                    self.back(read.token)?;
                    return Ok(());
                }
                let byte = read.meaning.value as u8;
                if byte == b' '
                    && (!quoted || (self.at_file_input() && self.document.file.line_ended()))
                {
                    return Ok(());
                }
                if byte == b'"' {
                    quoted = !quoted;
                } else {
                    name.push(byte);
                }
                read = self.next_expanded()?;
            }
        })();
        self.name_in_progress = false;
        result
    }
}

#[cfg(test)]
#[path = "input_files_tests.rs"]
mod tests;
