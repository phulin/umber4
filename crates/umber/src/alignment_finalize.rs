//! Native left-to-right span resolution and in-place unset finalization.
use super::*;
use crate::{
    nodes::{BoxData, GlueSet, GlueSign},
    packing::glue_setting,
    page_output::RUNNING,
};
#[derive(Clone, Copy)]
struct Prototype {
    width: Scaled,
    glue: GlueSet,
}
impl LayoutBuffers {
    fn resolve_columns(&mut self) {
        for i in 0..self.columns.len() {
            if self.columns[i].width.is_none() {
                self.columns[i].width = Some(0);
                self.columns[i].after = Glue::default();
            }
            let consumed = self.columns[i]
                .width
                .unwrap()
                .wrapping_add(self.columns[i].after.width.0);
            let mut at = self.columns[i].spans.take();
            let mut previous = None;
            let mut destination = self.columns.get(i + 1).and_then(|column| column.spans);
            while let Some(index) = at {
                at = self.spans[index].next;
                self.spans[index].width = self.spans[index].width.wrapping_sub(consumed);
                if self.spans[index].extra == 1 {
                    let width = self.spans[index].width;
                    let old = self.columns[i + 1].width;
                    self.columns[i + 1].width = Some(old.map_or(width, |old| old.max(width)));
                } else {
                    self.spans[index].extra -= 1;
                    self.merge_span_at(i + 1, index, &mut previous, &mut destination);
                }
            }
        }
    }
    fn prototype(&self, leading: Glue, spec: PackSpec) -> Prototype {
        let mut totals = HorizontalTotals::default();
        let add = |totals: &mut HorizontalTotals, glue: Glue| {
            totals.width.0 = totals.width.0.wrapping_add(glue.width.0);
            let order = glue.stretch_order as usize;
            totals.stretch[order] = totals.stretch[order].wrapping_add(glue.stretch.0);
            let order = glue.shrink_order as usize;
            totals.shrink[order] = totals.shrink[order].wrapping_add(glue.shrink.0);
        };
        add(&mut totals, leading);
        for column in &self.columns {
            totals.width.0 = totals.width.0.wrapping_add(column.width.unwrap());
            add(&mut totals, column.after);
        }
        let width = spec.target(totals.width.0);
        Prototype {
            width,
            glue: glue_setting(
                width.0.wrapping_sub(totals.width.0),
                totals.stretch,
                totals.shrink,
                true,
            ),
        }
    }
}
fn cell_glue(cell: Cell, excess: i32) -> GlueSet {
    if excess == 0 {
        return GlueSet::default();
    }
    if excess > 0 {
        GlueSet {
            sign: GlueSign::Stretching,
            order: cell.stretch_order,
            ratio: if cell.stretch == 0 {
                0.0
            } else {
                f64::from(excess) / f64::from(cell.stretch)
            },
        }
    } else {
        GlueSet {
            sign: GlueSign::Shrinking,
            order: cell.shrink_order,
            ratio: if cell.shrink == 0 {
                0.0
            } else if cell.shrink_order == GlueOrder::Normal && excess.wrapping_neg() > cell.shrink
            {
                1.0
            } else {
                -f64::from(excess) / f64::from(cell.shrink)
            },
        }
    }
}
fn individual_tab(glue: Glue, setting: GlueSet) -> i32 {
    let mut width = glue.width.0;
    if setting.sign == GlueSign::Stretching && setting.order == glue.stretch_order {
        width = width.wrapping_add((setting.ratio * f64::from(glue.stretch.0)).round() as i32);
    } else if setting.sign == GlueSign::Shrinking && setting.order == glue.shrink_order {
        width = width.wrapping_sub((setting.ratio * f64::from(glue.shrink.0)).round() as i32);
    }
    width
}
impl Engine<'_> {
    fn finish_alignment_row(
        &mut self,
        row: NodeId,
        prototype: Prototype,
        cell_index: &mut usize,
    ) -> Result<()> {
        let data = self.document.nodes.unset_mut(row);
        let (height, depth, first) = (data.height, data.depth, data.list.head());
        data.width = prototype.width;
        data.glue_set = prototype.glue;
        data.shift = Scaled::ZERO;
        self.document.nodes.finish_unset(row);
        let mut node = self.document.nodes.get(first).next;
        let mut column = 0;
        while !node.is_null() {
            let cell = *self
                .document
                .alignment_layouts
                .last()
                .unwrap()
                .buffers
                .cells
                .get(*cell_index)
                .ok_or(Error::Syntax)?;
            if cell.node != node {
                return Err(Error::Syntax);
            }
            let tab = self.document.nodes.get(node).next;
            let width = self
                .document
                .alignment_layouts
                .last()
                .unwrap()
                .buffers
                .columns[column]
                .width
                .unwrap();
            let mut target = width;
            for _ in 0..cell.extra {
                let frame = self.document.alignment_layouts.last().unwrap();
                let skip = frame.buffers.columns[column].after;
                column += 1;
                let next_width = frame.buffers.columns[column].width.unwrap();
                target = target
                    .wrapping_add(individual_tab(skip, prototype.glue))
                    .wrapping_add(next_width);
                let glue = self.document.nodes.alloc(Kind::Glue {
                    spec: skip,
                    subtype: GlueKind::TabSkip,
                })?;
                self.document.nodes.append(
                    &mut self.document.alignment_layouts.last_mut().unwrap().pending,
                    glue,
                );
                let empty = self.document.nodes.alloc_box(BoxData {
                    width: Scaled(next_width),
                    ..BoxData::default()
                })?;
                self.document.nodes.append(
                    &mut self.document.alignment_layouts.last_mut().unwrap().pending,
                    empty,
                );
            }
            let data = self.document.nodes.unset_mut(node);
            data.glue_set = cell_glue(cell, target.wrapping_sub(data.width.0));
            data.width = Scaled(width);
            data.height = height;
            data.depth = depth;
            data.shift = Scaled::ZERO;
            self.document.nodes.finish_unset(node);
            let pending =
                std::mem::take(&mut self.document.alignment_layouts.last_mut().unwrap().pending);
            let mut list = std::mem::take(&mut self.document.nodes.box_mut(row).list);
            self.document.nodes.insert_after(&mut list, node, pending);
            self.document.nodes.box_mut(row).list = list;
            column += 1;
            *cell_index += 1;
            if tab.is_null() {
                return Err(Error::Syntax);
            }
            node = self.document.nodes.get(tab).next;
        }
        Ok(())
    }
    pub(crate) fn finish_alignment_layout(&mut self, spec: PackSpec) -> Result<List> {
        let frame = self
            .document
            .alignment_layouts
            .last_mut()
            .ok_or(Error::Group)?;
        if frame.row_open || frame.body_out {
            return Err(Error::Group);
        }
        frame.buffers.resolve_columns();
        let prototype = frame.buffers.prototype(frame.leading, spec);
        let mut node = frame.body.head();
        let mut cells = 0;
        while !node.is_null() {
            if matches!(self.document.nodes.get(node).kind, Kind::Unset(_)) {
                self.finish_alignment_row(node, prototype, &mut cells)?;
            } else if let Kind::Rule {
                width,
                height,
                depth,
            } = self.document.nodes.kind_mut(node)
            {
                if width.0 == RUNNING {
                    *width = prototype.width;
                }
                if height.0 == RUNNING {
                    *height = Scaled::ZERO;
                }
                if depth.0 == RUNNING {
                    *depth = Scaled::ZERO;
                }
            }
            node = self.document.nodes.get(node).next;
        }
        if cells
            != self
                .document
                .alignment_layouts
                .last()
                .unwrap()
                .buffers
                .cells
                .len()
        {
            return Err(Error::Syntax);
        }
        let mut frame = self.document.alignment_layouts.pop().unwrap();
        let body = std::mem::take(&mut frame.body);
        self.recycle_layout(frame);
        Ok(body)
    }
}
