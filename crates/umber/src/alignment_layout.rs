//! Pure horizontal alignment layout; no token reading or template execution.
use crate::{
    Engine, Error, Result,
    nodes::{GlueKind, Kind, List, NodeId},
    numeric::{Glue, GlueOrder, Scaled},
    packing::{HorizontalTotals, PackSpec},
    value_metadata::Buffer,
};
#[path = "alignment_finalize.rs"]
mod finalize;

#[derive(Clone, Copy)]
struct Column {
    width: Option<i32>,
    after: Glue,
    spans: Option<usize>,
}
#[derive(Clone, Copy)]
struct Span {
    width: i32,
    extra: u8,
    next: Option<usize>,
}
#[derive(Clone, Copy)]
struct Cell {
    node: NodeId,
    extra: u8,
    stretch: i32,
    stretch_order: GlueOrder,
    shrink: i32,
    shrink_order: GlueOrder,
}
pub(crate) struct LayoutBuffers {
    columns: Buffer<Column>,
    spans: Buffer<Span>,
    cells: Buffer<Cell>,
}
impl Default for LayoutBuffers {
    fn default() -> Self {
        Self {
            columns: Buffer::new(&std::rc::Rc::default()),
            spans: Buffer::new(&std::rc::Rc::default()),
            cells: Buffer::new(&std::rc::Rc::default()),
        }
    }
}
impl LayoutBuffers {
    fn clear(&mut self) {
        self.columns.clear();
        self.spans.clear();
        self.cells.clear();
    }
    /// Transfer a sparse constraint into a sorted list, coalescing equal spans.
    fn merge_span(&mut self, column: usize, incoming: usize) {
        let mut previous = None;
        let mut at = self.columns[column].spans;
        self.merge_span_at(column, incoming, &mut previous, &mut at);
    }
    /// Incoming residual spans are sorted, so finalization keeps this cursor
    /// instead of revisiting the destination prefix for each incoming record.
    fn merge_span_at(
        &mut self,
        column: usize,
        incoming: usize,
        previous: &mut Option<usize>,
        at: &mut Option<usize>,
    ) {
        let extra = self.spans[incoming].extra;
        while let Some(index) = *at {
            if self.spans[index].extra >= extra {
                break;
            }
            *previous = *at;
            *at = self.spans[index].next;
        }
        if let Some(index) = *at
            && self.spans[index].extra == extra
        {
            self.spans[index].width = self.spans[index].width.max(self.spans[incoming].width);
            return;
        }
        self.spans[incoming].next = *at;
        if let Some(previous) = *previous {
            self.spans[previous].next = Some(incoming);
        } else {
            self.columns[column].spans = Some(incoming);
        }
        *previous = Some(incoming);
    }
    fn record_width(&mut self, start: usize, extra: u8, width: i32) {
        if extra == 0 {
            let old = self.columns[start].width;
            self.columns[start].width = Some(old.map_or(width, |old| old.max(width)));
        } else {
            let mut at = self.columns[start].spans;
            while let Some(index) = at {
                if self.spans[index].extra == extra {
                    self.spans[index].width = self.spans[index].width.max(width);
                    return;
                }
                if self.spans[index].extra > extra {
                    break;
                }
                at = self.spans[index].next;
            }
            let index = self.spans.len();
            self.spans.push(Span {
                width,
                extra,
                next: None,
            });
            self.merge_span(start, index);
        }
    }
}
pub(crate) struct LayoutFrame {
    buffers: LayoutBuffers,
    leading: Glue,
    body: List,
    row: List,
    adjustments: List,
    pending: List,
    column: usize,
    row_open: bool,
    body_out: bool,
}
fn dominant(amounts: [i32; 4]) -> (i32, GlueOrder) {
    let order = (0..4).rev().find(|&i| amounts[i] != 0).unwrap_or(0);
    (
        amounts[order],
        [
            GlueOrder::Normal,
            GlueOrder::Fil,
            GlueOrder::Fill,
            GlueOrder::Filll,
        ][order],
    )
}
impl Cell {
    fn new(node: NodeId, extra: u8, totals: HorizontalTotals) -> Self {
        let (stretch, stretch_order) = dominant(totals.stretch);
        let (shrink, shrink_order) = dominant(totals.shrink);
        Self {
            node,
            extra,
            stretch,
            stretch_order,
            shrink,
            shrink_order,
        }
    }
}
impl Engine<'_> {
    pub(crate) fn begin_alignment_layout(&mut self, tabskips: &[Glue]) -> Result<()> {
        if tabskips.len() < 2 {
            return Err(Error::Syntax);
        }
        let depth = self.document.alignment_layouts.len();
        if self.alignment_layout_pool.len() == depth {
            self.alignment_layout_pool.push(None);
        }
        let mut buffers = self.alignment_layout_pool[depth].take().unwrap_or_default();
        debug_assert!(
            buffers.columns.is_empty() && buffers.spans.is_empty() && buffers.cells.is_empty()
        );
        for &after in &tabskips[1..] {
            buffers.columns.push(Column {
                width: None,
                after,
                spans: None,
            });
        }
        self.document.alignment_layouts.push(LayoutFrame {
            buffers,
            leading: tabskips[0],
            body: List::default(),
            row: List::default(),
            adjustments: List::default(),
            pending: List::default(),
            column: 0,
            row_open: false,
            body_out: false,
        });
        Ok(())
    }
    pub(crate) fn begin_alignment_row(&mut self) -> Result<()> {
        let frame = self
            .document
            .alignment_layouts
            .last_mut()
            .ok_or(Error::Group)?;
        if frame.row_open || frame.body_out {
            return Err(Error::Group);
        }
        let node = self.document.nodes.alloc(Kind::Glue {
            spec: frame.leading,
            subtype: GlueKind::TabSkip,
        })?;
        self.document.nodes.append(&mut frame.row, node);
        frame.row_open = true;
        frame.column = 0;
        Ok(())
    }
    /// Consume a cell's completed list, retaining only operation-local totals.
    pub(crate) fn alignment_cell(&mut self, contents: List, columns: usize) -> Result<()> {
        let valid = self.document.alignment_layouts.last().is_some_and(|f| {
            f.row_open
                && (1..=256).contains(&columns)
                && columns <= f.buffers.columns.len() - f.column
        });
        if !valid {
            self.document
                .nodes
                .release(contents, &mut self.page_release);
            return Err(Error::Syntax);
        }
        let mut adjustments = std::mem::take(
            &mut self
                .document
                .alignment_layouts
                .last_mut()
                .unwrap()
                .adjustments,
        );
        let packed = self.hpack_adjusted_with_totals(contents, PackSpec::Natural, &mut adjustments);
        self.document
            .alignment_layouts
            .last_mut()
            .unwrap()
            .adjustments = adjustments;
        let (node, totals) = packed?;
        let node = self.document.nodes.mark_unset(node);
        let frame = self.document.alignment_layouts.last_mut().unwrap();
        let extra = (columns - 1) as u8;
        frame
            .buffers
            .cells
            .push(Cell::new(node.id(), extra, totals));
        frame
            .buffers
            .record_width(frame.column, extra, totals.width.0);
        frame.column += columns;
        self.document.nodes.append(&mut frame.row, node);
        let node = self.document.nodes.alloc(Kind::Glue {
            spec: frame.buffers.columns[frame.column - 1].after,
            subtype: GlueKind::TabSkip,
        })?;
        self.document.nodes.append(&mut frame.row, node);
        Ok(())
    }
    /// Append the packed unset row, baseline glue, then extracted adjustments.
    pub(crate) fn end_alignment_row(&mut self) -> Result<()> {
        let frame = self
            .document
            .alignment_layouts
            .last_mut()
            .ok_or(Error::Group)?;
        if !frame.row_open || frame.column == 0 {
            return Err(Error::Group);
        }
        let row = std::mem::take(&mut frame.row);
        let node = self.hpack_alignment_row(row)?;
        let node = self.document.nodes.mark_unset(node);
        let data = self.document.nodes.unset_mut(node.id());
        let (height, depth) = (data.height, data.depth);
        let baseline = crate::packing::baseline_glue(
            &self.document.layout_values,
            self.document.text.prev_depth,
            height,
        );
        let glue = baseline
            .map(|kind| self.document.nodes.alloc(kind))
            .transpose();
        let frame = self.document.alignment_layouts.last_mut().unwrap();
        let error = match glue {
            Ok(Some(glue)) => {
                self.document.nodes.append(&mut frame.body, glue);
                None
            }
            Ok(None) => None,
            Err(error) => Some(error),
        };
        self.document.nodes.append(&mut frame.body, node);
        self.document
            .nodes
            .splice(&mut frame.body, std::mem::take(&mut frame.adjustments));
        frame.row_open = false;
        if let Some(error) = error {
            return Err(error);
        }
        self.document.text.prev_depth = depth;
        Ok(())
    }
    /// Intervening already-constructed vertical material, such as noalign output.
    pub(crate) fn alignment_material(&mut self, material: List) -> Result<()> {
        if let Some(frame) = self.document.alignment_layouts.last_mut()
            && !frame.row_open
            && !frame.body_out
        {
            self.document.nodes.splice(&mut frame.body, material);
            return Ok(());
        }
        self.document
            .nodes
            .release(material, &mut self.page_release);
        Err(Error::Group)
    }
    /// Move the sole body owner into ordinary internal-vertical execution.
    pub(crate) fn take_alignment_body(&mut self) -> Result<List> {
        let frame = self
            .document
            .alignment_layouts
            .last_mut()
            .ok_or(Error::Group)?;
        if frame.row_open || frame.body_out {
            return Err(Error::Group);
        }
        frame.body_out = true;
        Ok(std::mem::take(&mut frame.body))
    }
    pub(crate) fn put_alignment_body(&mut self, body: List) -> Result<()> {
        if let Some(frame) = self.document.alignment_layouts.last_mut()
            && frame.body_out
        {
            debug_assert!(frame.body.is_empty());
            frame.body = body;
            frame.body_out = false;
            return Ok(());
        }
        self.document.nodes.release(body, &mut self.page_release);
        Err(Error::Group)
    }
    fn recycle_layout(&mut self, mut frame: LayoutFrame) {
        frame.buffers.clear();
        let depth = self.document.alignment_layouts.len();
        debug_assert!(self.alignment_layout_pool[depth].is_none());
        self.alignment_layout_pool[depth] = Some(frame.buffers);
    }
    pub(crate) fn abort_alignment_layout(&mut self) -> Result<()> {
        if self
            .document
            .alignment_layouts
            .last()
            .is_none_or(|frame| frame.body_out)
        {
            return Err(Error::Group);
        }
        let mut frame = self.document.alignment_layouts.pop().unwrap();
        for list in [
            &mut frame.body,
            &mut frame.row,
            &mut frame.adjustments,
            &mut frame.pending,
        ] {
            self.document
                .nodes
                .release(std::mem::take(list), &mut self.page_release);
        }
        self.recycle_layout(frame);
        Ok(())
    }
    /// Whole-root restore abandons IDs before replacing node storage.
    pub(crate) fn abandon_alignment_layouts(&mut self) {
        while let Some(mut frame) = self.document.alignment_layouts.pop() {
            frame.body = List::default();
            frame.row = List::default();
            frame.adjustments = List::default();
            frame.pending = List::default();
            self.recycle_layout(frame);
        }
    }
}

#[cfg(test)]
#[path = "alignment_layout_tests.rs"]
mod tests;
