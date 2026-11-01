//! Restricted horizontal construction and affine box-register bindings.
use crate::{
    Engine, Error, Result,
    execute::{GroupKind, Save},
    nodes::{DetachedNode, Kind, List, NodeId},
    numeric::Scaled,
    packing::PackSpec,
    tables::Table,
    text::{Mode, TextState},
    token::*,
};

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
    Append,
    Register { index: usize, global: bool },
}
pub(crate) struct Frame {
    outer: TextState,
    spec: PackSpec,
    destination: Destination,
}

impl Engine<'_> {
    pub(crate) fn set_box(&mut self, global: bool) -> Result<()> {
        let index = self.scan_index()?;
        self.scan_equals()?;
        let read = loop {
            let read = self.nonspace(true)?;
            if read.meaning.command != cmd::RELAX {
                break read;
            }
        };
        if read.meaning.command != cmd::MAKE_BOX {
            return Err(Error::Syntax);
        }
        self.make_box(read.meaning.value, Destination::Register { index, global })
    }
    pub(crate) fn make_box(&mut self, operation: i32, destination: Destination) -> Result<()> {
        if operation > 2 {
            return Err(Error::Unsupported);
        }
        if operation == 2 {
            let spec = if self.scan_keyword(b"to")? {
                PackSpec::To(self.scan_dimen()?)
            } else if self.scan_keyword(b"spread")? {
                PackSpec::Spread(self.scan_dimen()?)
            } else {
                PackSpec::Natural
            };
            self.open_group(GroupKind::Hbox)?;
            self.opening_brace()?;
            let outer = std::mem::replace(
                &mut self.document.text,
                TextState {
                    mode: Mode::RestrictedHorizontal,
                    ..TextState::default()
                },
            );
            self.document.box_frames.push(Frame {
                outer,
                spec,
                destination,
            });
            // SETBOX's afterassignment token is backed above this hook by the
            // ordinary assignment epilogue, before construction resumes.
            if let Some(owner) = self.document.token_registers[257].owner.clone() {
                self.push_tokens(owner, 0)?;
            }
            return Ok(());
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
    pub(crate) fn finish_hbox(&mut self) -> Result<()> {
        let frame = self.document.box_frames.pop().expect("hbox construction");
        let contents = std::mem::take(&mut self.document.text.horizontal);
        let result = self.hpack(contents, frame.spec);
        // Both success and failure restore the enclosing owner. Allocation
        // rejection has already released the transferred box contents.
        self.document.text = frame.outer;
        self.finish_box(frame.destination, Some(result?))
    }
    fn finish_box(&mut self, destination: Destination, node: Option<DetachedNode>) -> Result<()> {
        if let Destination::Register { index, global } = destination {
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
            self.document.nodes.box_mut(node.id()).shift = Scaled::ZERO;
            if self.document.text.mode == Mode::Vertical {
                self.append_to_vlist(node)?;
                self.build_page()?;
            } else {
                self.document.text.space_factor = 1000;
                self.document
                    .nodes
                    .append(&mut self.document.text.horizontal, node);
            }
        }
        Ok(())
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
