//! Ordinary horizontal alignments: real body and cell execution states.
use crate::{
    Engine, Error, Result,
    execute::GroupKind,
    layout_values::GlueParam,
    numeric::Glue,
    packing::PackSpec,
    scanner::Scanner,
    text::{Mode, TextState},
    token::*,
    value_metadata::Buffer,
    values::{Builder, Class},
};

#[derive(Clone, Copy)]
pub(crate) struct ColumnTemplate {
    u: Span,
    v: Span,
    after: Glue,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Preamble,
    BetweenRows,
    Cell,
}
pub(crate) struct Frame {
    outer: TextState,
    body: Option<TextState>,
    columns: Buffer<ColumnTemplate>,
    preamble: Option<Builder>,
    phase: Phase,
    spec: PackSpec,
    leading: Glue,
    period: Option<(usize, usize)>,
    current: usize,
    span_start: usize,
    outer_level: u16,
}
impl Engine<'_> {
    pub(crate) fn alignment_command(&mut self, read: Read) -> Result<bool> {
        match read.meaning.command {
            cmd::HALIGN => match self.document.text.mode {
                Mode::Horizontal => self.head_for_vertical(read)?,
                Mode::Vertical | Mode::InternalVertical => self.begin_halign()?,
                Mode::DisplayMath => return Err(Error::Unsupported),
                _ => return Err(Error::Syntax),
            },
            cmd::ENDV if self.document.text.mode.is_math() => return Err(Error::Syntax),
            cmd::ENDV => self.finish_alignment_cell()?,
            cmd::END_GROUP_CHAR if self.current_group() == Some(GroupKind::Align) => {
                self.back(read.token)?;
                return Err(Error::Syntax);
            }
            cmd::OMIT | cmd::NOALIGN | cmd::ALIGN_CR | 4 => return Err(Error::Syntax),
            _ => return Ok(false),
        }
        Ok(true)
    }
    fn begin_halign(&mut self) -> Result<()> {
        let depth = self.document.alignment_frames.len();
        if self.alignment_execution_pool.len() == depth {
            self.alignment_execution_pool.push(None);
        }
        let columns = self.alignment_execution_pool[depth]
            .take()
            .unwrap_or_else(|| Buffer::new(&std::rc::Rc::default()));
        let prev_depth = self.document.text.prev_depth;
        let outer = std::mem::replace(
            &mut self.document.text,
            TextState {
                mode: Mode::InternalVertical,
                vertical_mode: Mode::InternalVertical,
                prev_depth,
                ..TextState::default()
            },
        );
        self.document.alignment_frames.push(Frame {
            outer,
            body: None,
            columns,
            preamble: None,
            phase: Phase::Preamble,
            spec: PackSpec::Natural,
            leading: Glue::default(),
            period: None,
            current: 0,
            span_start: 0,
            outer_level: 0,
        });
        self.begin_alignment_reader()?;
        self.document.alignment_frames.last_mut().unwrap().preamble =
            Some(self.values.builder(Class::Durable)?);
        let spec = if self.scan_keyword(b"to")? {
            PackSpec::To(self.scan_dimen()?)
        } else if self.scan_keyword(b"spread")? {
            PackSpec::Spread(self.scan_dimen()?)
        } else {
            PackSpec::Natural
        };
        self.document.alignment_frames.last_mut().unwrap().spec = spec;
        self.open_group(GroupKind::Align)?;
        self.document
            .alignment_frames
            .last_mut()
            .unwrap()
            .outer_level = self.document.level;
        self.opening_brace()?;
        self.alignment_reader.state = -1000000;
        self.with_scanner(Scanner::Aligning, |e| e.scan_alignment_preamble())?;
        let owner = self
            .document
            .alignment_frames
            .last_mut()
            .unwrap()
            .preamble
            .take()
            .unwrap()
            .finish();
        self.install_alignment_templates(owner)?;
        let frame = self.document.alignment_frames.last().unwrap();
        let leading = frame.leading;
        let first = frame.columns[0].after;
        let count = frame.columns.len();
        self.begin_alignment_layout(&[leading, first])?;
        for i in 1..count {
            let after = self.document.alignment_frames.last().unwrap().columns[i].after;
            self.extend_alignment_columns(&[after])?;
        }
        self.open_group(GroupKind::Align)?;
        self.document.alignment_frames.last_mut().unwrap().phase = Phase::BetweenRows;
        self.document.text.vertical = self.take_alignment_body()?;
        self.alignment_everycr()?;
        self.alignment_peek()
    }
    fn alignment_everycr(&mut self) -> Result<()> {
        if let Some(owner) = self.document.token_registers[262].owner.clone() {
            self.push_tokens(owner, 0)?;
        }
        Ok(())
    }
    fn alignment_peek(&mut self) -> Result<()> {
        loop {
            let read = self.peek_alignment_entry()?;
            match read.meaning.command {
                cmd::NOALIGN => {
                    self.opening_brace()?;
                    self.open_group(GroupKind::NoAlign)?;
                    self.normal_paragraph()?;
                    return Ok(());
                }
                cmd::END_GROUP_CHAR => return self.finish_halign(),
                cmd::ALIGN_CR if read.meaning.value == 258 => continue,
                _ => return self.start_alignment_row(read),
            }
        }
    }
    fn start_alignment_row(&mut self, first: Read) -> Result<()> {
        let body = std::mem::take(&mut self.document.text.vertical);
        self.put_alignment_body(body)?;
        self.begin_alignment_row()?;
        let cell = TextState {
            mode: Mode::RestrictedHorizontal,
            vertical_mode: Mode::RestrictedHorizontal,
            ..TextState::default()
        };
        let body = std::mem::replace(&mut self.document.text, cell);
        let frame = self.document.alignment_frames.last_mut().unwrap();
        frame.body = Some(body);
        frame.current = 0;
        frame.span_start = 0;
        frame.phase = Phase::Cell;
        self.start_alignment_template(first)
    }
    fn start_alignment_template(&mut self, first: Read) -> Result<()> {
        let frame = self.document.alignment_frames.last().unwrap();
        let column = frame.columns[frame.current];
        self.begin_alignment_entry(
            first,
            if first.meaning.command == cmd::OMIT {
                None
            } else {
                Some((column.u, column.v))
            },
        )
    }
    fn extend_alignment_preamble(&mut self) -> Result<()> {
        let frame = self.document.alignment_frames.last_mut().unwrap();
        if frame.current + 1 < frame.columns.len() {
            return Ok(());
        }
        let (start, end) = frame.period.ok_or(Error::Syntax)?;
        let from = start + (frame.columns.len() - end) % (end - start);
        let column = frame.columns[from];
        frame.columns.push(column);
        self.extend_alignment_columns(&[column.after])
    }
    fn finish_alignment_cell(&mut self) -> Result<()> {
        let end = self.alignment_endv()?.0;
        let frame = self.document.alignment_frames.last().ok_or(Error::Group)?;
        if frame.phase != Phase::Cell
            || self.current_group() != Some(GroupKind::Align)
            || self.document.level != frame.outer_level + 1
        {
            return Err(Error::Group);
        }
        if end < 257 {
            self.extend_alignment_preamble()?;
        }
        if end == 256 {
            self.document.alignment_frames.last_mut().unwrap().current += 1;
            let first = self.peek_alignment_entry()?;
            return self.start_alignment_template(first);
        }
        self.close_group(GroupKind::Align)?;
        self.open_group(GroupKind::Align)?;
        let frame = self.document.alignment_frames.last().unwrap();
        let columns = frame.current - frame.span_start + 1;
        let contents = std::mem::take(&mut self.document.text.horizontal);
        self.alignment_cell(contents, columns)?;
        if end >= 257 {
            return self.finish_execution_row();
        }
        let frame = self.document.alignment_frames.last_mut().unwrap();
        frame.current += 1;
        frame.span_start = frame.current;
        self.document.text = TextState {
            mode: Mode::RestrictedHorizontal,
            vertical_mode: Mode::RestrictedHorizontal,
            ..TextState::default()
        };
        let first = self.peek_alignment_entry()?;
        self.start_alignment_template(first)
    }
    fn finish_execution_row(&mut self) -> Result<()> {
        self.document.text = self
            .document
            .alignment_frames
            .last_mut()
            .unwrap()
            .body
            .take()
            .unwrap();
        self.end_alignment_row()?;
        self.document.text.vertical = self.take_alignment_body()?;
        self.document.alignment_frames.last_mut().unwrap().phase = Phase::BetweenRows;
        self.alignment_everycr()?;
        self.alignment_peek()
    }
    pub(crate) fn finish_noalign(&mut self) -> Result<()> {
        if self
            .document
            .alignment_frames
            .last()
            .is_none_or(|f| f.phase != Phase::BetweenRows)
        {
            return Err(Error::Group);
        }
        self.end_graf()?;
        self.close_group(GroupKind::NoAlign)?;
        self.alignment_peek()
    }
    fn finish_halign(&mut self) -> Result<()> {
        let frame = self.document.alignment_frames.last().ok_or(Error::Group)?;
        if frame.phase != Phase::BetweenRows
            || self.current_group() != Some(GroupKind::Align)
            || self.document.level != frame.outer_level + 1
        {
            return Err(Error::Group);
        }
        let body = std::mem::take(&mut self.document.text.vertical);
        self.put_alignment_body(body)?;
        self.close_group(GroupKind::Align)?;
        self.close_group(GroupKind::Align)?;
        let spec = self.document.alignment_frames.last().unwrap().spec;
        self.document.text.vertical = self.finish_alignment_layout(spec)?;
        self.end_alignment_reader()?;
        let frame = self.document.alignment_frames.pop().unwrap();
        let depth = self.document.text.prev_depth;
        let body = std::mem::take(&mut self.document.text.vertical);
        self.document.text = frame.outer;
        self.document.text.prev_depth = depth;
        self.document
            .nodes
            .splice(&mut self.document.text.vertical, body);
        self.recycle_alignment_execution(frame.columns);
        if self.document.text.mode == Mode::Vertical {
            self.build_page()?;
        }
        Ok(())
    }
    fn recycle_alignment_execution(&mut self, mut columns: Buffer<ColumnTemplate>) {
        columns.clear();
        let depth = self.document.alignment_frames.len();
        debug_assert!(self.alignment_execution_pool[depth].is_none());
        self.alignment_execution_pool[depth] = Some(columns);
    }
    pub(crate) fn abandon_alignment_execution(&mut self) {
        while let Some(frame) = self.document.alignment_frames.pop() {
            self.recycle_alignment_execution(frame.columns);
        }
    }
}
#[path = "alignment_preamble.rs"]
mod preamble;
#[cfg(test)]
#[path = "alignment_execute_tests.rs"]
mod tests;
