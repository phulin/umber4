//! Scoped box construction and affine box-register bindings.
use crate::{
    Engine, Error, Result,
    execute::{GroupKind, Save},
    layout_values::DimenParam,
    nodes::{DetachedNode, Kind, List, NodeId},
    numeric::Scaled,
    packing::PackSpec,
    tables::Table,
    text::{Mode, TextState},
    token::*,
};
#[path = "insertions.rs"]
mod insertions;
#[path = "box_transfer.rs"]
mod transfer;

#[derive(Default)]
struct Slot {
    list: List,
    level: u16,
}
impl Clone for Slot {
    fn clone(&self) -> Self {
        // Only table COW duplicates live slots; roots retain the matching nodes.
        Self {
            list: self.list.snapshot(),
            level: self.level,
        }
    }
}
pub(crate) struct Registers(Table<Slot>);
impl Default for Registers {
    fn default() -> Self {
        Self(Table::new(256))
    }
}
impl Registers {
    pub fn snapshot(&self) -> Self {
        Self(self.0.clone())
    }
    pub fn head(&self, index: usize) -> NodeId {
        self.0[index].list.head()
    }
}
#[derive(Clone, Copy)]
pub(crate) enum Destination {
    Shipout,
    Append(Scaled),
    MathAppend(Scaled),
    Register { index: usize, global: bool },
    Insertion(u8),
    Leader(crate::nodes::GlueKind),
}
pub(crate) struct Frame {
    outer: TextState,
    spec: PackSpec,
    destination: Destination,
    kind: GroupKind,
    level: u16,
    captured: Option<insertions::Captured>,
    adjusted: bool,
}

