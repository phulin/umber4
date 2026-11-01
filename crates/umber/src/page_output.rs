//! Synchronous lowering over live lists; all retained output is detached.
use crate::{
    Engine, Error, Result,
    layout_values::DimenParam,
    nodes::{BoxData, DetachedNode, GlueSet, GlueSign, Kind, NodeId},
    numeric::Glue,
    output::{GlueSetting, PackedBox, Page, Runs},
    value_metadata::Buffer,
};
#[path = "page_leaders.rs"]
mod leaders;
use leaders::Repeat;
#[derive(Clone, Copy)]
enum Work {
    List(Frame),
    Repeat(Repeat),
}
#[derive(Clone, Copy)]
struct Frame {
    next: NodeId,
    box_index: u32,
    vertical: bool,
    x: i32,
    y: i32,
    left: i32,
    baseline: i32,
    width: i32,
    height: i32,
    depth: i32,
    glue: GlueSet,
    seen: f64,
    adjustment: i32,
}
impl Frame {
    fn new(b: &BoxData, index: u32, x: i32, y: i32) -> Self {
        Self {
            next: b.list.head(),
            box_index: index,
            vertical: b.vertical,
            x,
            y: if b.vertical {
                y.wrapping_sub(b.height.0)
            } else {
                y
            },
            left: x,
            baseline: y,
            width: b.width.0,
            height: b.height.0,
            depth: b.depth.0,
            glue: b.glue_set,
            seen: 0.0,
            adjustment: 0,
        }
    }
    fn glue(&mut self, g: Glue) -> i32 {
        let old = self.adjustment;
        if self.glue.sign == GlueSign::Stretching && self.glue.order == g.stretch_order {
            self.seen += f64::from(g.stretch.0);
        } else if self.glue.sign == GlueSign::Shrinking && self.glue.order == g.shrink_order {
            self.seen -= f64::from(g.shrink.0);
        }
        self.adjustment = (self.glue.ratio * self.seen)
            .clamp(-1_000_000_000.0, 1_000_000_000.0)
            .round() as i32;
        g.width.0.wrapping_sub(old).wrapping_add(self.adjustment)
    }
}
pub(crate) struct Scratch {
    frames: Buffer<Work>,
}
impl Default for Scratch {
    fn default() -> Self {
        Self {
            frames: Buffer::new(&std::rc::Rc::default()),
        }
    }
}
fn record(b: &BoxData, parent: Option<u32>, x: i32, y: i32) -> PackedBox {
    PackedBox {
        parent,
        vertical: b.vertical,
        x,
        y,
        width: b.width.0,
        height: b.height.0,
        depth: b.depth.0,
        glue: GlueSetting {
            sign: match b.glue_set.sign {
                GlueSign::Normal => 0,
                GlueSign::Stretching => 1,
                GlueSign::Shrinking => -1,
            },
            order: b.glue_set.order as u8,
            ratio: b.glue_set.ratio,
        },
    }
}
impl Engine<'_> {
    pub(crate) fn publish_page(&mut self, node: DetachedNode) -> Result<()> {
        self.document.page.dead_cycles = 0;
        let penalty = self.document.scalars
            [crate::scalars::Target::page(crate::scalars::PageParam::OutputPenalty)]
        .value;
        let Kind::Box(b) = &self.document.nodes.get(node.id()).kind else {
            unreachable!()
        };
        let b = self.document.nodes.box_data(b);
        if b.height.0 > crate::numeric::Scaled::MAX.0
            || b.depth.0 > crate::numeric::Scaled::MAX.0
            || i64::from(b.height.0)
                + i64::from(b.depth.0)
                + i64::from(
                    self.document.layout_values.dimens[DimenParam::VOffset.index()]
                        .value
                        .0,
                )
                > i64::from(crate::numeric::Scaled::MAX.0)
            || i64::from(b.width.0)
                + i64::from(
                    self.document.layout_values.dimens[DimenParam::HOffset.index()]
                        .value
                        .0,
                )
                > i64::from(crate::numeric::Scaled::MAX.0)
        {
            self.document
                .nodes
                .release_node(node, &mut self.page_release);
            return Err(Error::Overflow);
        }
        if self.document.page.output.is_empty()
            && let Err(error) = self.prepare_magnification()
        {
            self.document
                .nodes
                .release_node(node, &mut self.page_release);
            return Err(error);
        }
        let mut scratch = self
            .page_lowering
            .take()
            .expect("nonrecursive page lowering");
        let mut page = Page::new(
            &self.output_pools,
            self.document.scalars.counts().map(|i| i.value),
            penalty,
        );
        let result = self.lower_page(node.id(), &mut page, &mut scratch);
        scratch.frames.clear();
        self.page_lowering = Some(scratch);
        self.document
            .nodes
            .release_node(node, &mut self.page_release);
        result?;
        self.document.page.output.push(page);
        Ok(())
    }
    fn lower_page(&self, id: NodeId, page: &mut Page, scratch: &mut Scratch) -> Result<()> {
        let Kind::Box(b) = &self.document.nodes.get(id).kind else {
            return Err(Error::Unsupported);
        };
        let b = self.document.nodes.box_data(b);
        // These TeX offsets precede the backend's implicit one-inch origin.
        let x = self.document.layout_values.dimens[DimenParam::HOffset.index()]
            .value
            .0;
        let y = b.height.0.wrapping_add(
            self.document.layout_values.dimens[DimenParam::VOffset.index()]
                .value
                .0,
        );
        let index = page.box_record(record(b, None, x, y));
        scratch.frames.push(Work::List(Frame::new(b, index, x, y)));
        let mut runs = Runs::default();
        let mut font_binding: Option<(u32, &crate::tfm::ScaledFont, u32)> = None;
        while let Some(work) = scratch.frames.last_mut() {
            if let Work::Repeat(repeat) = work {
                let Some((x, y)) = repeat.next() else {
                    scratch.frames.pop();
                    continue;
                };
                let Kind::Box(id) = &self.document.nodes.get(repeat.template).kind else {
                    unreachable!()
                };
                let b = self.document.nodes.box_data(id);
                runs.finish(page);
                let index = page.box_record(record(b, Some(repeat.parent), x, y));
                if !b.list.is_empty() {
                    scratch.frames.push(Work::List(Frame::new(b, index, x, y)));
                }
                continue;
            }
            let Work::List(frame) = work else {
                unreachable!()
            };
            if frame.next.is_null() {
                runs.finish(page);
                scratch.frames.pop();
                continue;
            }
            let node = self.document.nodes.get(frame.next);
            frame.next = node.next;
            if !matches!(node.kind, Kind::Character { .. } | Kind::Ligature { .. }) {
                runs.finish(page);
            }
            match &node.kind {
                Kind::Character { font, character }
                | Kind::Ligature {
                    font, character, ..
                } => {
                    if frame.vertical {
                        return Err(Error::Unsupported);
                    }
                    let (m, local) = match font_binding {
                        Some((id, metrics, local)) if id == *font => {
                            (metrics.metrics(*character).ok_or(Error::Font)?, local)
                        }
                        _ => {
                            if *font as usize >= self.document.fonts.len {
                                return Err(Error::Font);
                            }
                            let scaled = self.document.fonts.descriptors[*font as usize]
                                .scaled
                                .ok_or(Error::Font)?;
                            let resource = &self.font_catalog.scaled[scaled];
                            let metrics = resource.descriptor();
                            let m = metrics.metrics(*character).ok_or(Error::Font)?;
                            runs.finish(page);
                            let local = page.font(resource);
                            font_binding = Some((*font, metrics, local));
                            (m, local)
                        }
                    };
                    runs.glyph(page, local, *character, frame.x, frame.baseline);
                    frame.x = frame.x.wrapping_add(m.width.0);
                }
                Kind::Box(b) => {
                    let b = self.document.nodes.box_data(b);
                    let (x, y) = if frame.vertical {
                        (
                            frame.left.wrapping_add(b.shift.0),
                            frame.y.wrapping_add(b.height.0),
                        )
                    } else {
                        (frame.x, frame.baseline.wrapping_add(b.shift.0))
                    };
                    let index = page.box_record(record(b, Some(frame.box_index), x, y));
                    if frame.vertical {
                        frame.y = frame.y.wrapping_add(b.height.0).wrapping_add(b.depth.0);
                    } else {
                        frame.x = frame.x.wrapping_add(b.width.0);
                    }
                    if !b.list.is_empty() {
                        scratch.frames.push(Work::List(Frame::new(b, index, x, y)));
                    }
                }
                Kind::Rule {
                    width,
                    height,
                    depth,
                } => {
                    frame.rule(width.0, height.0, depth.0, page);
                }
                Kind::Leader(id) => {
                    if let Some(repeat) = self.lower_leader(id, frame, page) {
                        scratch.frames.push(Work::Repeat(repeat));
                    }
                }
                Kind::Glue { spec, .. } => {
                    let width = frame.glue(*spec);
                    if frame.vertical {
                        frame.y = frame.y.wrapping_add(width);
                    } else {
                        frame.x = frame.x.wrapping_add(width);
                    }
                }
                Kind::Kern { width, .. } => {
                    if frame.vertical {
                        frame.y = frame.y.wrapping_add(width.0);
                    } else {
                        frame.x = frame.x.wrapping_add(width.0);
                    }
                }
                Kind::MathBoundary { width, .. } => {
                    frame.x = frame.x.wrapping_add(width.0);
                }
                Kind::Penalty(_)
                | Kind::Discretionary { .. }
                | Kind::Insertion(_)
                | Kind::Adjustment(_)
                | Kind::Language(_) => {}
                Kind::Free => unreachable!(),
            }
        }
        Ok(())
    }
}
pub(crate) const RUNNING: i32 = -1_073_741_824;

#[cfg(test)]
#[path = "page_offset_tests.rs"]
mod offset_tests;
