//! Alignment input lifetimes and native get_next/template boundary rules.
use crate::{Engine, Error, Result, scanner::Scanner, token::*, values::Owner};

pub(crate) struct Reader {
    frames: Vec<Frame>,
    pub state: i32,
}
struct Frame {
    saved_state: i32,
    backing: Option<Owner>,
    entry: Option<Entry>,
}
struct Entry {
    v: Option<Span>,
    terminator: Option<Terminator>,
}
/// Keep the native modifier; tab characters, span=256, cr=257, crcr=258.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Terminator(pub i32);
impl Default for Reader {
    fn default() -> Self {
        Self {
            frames: Vec::new(),
            state: 1000000,
        }
    }
}
impl Reader {
    pub fn active(&self) -> bool {
        !self.frames.is_empty()
    }
    pub fn abandon(&mut self) {
        self.frames.clear();
        self.state = 1000000;
    }
}
impl Engine<'_> {
    pub(crate) fn undo_alignment_brace(&mut self, token: Token) -> Result<()> {
        self.alignment_brace(token, -1)
    }
    fn alignment_brace(&mut self, token: Token, direction: i32) -> Result<()> {
        if self.alignment_reader.active() {
            let amount = if token.is_begin() {
                direction
            } else if token.is_end() {
                -direction
            } else {
                0
            };
            self.alignment_reader.state = self
                .alignment_reader
                .state
                .checked_add(amount)
                .ok_or(Error::Capacity)?;
        }
        Ok(())
    }
    /// Called exactly once after fetching/substituting a physical token.
    pub(crate) fn alignment_token(&mut self, raw: Raw) -> Result<bool> {
        if !self.alignment_reader.active() {
            return Ok(false);
        }
        self.alignment_brace(raw.token, 1)?;
        let meaning = self.raw_meaning(raw)?;
        if self.alignment_reader.state != 0 || !matches!(meaning.command, 4 | cmd::ALIGN_CR) {
            return Ok(false);
        }
        if self.scanner == Scanner::Aligning {
            return Err(Error::Syntax);
        }
        let depth = self.alignment_reader.frames.len() as u32;
        let entry = self
            .alignment_reader
            .frames
            .last_mut()
            .unwrap()
            .entry
            .as_mut()
            .ok_or(Error::Syntax)?;
        entry.terminator = Some(Terminator(meaning.value));
        let v = entry.v;
        if let Some(v) = v {
            self.push_alignment_span(v, SourceRole::AlignmentV(depth))?;
        } else {
            self.check_source_limit()?;
            self.push_inline([FROZEN_END_TEMPLATE, Token(0)], 1, false);
            let Source::Inline { role, .. } = self.document.sources.last_mut().unwrap() else {
                unreachable!()
            };
            *role = SourceRole::AlignmentV(depth);
        }
        self.alignment_reader.state = 1000000;
        Ok(true)
    }
    fn push_alignment_span(&mut self, span: Span, role: SourceRole) -> Result<()> {
        self.check_source_limit()?;
        let backing = self
            .alignment_reader
            .frames
            .last()
            .and_then(|f| f.backing.as_ref())
            .ok_or(Error::Syntax)?;
        if span.store != backing.id || span.start > span.end || span.end > backing.tokens.len {
            return Err(Error::Syntax);
        }
        if span.empty() {
            self.push_inline([Token(0); 2], 0, false);
            let Source::Inline {
                role: installed, ..
            } = self.document.sources.last_mut().unwrap()
            else {
                unreachable!()
            };
            *installed = role;
            return Ok(());
        }
        let (next, end, end_position) = backing.tokens.window(span.start, span.end);
        // The active reader frame owns backing until every cursor of its depth
        // has retired. A capture retains its own owner before the next read.
        self.document.sources.push(Source::Tokens(Cursor {
            role,
            next,
            end,
            end_position,
            store: span.store,
            _owner: None,
            argument: None,
            span_index: 0,
            span_end: span.end,
            owns_packet: None,
        }));
        Ok(())
    }
    pub(crate) fn next_expanded_or_protected(&mut self) -> Result<Read> {
        loop {
            let read = self.next_token()?;
            if !read.meaning.expandable() || read.meaning.is_protected_macro() {
                return Ok(read);
            }
            if read.meaning.is_macro() {
                self.begin_macro(read.meaning.value as u32)?;
            } else {
                self.expand(read)?;
            }
        }
    }
}