impl Engine<'_> {
    pub(crate) fn set_box(&mut self, global: bool) -> Result<()> {
        let index = self.scan_index()?;
        self.scan_equals()?;
        self.scan_box(Destination::Register { index, global })
    }
    pub(crate) fn scan_box(&mut self, destination: Destination) -> Result<()> {
        let read = loop {
            let read = self.nonspace(true)?;
            if read.meaning.command != cmd::RELAX {
                break read;
            }
        };
        if read.meaning.command == cmd::RULE
            && let Destination::Leader(alignment) = destination
        {
            let rule = self.scan_rule(read.meaning.value == 1)?;
            let node = self.document.nodes.alloc(rule)?;
            return self.finish_leader(alignment, node);
        }
        if read.meaning.command != cmd::MAKE_BOX {
            return Err(Error::Syntax);
        }
        self.make_box(read.meaning.value, destination)
    }
    pub(crate) fn make_box(&mut self, operation: i32, destination: Destination) -> Result<()> {
        if (2..=4).contains(&operation) {
            let spec = if self.scan_keyword(b"to")? {
                PackSpec::To(self.scan_dimen()?)
            } else if self.scan_keyword(b"spread")? {
                PackSpec::Spread(self.scan_dimen()?)
            } else {
                PackSpec::Natural
            };
            let kind = match operation {
                2 => GroupKind::Hbox,
                3 => GroupKind::Vbox,
                _ => GroupKind::Vtop,
            };
            self.open_group(kind)?;
            self.opening_brace()?;
            if kind != GroupKind::Hbox {
                self.normal_paragraph()?;
            }
            let mode = if kind == GroupKind::Hbox {
                Mode::RestrictedHorizontal
            } else {
                Mode::InternalVertical
            };
            let outer = std::mem::replace(
                &mut self.document.text,
                TextState {
                    mode,
                    vertical_mode: mode,
                    ..TextState::default()
                },
            );
            let outer_mode = outer.mode;
            self.document.box_frames.push(Frame {
                outer,
                spec,
                destination,
                kind,
                level: self.document.level,
                captured: None,
                adjusted: kind == GroupKind::Hbox
                    && matches!(destination, Destination::Append(_))
                    && outer_mode.is_vertical(),
            });
            // SETBOX's afterassignment token is backed above this hook by the
            // ordinary assignment epilogue, before construction resumes.
            let hook = if kind == GroupKind::Hbox { 257 } else { 259 };
            if let Some(owner) = self.document.token_registers[hook].owner.clone() {
                self.push_tokens(owner, 0)?;
            }
            return Ok(());
        }
        if operation == 5 {
            let node = self.take_last_box()?;
            return self.finish_box(destination, node);
        }
        let index = self.scan_index()?;
        let mut list = if operation == 0 {
            // Taking a box changes its pointer, never its assignment level.
            std::mem::take(&mut self.document.boxes.0[index].list)
        } else {
            let source = self.document.boxes.head(index);
            self.document.nodes.copy_list(source, &mut self.box_copy)?
        };
        let node = self.document.nodes.pop_front(&mut list);
        debug_assert!(list.is_empty());
        self.finish_box(destination, node)
    }
    pub(crate) fn prepare_box_end(&mut self, kind: GroupKind) -> Result<Option<Scaled>> {
        let Some(frame) = self.document.box_frames.last() else {
            return Ok(None);
        };
        if kind != GroupKind::Simple || frame.level != self.document.level {
            return Ok(None);
        }
        let construction = frame.kind;
        if construction != GroupKind::Hbox {
            self.end_graf()?;
        }
        if construction == GroupKind::Insert {
            let captured = insertions::Captured::read(self);
            self.document.box_frames.last_mut().unwrap().captured = Some(captured);
        }
        Ok(Some(
            self.document.layout_values.dimens[DimenParam::BoxMaxDepth.index()].value,
        ))
    }
    pub(crate) fn finish_box_group(&mut self, max_depth: Scaled) -> Result<()> {
        let frame = self.document.box_frames.pop().expect("box construction");
        let mut adjustments = List::default();
        let result = if frame.kind == GroupKind::Hbox {
            let contents = std::mem::take(&mut self.document.text.horizontal);
            if frame.adjusted {
                self.hpack_adjusted(contents, frame.spec, &mut adjustments)
            } else {
                self.hpack(contents, frame.spec)
            }
        } else {
            let contents = std::mem::take(&mut self.document.text.vertical);
            let max_depth = if frame.kind == GroupKind::Insert {
                Scaled::MAX
            } else {
                max_depth
            };
            self.vpackage(contents, frame.spec, max_depth)
        };
        // Both success and failure restore the enclosing owner. Allocation
        // rejection has already released the transferred box contents.
        self.document.text = frame.outer;
        let node = match result {
            Ok(node) => node,
            Err(error) => {
                self.document
                    .nodes
                    .release(adjustments, &mut self.page_release);
                return Err(error);
            }
        };
        if let Destination::Insertion(class) = frame.destination {
            return self.finish_insertion(class, node, frame.captured.expect("insertion capture"));
        }
        if frame.kind == GroupKind::Vtop {
            let Kind::Box(id) = &self.document.nodes.get(node.id()).kind else {
                unreachable!("packed box")
            };
            let data = self.document.nodes.box_data(id);
            let (first, total) = (data.list.head(), data.height.0.wrapping_add(data.depth.0));
            let height = if first.is_null() {
                Scaled::ZERO
            } else {
                match &self.document.nodes.get(first).kind {
                    Kind::Box(id) => self.document.nodes.box_data(id).height,
                    Kind::Rule { height, .. } => *height,
                    _ => Scaled::ZERO,
                }
            };
            let data = self.document.nodes.box_mut(node.id());
            data.height = height;
            data.depth = Scaled(total.wrapping_sub(height.0));
        }
        if frame.adjusted {
            let Destination::Append(shift) = frame.destination else {
                unreachable!()
            };
            self.document.nodes.box_mut(node.id()).shift = shift;
            // Establish ownership even when allocating baseline glue fails.
            let result = self.append_to_vlist(node);
            self.document
                .nodes
                .splice(&mut self.document.text.vertical, adjustments);
            result?;
            if self.document.text.mode == Mode::Vertical {
                self.build_page()?;
            }
            Ok(())
        } else {
            self.finish_box(frame.destination, Some(node))
        }
    }
    fn finish_box(&mut self, destination: Destination, node: Option<DetachedNode>) -> Result<()> {
        if let Destination::Leader(alignment) = destination {
            return if let Some(node) = node {
                self.finish_leader(alignment, node)
            } else {
                Ok(())
            };
        }
        if let Destination::MathAppend(shift) = destination {
            if let Some(node) = node {
                self.document.nodes.box_mut(node.id()).shift = shift;
                self.append_math_box(node)?;
            }
            return Ok(());
        }
        if let Destination::Shipout = destination {
            if let Some(node) = node {
                self.publish_page(node)?;
            }
        } else if let Destination::Register { index, global } = destination {
            let mut list = List::default();
            if let Some(node) = node {
                self.document.nodes.append(&mut list, node);
            }
            let level = if global { 0 } else { self.document.level };
            let old = std::mem::replace(&mut self.document.boxes.0[index], Slot { list, level });
            if !global && level > 0 && old.level != level {
                self.document
                    .save
                    .push(Save::Box(index, old.list, old.level));
            } else {
                self.document
                    .nodes
                    .release(old.list, &mut self.page_release);
            }
        } else if let Some(node) = node {
            let Destination::Append(shift) = destination else {
                unreachable!()
            };
            self.document.nodes.box_mut(node.id()).shift = shift;
            if self.document.text.mode.is_vertical() {
                self.append_to_vlist(node)?;
                if self.document.text.mode == Mode::Vertical {
                    self.build_page()?;
                }
            } else {
                self.document.text.space_factor = 1000;
                self.document
                    .nodes
                    .append(&mut self.document.text.horizontal, node);
            }
        }
        Ok(())
    }
    pub(crate) fn install_output_box(&mut self, node: DetachedNode) {
        debug_assert!(self.document.boxes.head(255).is_null());
        self.document
            .nodes
            .append(&mut self.document.boxes.0[255].list, node);
    }
    pub(crate) fn take_output_box(&mut self) -> DetachedNode {
        self.document
            .nodes
            .pop_front(&mut self.document.boxes.0[255].list)
            .expect("prepared page")
    }
    pub(crate) fn restore_box(&mut self, index: usize, list: List, level: u16) {
        let discarded = if self.document.boxes.0[index].level == 0 {
            list
        } else {
            std::mem::replace(&mut self.document.boxes.0[index], Slot { list, level }).list
        };
        self.document
            .nodes
            .release(discarded, &mut self.page_release);
    }
    pub(crate) fn box_dimension(&mut self, dimension: i32) -> Result<Scaled> {
        let index = self.scan_index()?;
        let head = self.document.boxes.head(index);
        if head.is_null() {
            return Ok(Scaled::ZERO);
        }
        let Kind::Box(id) = &self.document.nodes.get(head).kind else {
            unreachable!("box register")
        };
        let data = self.document.nodes.box_data(id);
        Ok(match dimension {
            0 => data.width,
            1 => data.height,
            _ => data.depth,
        })
    }
    pub(crate) fn assign_box_dimension(&mut self, dimension: i32) -> Result<()> {
        let index = self.scan_index()?;
        self.scan_equals()?;
        let value = self.scan_dimen()?;
        let head = self.document.boxes.head(index);
        if !head.is_null() {
            let data = self.document.nodes.box_mut(head);
            match dimension {
                0 => data.width = value,
                1 => data.height = value,
                _ => data.depth = value,
            }
        }
        Ok(())
    }
    pub(crate) fn box_condition(&mut self, command: u8) -> Result<bool> {
        let index = self.scan_index()?;
        let head = self.document.boxes.head(index);
        if command == cmd::IFVOID {
            return Ok(head.is_null());
        }
        if head.is_null() {
            return Ok(false);
        }
        let Kind::Box(id) = &self.document.nodes.get(head).kind else {
            unreachable!("box register")
        };
        Ok(self.document.nodes.box_data(id).vertical == (command == cmd::IFVBOX))
    }
}

#[cfg(test)]
#[path = "boxes_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "vertical_box_tests.rs"]
mod vertical_tests;
