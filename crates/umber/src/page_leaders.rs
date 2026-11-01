//! Leader placement borrows one template; repeat cursors are traversal scratch.
use super::{Frame, RUNNING};
use crate::{
    Engine,
    nodes::{BoxData, GlueKind, Kind, LeaderId, NodeId},
    output::Page,
};

#[derive(Clone, Copy)]
pub(super) struct Repeat {
    pub template: NodeId,
    pub parent: u32,
    vertical: bool,
    position: i32,
    edge: i32,
    step: i32,
    length: i32,
    fixed: i32,
    height: i32,
}
impl Repeat {
    fn new(
        frame: &Frame,
        template: NodeId,
        b: &BoxData,
        alignment: GlueKind,
        extent: i32,
    ) -> Option<Self> {
        let length = if frame.vertical {
            b.height.0.wrapping_add(b.depth.0)
        } else {
            b.width.0
        };
        if length <= 0 || extent <= 0 {
            return None;
        }
        let start = if frame.vertical { frame.y } else { frame.x };
        let space = extent.wrapping_add(10);
        let edge = start.wrapping_add(space);
        let mut position = start;
        let mut gap = 0;
        if alignment == GlueKind::AlignedLeaders {
            let origin = if frame.vertical {
                frame.baseline.wrapping_sub(frame.height)
            } else {
                frame.left
            };
            position =
                origin.wrapping_add(length.wrapping_mul(start.wrapping_sub(origin) / length));
            if position < start {
                position = position.wrapping_add(length);
            }
        } else {
            let count = space / length;
            let remainder = space % length;
            if alignment == GlueKind::CenteredLeaders {
                position = position.wrapping_add(remainder / 2);
            } else {
                gap = remainder / (count + 1);
                position = position
                    .wrapping_add(remainder.wrapping_sub((count - 1).wrapping_mul(gap)) / 2);
            }
        }
        Some(Self {
            template,
            parent: frame.box_index,
            vertical: frame.vertical,
            position,
            edge,
            step: length.wrapping_add(gap),
            length,
            fixed: if frame.vertical {
                frame.left.wrapping_add(b.shift.0)
            } else {
                frame.baseline.wrapping_add(b.shift.0)
            },
            height: b.height.0,
        })
    }
    pub fn next(&mut self) -> Option<(i32, i32)> {
        if self.position.wrapping_add(self.length) > self.edge {
            return None;
        }
        let coordinates = if self.vertical {
            (self.fixed, self.position.wrapping_add(self.height))
        } else {
            (self.position, self.fixed)
        };
        self.position = self.position.wrapping_add(self.step);
        Some(coordinates)
    }
}
impl Frame {
    pub(super) fn advance(&mut self, extent: i32) {
        if self.vertical {
            self.y = self.y.wrapping_add(extent);
        } else {
            self.x = self.x.wrapping_add(extent);
        }
    }
    pub(super) fn rule(&mut self, width: i32, height: i32, depth: i32, page: &mut Page) {
        let width = if self.vertical && width == RUNNING {
            self.width
        } else {
            width
        };
        let height = if !self.vertical && height == RUNNING {
            self.height
        } else {
            height
        };
        let depth = if !self.vertical && depth == RUNNING {
            self.depth
        } else {
            depth
        };
        let thickness = height.wrapping_add(depth);
        let (x, y) = if self.vertical {
            let y = self.y;
            self.advance(thickness);
            (self.left, y)
        } else {
            let x = self.x;
            self.advance(width);
            (x, self.baseline.wrapping_sub(height))
        };
        if width > 0 && thickness > 0 {
            page.rule(x, y, width, thickness);
        }
    }
}
impl Engine<'_> {
    pub(super) fn lower_leader(
        &self,
        id: &LeaderId,
        frame: &mut Frame,
        page: &mut Page,
    ) -> Option<Repeat> {
        let data = self.document.nodes.leader_data(id);
        let extent = frame.glue(data.spec);
        if !data.subtype.is_leader() {
            frame.advance(extent);
            return None;
        }
        let template = data.list.head();
        match &self.document.nodes.get(template).kind {
            Kind::Rule {
                width,
                height,
                depth,
            } => {
                if frame.vertical {
                    frame.rule(width.0, extent, 0, page);
                } else {
                    frame.rule(extent, height.0, depth.0, page);
                }
                None
            }
            Kind::Box(id) => {
                let b = self.document.nodes.box_data(id);
                let repeat = Repeat::new(frame, template, b, data.subtype, extent);
                frame.advance(extent);
                repeat
            }
            _ => unreachable!("leader template"),
        }
    }
}
