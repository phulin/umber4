//! Insertion/adjustment capture uses the existing internal-vertical frames.
use super::{Destination, Frame};
use crate::{
    Engine, Error, Result,
    execute::GroupKind,
    layout_values::{DimenParam, GlueParam},
    nodes::{DetachedNode, InsertionData, Kind, List},
    numeric::{Glue, Scaled},
    packing::PackSpec,
    scalars::Target,
    text::{Mode, TextState},
};

pub(super) struct Captured {
    split_top: Glue,
    max_depth: Scaled,
    float_cost: i32,
}
impl Captured {
    pub fn read(engine: &Engine<'_>) -> Self {
        Self {
            split_top: engine.document.layout_values.glues[GlueParam::SplitTop.index()].value,
            max_depth: engine.document.layout_values.dimens[DimenParam::SplitMaxDepth.index()]
                .value,
            float_cost: engine.document.scalars[Target::FLOATING_PENALTY].value,
        }
    }
}
impl Engine<'_> {
    pub(crate) fn begin_insertion(&mut self, adjustment: bool) -> Result<()> {
        let class = if adjustment {
            if !self.document.text.mode.is_horizontal() {
                return Err(Error::Syntax);
            }
            255
        } else {
            let class = self.scan_index()? as u8;
            if class == 255 {
                return Err(Error::Unsupported);
            }
            class
        };
        self.open_group(GroupKind::Insert)?;
        self.opening_brace()?;
        self.normal_paragraph()?;
        let outer = std::mem::replace(
            &mut self.document.text,
            TextState {
                mode: Mode::InternalVertical,
                vertical_mode: Mode::InternalVertical,
                ..TextState::default()
            },
        );
        self.document.box_frames.push(Frame {
            outer,
            spec: PackSpec::Natural,
            destination: Destination::Insertion(class),
            kind: GroupKind::Insert,
            level: self.document.level,
            captured: None,
            adjusted: false,
        });
        Ok(())
    }
    pub(super) fn finish_insertion(
        &mut self,
        class: u8,
        node: DetachedNode,
        captured: Captured,
    ) -> Result<()> {
        let data = self.document.nodes.take_box(node);
        let node = if class == 255 {
            self.document.nodes.alloc(Kind::Adjustment(data.list))?
        } else {
            self.document.nodes.alloc_insertion(InsertionData {
                list: data.list,
                class,
                height: Scaled(data.height.0.wrapping_add(data.depth.0)),
                split_top: captured.split_top,
                max_depth: captured.max_depth,
                float_cost: captured.float_cost,
            })?
        };
        let list = if self.document.text.mode.is_vertical() {
            &mut self.document.text.vertical
        } else {
            &mut self.document.text.horizontal
        };
        self.document.nodes.append(list, node);
        if self.document.text.mode == Mode::Vertical {
            self.build_page()?;
        }
        Ok(())
    }
    /// Transfer a class register's list while preserving its assignment level.
    pub(crate) fn take_insertion_box(&mut self, class: usize) -> Result<List> {
        let head = self.document.boxes.head(class);
        if head.is_null() {
            return Ok(List::default());
        }
        let Kind::Box(id) = &self.document.nodes.get(head).kind else {
            unreachable!()
        };
        if !self.document.nodes.box_data(id).vertical {
            return Err(Error::Unsupported);
        }
        let node = self
            .document
            .nodes
            .pop_front(&mut self.document.boxes.0[class].list)
            .unwrap();
        Ok(self.document.nodes.take_box(node).list)
    }
    pub(crate) fn install_insertion_box(&mut self, class: usize, node: DetachedNode) {
        debug_assert!(self.document.boxes.head(class).is_null());
        self.document
            .nodes
            .append(&mut self.document.boxes.0[class].list, node);
    }
}
