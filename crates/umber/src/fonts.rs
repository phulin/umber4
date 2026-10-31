//! Restorable font identities and mutable parameters, separate from the cache.
use crate::{
    Engine, Error, Result,
    numeric::Scaled,
    scalars::Target,
    tables::Table,
    tfm::{Metrics, ratio},
    token::*,
};
#[derive(Clone, Copy, Default)]
pub(crate) struct Font {
    pub scaled: Option<usize>,
    pub params: usize,
    pub count: usize,
    pub hyphen: i32,
    pub skew: i32,
    pub identifier: usize,
}
#[derive(Clone)]
pub(crate) struct Fonts {
    pub descriptors: Table<Font>,
    pub parameters: Table<Scaled>,
    pub len: usize,
    pub parameter_end: usize,
    pub current: Integer,
}
impl Default for Fonts {
    fn default() -> Self {
        let mut descriptors = Table::new(1);
        descriptors[0] = Font {
            count: 7,
            hyphen: 45,
            skew: -1,
            ..Font::default()
        };
        Self {
            descriptors,
            parameters: Table::new(7),
            len: 1,
            parameter_end: 7,
            current: Integer::default(),
        }
    }
}
impl Engine<'_> {
    /// Register an immutable metric resource before the first nonzero advance.
    /// Names are exact bytes, with an optional terminal `.tfm`; no file is opened.
    /// Registration remains sealed after restoring an earlier checkpoint.
    pub fn register_tfm(&mut self, name: &[u8], bytes: &[u8]) -> Result<()> {
        self.font_catalog.register(name, bytes)
    }
    pub(crate) fn character_metrics(&self, font: u32, character: u8) -> Option<Metrics> {
        if font as usize >= self.document.fonts.len {
            return None;
        }
        self.document.fonts.descriptors[font as usize]
            .scaled
            .and_then(|s| self.font_catalog.metrics(s, character))
    }
    pub(crate) fn font_parameter(&self, font: usize, n: usize) -> Scaled {
        let f = self.document.fonts.descriptors[font];
        if n == 0 || n > f.count {
            Scaled::ZERO
        } else {
            self.document.fonts.parameters[f.params + n - 1]
        }
    }
    pub(crate) fn scan_font_ident(&mut self) -> Result<usize> {
        let read = loop {
            let r = self.nonspace(true)?;
            if r.meaning.command != cmd::RELAX {
                break r;
            }
        };
        match read.meaning.command {
            cmd::FONT => Ok(self.document.fonts.current.value as usize),
            cmd::SELECT_FONT => Ok(read.meaning.value as usize),
            _ => Err(Error::Syntax),
        }
    }
    pub(crate) fn find_font_dimen(&mut self) -> Result<usize> {
        let n = self.scan_int()?;
        let f = self.scan_font_ident()?;
        if n <= 0 {
            return Err(Error::Number);
        }
        let n = n as usize;
        let font = self.document.fonts.descriptors[f];
        if n > font.count {
            if f + 1 != self.document.fonts.len {
                return Err(Error::Number);
            }
            let extra = n - font.count;
            let end = self
                .document
                .fonts
                .parameter_end
                .checked_add(extra)
                .ok_or(Error::Capacity)?;
            // Extension owns the last document span. Clear abandoned branch data
            // even if retained COW backing still has slots above the live end.
            for i in self.document.fonts.parameter_end..end {
                self.document.fonts.parameters[i] = Scaled::ZERO;
            }
            self.document.fonts.parameter_end = end;
            self.document.fonts.descriptors[f].count = n;
        }
        Ok(font.params + n - 1)
    }
    pub(crate) fn font_dimension(&mut self, read: Read) -> Result<Scaled> {
        if read.meaning.command == cmd::FONT_DIMEN {
            let i = self.find_font_dimen()?;
            return Ok(self.document.fonts.parameters[i]);
        }
        let f = self.scan_font_ident()?;
        let c = self.scan_index()? as u8;
        let m = self.document.fonts.descriptors[f]
            .scaled
            .and_then(|s| self.font_catalog.metric_fields(s, c))
            .unwrap_or_default();
        Ok(match read.meaning.value {
            0 => m.width,
            1 => m.height,
            2 => m.depth,
            _ => m.italic,
        })
    }
    pub(crate) fn font_integer(&mut self, read: Read) -> Result<i32> {
        match read.meaning.command {
            cmd::SPACE_FACTOR => {
                if self.document.text.mode != crate::text::Mode::Horizontal {
                    return Err(Error::Syntax);
                }
                Ok(self.document.text.space_factor)
            }
            cmd::FONT_INT => {
                let f = self.scan_font_ident()?;
                let f = self.document.fonts.descriptors[f];
                Ok(if read.meaning.value == 0 {
                    f.hyphen
                } else {
                    f.skew
                })
            }
            _ => Err(Error::Number),
        }
    }
    pub(crate) fn font_assignment(&mut self, read: Read, global: bool) -> Result<()> {
        match read.meaning.command {
            cmd::FONT => self.define_font(global),
            cmd::SELECT_FONT => {
                let old = self.document.fonts.current;
                if !global && old.value == read.meaning.value {
                    return Ok(());
                }
                if !global && self.document.level > 0 && old.level != self.document.level {
                    self.document.save.push(crate::execute::Save::Font(old));
                }
                self.document.fonts.current = Integer {
                    value: read.meaning.value,
                    level: if global { 0 } else { self.document.level },
                };
                Ok(())
            }
            cmd::FONT_DIMEN => {
                let index = self.find_font_dimen()?;
                self.scan_equals()?;
                let value = self.scan_dimen()?;
                self.document.fonts.parameters[index] = value;
                Ok(())
            }
            cmd::FONT_INT => {
                let f = self.scan_font_ident()?;
                self.scan_equals()?;
                let value = self.scan_int()?;
                let f = &mut self.document.fonts.descriptors[f];
                if read.meaning.value == 0 {
                    f.hyphen = value;
                } else {
                    f.skew = value;
                }
                Ok(())
            }
            cmd::SPACE_FACTOR => {
                if self.document.text.mode != crate::text::Mode::Horizontal {
                    return Err(Error::Syntax);
                }
                self.scan_equals()?;
                let value = self.scan_int()?;
                if !(1..=32767).contains(&value) {
                    return Err(Error::Number);
                }
                self.document.text.space_factor = value;
                Ok(())
            }
            _ => Err(Error::Unsupported),
        }
    }
    fn define_font(&mut self, global: bool) -> Result<()> {
        let id = self.scan_cs()?;
        self.assign_meaning(
            id,
            Meaning {
                command: cmd::SELECT_FONT,
                value: 0,
            },
            global,
        );
        self.scan_equals()?;
        self.font_name.clear();
        let mut read = self.nonspace(true)?;
        loop {
            if read.token.is_cs() || !matches!(read.meaning.command, 11 | 12) {
                if read.meaning.command != cmd::SPACE {
                    self.back(read.token);
                }
                break;
            }
            self.font_name.push(read.token.byte());
            read = self.next_expanded()?;
        }
        if self.font_name.is_empty() {
            return Err(Error::Syntax);
        }
        let resource = self
            .font_catalog
            .find(&self.font_name)
            .ok_or(Error::Resource)?;
        let design = self.font_catalog.resources[resource].design;
        let size = if self.scan_keyword(b"at")? {
            let s = self.scan_dimen()?;
            if s.0 <= 0 || s.0 >= 134_217_728 {
                return Err(Error::Number);
            }
            s
        } else if self.scan_keyword(b"scaled")? {
            let n = self.scan_int()?;
            if !(1..=32768).contains(&n) {
                return Err(Error::Number);
            }
            Scaled(ratio(design.0, n, 1000)?)
        } else {
            design
        };
        let mut found = None;
        for i in 1..self.document.fonts.len {
            let scaled = self.document.fonts.descriptors[i].scaled.unwrap();
            let cached = self.font_catalog.scaled[scaled].descriptor();
            if cached.resource == resource && cached.size == size {
                found = Some(i);
                break;
            }
        }
        let f = if let Some(f) = found {
            f
        } else {
            let scaled = self.font_catalog.scaled(resource, size)?;
            let start = self.font_catalog.param_start(scaled);
            let count = self.font_catalog.resources[resource].np.max(7);
            let params = self.document.fonts.parameter_end;
            for i in 0..count {
                self.document.fonts.parameters[params + i] =
                    self.font_catalog.scaled[scaled].descriptor().numbers[start + i];
            }
            let f = self.document.fonts.len;
            self.document.fonts.descriptors[f] = Font {
                scaled: Some(scaled),
                params,
                count,
                hyphen: self.document.scalars[Target::DEFAULT_HYPHEN].value,
                skew: self.document.scalars[Target::DEFAULT_SKEW].value,
                identifier: id,
            };
            self.document.fonts.len += 1;
            self.document.fonts.parameter_end += count;
            f
        };
        // The last alias becomes the font identifier of record, independently
        // of the scoped selector meaning. A future \the\font uses that record.
        self.document.fonts.descriptors[f].identifier = id;
        self.document.bindings[id] = Binding::new(
            Meaning {
                command: cmd::SELECT_FONT,
                value: f as i32,
            },
            if global { 0 } else { self.document.level },
        );
        Ok(())
    }
}