impl Engine<'_> {
    pub(crate) fn begin_alignment_reader(&mut self) -> Result<()> {
        if self.alignment_reader.frames.len() >= 10000 {
            return Err(Error::Capacity);
        }
        let saved_state = self.alignment_reader.state;
        self.alignment_reader.frames.push(Frame {
            saved_state,
            backing: None,
            entry: None,
        });
        self.alignment_reader.state = -1000000;
        Ok(())
    }
    pub(crate) fn install_alignment_templates(&mut self, backing: Owner) -> Result<()> {
        let frame = self
            .alignment_reader
            .frames
            .last_mut()
            .ok_or(Error::Syntax)?;
        if frame.backing.is_some() {
            return Err(Error::Syntax);
        }
        frame.backing = Some(backing);
        Ok(())
    }
    pub(crate) fn peek_alignment_entry(&mut self) -> Result<Read> {
        if !self.alignment_reader.active() {
            return Err(Error::Syntax);
        }
        self.alignment_reader.state = 1000000;
        loop {
            let read = self.next_expanded_or_protected()?;
            if read.meaning.command != cmd::SPACE {
                return Ok(read);
            }
        }
    }
    /// None means omit; otherwise back the already-read first cell token and
    /// install U even when its range is empty.
    pub(crate) fn begin_alignment_entry(
        &mut self,
        first: Read,
        templates: Option<(Span, Span)>,
    ) -> Result<()> {
        let frame = self
            .alignment_reader
            .frames
            .last_mut()
            .ok_or(Error::Syntax)?;
        if let Some((u, v)) = templates {
            let backing = frame.backing.as_ref().ok_or(Error::Syntax)?;
            for span in [u, v] {
                if span.store != backing.id
                    || span.start > span.end
                    || span.end > backing.tokens.len
                {
                    return Err(Error::Syntax);
                }
            }
            if v.empty() || self.values.token(v.store, v.end - 1) != FROZEN_END_TEMPLATE {
                return Err(Error::Syntax);
            }
        }
        frame.entry = Some(Entry {
            v: templates.map(|(_, v)| v),
            terminator: None,
        });
        if let Some((u, _)) = templates {
            self.back(first.token)?;
            let depth = self.alignment_reader.frames.len() as u32;
            self.push_alignment_span(u, SourceRole::AlignmentU(depth))?;
        } else {
            self.alignment_reader.state = 0;
        }
        Ok(())
    }
    /// Validate actual source provenance without discarding pending wrappers.
    /// Normal subsequent reads retire V; this does not turn it into a wall.
    pub(crate) fn alignment_endv(&self) -> Result<Terminator> {
        if self.alignment_reader.state < 500000 {
            return Err(Error::Syntax);
        }
        let depth = self.alignment_reader.frames.len() as u32;
        for source in self.document.sources.iter().rev() {
            if !self.source_exhausted(source) {
                return Err(Error::Syntax);
            }
            if source.role() == SourceRole::RetiredAlignmentV {
                return Err(Error::Syntax);
            }
            if let SourceRole::AlignmentV(d) = source.role() {
                if d != depth {
                    return Err(Error::Syntax);
                }
                return self
                    .alignment_reader
                    .frames
                    .last()
                    .and_then(|f| f.entry.as_ref())
                    .and_then(|e| e.terminator)
                    .ok_or(Error::Syntax);
            }
        }
        Err(Error::Syntax)
    }
    pub(crate) fn end_alignment_reader(&mut self) -> Result<()> {
        let depth = self.alignment_reader.frames.len() as u32;
        if depth == 0 {
            return Err(Error::Syntax);
        }
        for source in &self.document.sources {
            match source.role() {
                SourceRole::AlignmentU(d) if d == depth => return Err(Error::Syntax),
                SourceRole::AlignmentV(d) if d == depth && !self.source_exhausted(source) => {
                    return Err(Error::Syntax);
                }
                _ => {}
            }
        }
        for source in &mut self.document.sources {
            if source.role() == SourceRole::AlignmentV(depth) {
                // A backed aftergroup closer can leave an exhausted V below it.
                // Preserve its native eager-cleanup barrier, but remove borrowed
                // pointers before the backing owner is released.
                *source = Source::Inline {
                    role: SourceRole::RetiredAlignmentV,
                    tokens: [Token(0); 2],
                    position: 0,
                    len: 0,
                    suppress: false,
                };
            }
        }
        self.alignment_reader.state = self.alignment_reader.frames.pop().unwrap().saved_state;
        Ok(())
    }
}

#[cfg(test)]
#[path = "alignment_reader_tests.rs"]
mod tests;
