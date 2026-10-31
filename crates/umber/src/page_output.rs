//! Synchronous lowering over live lists; all retained output is detached.
use crate::{
    Engine, Error, Result,
    nodes::{BoxData, DetachedNode, GlueSet, GlueSign, Kind, NodeId},
    numeric::Glue,
    output::{GlueSetting, PackedBox, Page, Placement},
    value_metadata::Buffer,
};
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
    frames: Buffer<Frame>,
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
    pub(crate) fn publish_page(&mut self, node: DetachedNode, penalty: i32) -> Result<()> {
        let Kind::Box(b) = &self.document.nodes.get(node.id()).kind else {
            unreachable!()
        };
        if b.height.0 > crate::numeric::Scaled::MAX.0
            || b.depth.0 > crate::numeric::Scaled::MAX.0
            || i64::from(b.height.0) + i64::from(b.depth.0)
                > i64::from(crate::numeric::Scaled::MAX.0)
            || b.width.0 > crate::numeric::Scaled::MAX.0
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
        // Geometry precedes any backend-specific origin offset.
        let index = page.box_record(record(b, None, 0, b.height.0));
        scratch.frames.push(Frame::new(b, index, 0, b.height.0));
        while let Some(frame) = scratch.frames.last_mut() {
            if frame.next.is_null() {
                scratch.frames.pop();
                continue;
            }
            let node = self.document.nodes.get(frame.next);
            frame.next = node.next;
            match &node.kind {
                Kind::Character { font, character }
                | Kind::Ligature {
                    font, character, ..
                } => {
                    if frame.vertical {
                        return Err(Error::Unsupported);
                    }
                    let m = self
                        .character_metrics(*font, *character)
                        .ok_or(Error::Font)?;
                    let scaled = self.document.fonts.descriptors[*font as usize]
                        .scaled
                        .ok_or(Error::Font)?;
                    let local = page.font(&self.font_catalog.scaled[scaled]);
                    page.place(Placement::Glyph {
                        font: local,
                        character: *character,
                        x: frame.x,
                        y: frame.baseline,
                    });
                    frame.x = frame.x.wrapping_add(m.width.0);
                }
                Kind::Box(b) => {
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
                        scratch.frames.push(Frame::new(b, index, x, y));
                    }
                }
                Kind::Rule {
                    width,
                    height,
                    depth,
                } => {
                    let (w, h, d) = (
                        if width.0 == RUNNING {
                            frame.width
                        } else {
                            width.0
                        },
                        if height.0 == RUNNING {
                            frame.height
                        } else {
                            height.0
                        },
                        if depth.0 == RUNNING {
                            frame.depth
                        } else {
                            depth.0
                        },
                    );
                    let thickness = h.wrapping_add(d);
                    let (x, y) = if frame.vertical {
                        let y = frame.y;
                        frame.y = frame.y.wrapping_add(thickness);
                        (frame.left, y)
                    } else {
                        let x = frame.x;
                        frame.x = frame.x.wrapping_add(w);
                        (x, frame.baseline.wrapping_sub(h))
                    };
                    if w > 0 && thickness > 0 {
                        page.place(Placement::Rule {
                            x,
                            y,
                            width: w,
                            height: thickness,
                        });
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
                Kind::Penalty(_) | Kind::Discretionary { .. } => {}
                Kind::Free => unreachable!(),
            }
        }
        Ok(())
    }
}
pub(crate) const RUNNING: i32 = -1_073_741_824;
