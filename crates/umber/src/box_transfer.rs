//! Consuming/copying unboxing and tail/shift operations over existing owners.
use super::*;

impl Engine<'_> {
    pub(crate) fn shift_box(&mut self, operation: i32) -> Result<()> {
        let allowed = if operation < 2 {
            self.document.text.mode.is_horizontal()
        } else {
            self.document.text.mode.is_vertical()
        };
        if !allowed {
            return Err(Error::Syntax);
        }
        let amount = self.scan_dimen()?;
        let shift = if operation & 1 != 0 {
            Scaled(amount.0.wrapping_neg())
        } else {
            amount
        };
        self.scan_box(Destination::Append(shift))
    }
    pub(crate) fn unbox(&mut self, operation: i32) -> Result<()> {
        let vertical = operation >= 2;
        if self.document.text.mode.is_vertical() != vertical {
            return Err(Error::Syntax);
        }
        let index = self.scan_index()?;
        let head = self.document.boxes.head(index);
        if head.is_null() {
            return Ok(());
        }
        let Kind::Box(id) = &self.document.nodes.get(head).kind else {
            unreachable!("box register")
        };
        let data = self.document.nodes.box_data(id);
        if data.vertical != vertical {
            return Err(Error::Syntax);
        }
        let source = data.list.head();
        let contents = if operation & 1 != 0 {
            self.document.nodes.copy_list(source, &mut self.box_copy)?
        } else {
            let mut owner = std::mem::take(&mut self.document.boxes.0[index].list);
            let node = self
                .document
                .nodes
                .pop_front(&mut owner)
                .expect("box owner");
            let contents = std::mem::take(&mut self.document.nodes.box_mut(node.id()).list);
            self.document
                .nodes
                .release_node(node, &mut self.page_release);
            contents
        };
        let text = &mut self.document.text;
        let target = if vertical {
            &mut text.vertical
        } else {
            &mut text.horizontal
        };
        self.document.nodes.splice(target, contents);
        Ok(())
    }
    pub(super) fn take_last_box(&mut self) -> Result<Option<DetachedNode>> {
        let text = &self.document.text;
        let list = if text.mode.is_horizontal() {
            &text.horizontal
        } else {
            &text.vertical
        };
        if list.is_empty() {
            return if text.mode == Mode::Vertical {
                Err(Error::Syntax)
            } else {
                Ok(None)
            };
        }
        let tail = list.tail();
        if !matches!(self.document.nodes.get(tail).kind, Kind::Box(_)) {
            return Ok(None);
        }
        let mut current = list.head();
        let mut previous = NodeId::NULL;
        while current != tail {
            let node = self.document.nodes.get(current);
            previous = current;
            if let Kind::Discretionary { replace_count, .. } = node.kind {
                for _ in 0..replace_count {
                    previous = self.document.nodes.get(previous).next;
                }
                if previous == tail {
                    return Ok(None);
                }
            }
            current = self.document.nodes.get(previous).next;
        }
        let text = &mut self.document.text;
        let list = if text.mode.is_horizontal() {
            &mut text.horizontal
        } else {
            &mut text.vertical
        };
        let mut suffix = self.document.nodes.split_after(list, previous);
        let node = self
            .document
            .nodes
            .pop_front(&mut suffix)
            .expect("tail box");
        self.document.nodes.box_mut(node.id()).shift = Scaled::ZERO;
        Ok(Some(node))
    }
}
