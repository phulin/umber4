//! Unsplit insertion accounting and consuming transfer at the selected page break.
use super::{Contents, Engine};
use crate::{
    Error, Result,
    nodes::{BoxData, Kind, NodeId},
    numeric::{GlueOrder, Scaled},
    packing::PackSpec,
    scalars::Target,
};

#[derive(Clone, Copy)]
pub(super) struct Class {
    number: u8,
    height: i32,
    pub last: NodeId,
    pub best: NodeId,
}
fn scaled(height: i32, count: i32) -> i32 {
    if count == 1000 {
        height
    } else {
        (height / 1000).wrapping_mul(count)
    }
}
impl Engine<'_> {
    pub(super) fn consider_insertion(&mut self, p: NodeId) -> Result<()> {
        if self.document.page.state.contents == Contents::Empty {
            self.freeze_page(Contents::InsertsOnly);
        }
        let Kind::Insertion(id) = &self.document.nodes.get(p).kind else {
            unreachable!()
        };
        let data = self.document.nodes.insertion_data(id);
        let (number, height) = (data.class, data.height.0);
        let class = number as usize;
        let count = self.document.scalars[Target::count(class)].value;
        let index = match self
            .document
            .page
            .classes
            .binary_search_by_key(&number, |r| r.number)
        {
            Ok(index) => index,
            Err(index) => {
                let head = self.document.boxes.head(class);
                let existing = if head.is_null() {
                    0
                } else {
                    let Kind::Box(id) = &self.document.nodes.get(head).kind else {
                        unreachable!()
                    };
                    let b = self.document.nodes.box_data(id);
                    if !b.vertical {
                        return Err(Error::Unsupported);
                    }
                    b.height.0.wrapping_add(b.depth.0)
                };
                let skip = self.document.layout_values.glues[class].value;
                if skip.shrink.0 != 0 && skip.shrink_order != GlueOrder::Normal {
                    return Err(Error::Unsupported);
                }
                let s = &mut self.document.page.state;
                s.totals[0] = s.totals[0]
                    .wrapping_sub(scaled(existing, count))
                    .wrapping_sub(skip.width.0);
                let order = 2 + skip.stretch_order as usize;
                s.totals[order] = s.totals[order].wrapping_add(skip.stretch.0);
                s.totals[6] = s.totals[6].wrapping_add(skip.shrink.0);
                self.document.page.classes.push(Class {
                    number,
                    height: existing,
                    last: NodeId::NULL,
                    best: NodeId::NULL,
                });
                self.document.page.classes[index..].rotate_right(1);
                index
            }
        };
        let r = &mut self.document.page.classes[index];
        r.last = p;
        let s = &mut self.document.page.state;
        let delta = s.totals[0]
            .wrapping_sub(s.totals[1])
            .wrapping_sub(s.totals[7])
            .wrapping_add(s.totals[6]);
        let h = scaled(height, count);
        if ((h <= 0) || (h <= delta))
            && height.wrapping_add(r.height) <= self.document.layout_values.dimens[class].value.0
        {
            s.totals[0] = s.totals[0].wrapping_sub(h);
            r.height = r.height.wrapping_add(height);
            Ok(())
        } else {
            // This is exactly WEB's internal vert_break branch. The insertion
            // and all children still belong to the contribution root on failure.
            Err(Error::Unsupported)
        }
    }
    pub(super) fn prepare_insertions(&mut self) -> Result<()> {
        if self.document.scalars[Target::HOLDING_INSERTS].value > 0 {
            return Ok(());
        }
        // WEB prepares every selected class in class order before moving any
        // insertion children. A changed nonvertical class fails at this stage.
        for index in 0..self.document.page.classes.len() {
            let r = self.document.page.classes[index];
            if r.best.is_null() {
                continue;
            }
            let class = r.number as usize;
            let head = self.document.boxes.head(class);
            if head.is_null() {
                let node = self.document.nodes.alloc_box(BoxData {
                    vertical: true,
                    ..BoxData::default()
                })?;
                self.install_insertion_box(class, node);
            } else {
                let Kind::Box(id) = &self.document.nodes.get(head).kind else {
                    unreachable!()
                };
                if !self.document.nodes.box_data(id).vertical {
                    return Err(Error::Unsupported);
                }
            }
        }
        Ok(())
    }
    pub(super) fn commit_insertions(&mut self) -> Result<()> {
        if self.document.scalars[Target::HOLDING_INSERTS].value > 0 {
            return Ok(());
        }
        let mut previous = NodeId::NULL;
        let mut p = self.document.page.current.head();
        while !p.is_null() {
            let next = self.document.nodes.get(p).next;
            if let Kind::Insertion(id) = &self.document.nodes.get(p).kind {
                let class = self.document.nodes.insertion_data(id).class as usize;
                let index = self
                    .document
                    .page
                    .classes
                    .binary_search_by_key(&(class as u8), |r| r.number)
                    .unwrap();
                let best = self.document.page.classes[index].best;
                assert!(!best.is_null(), "selected unsplit insertion");
                let head = self.document.boxes.head(class);
                let children = std::mem::take(&mut self.document.nodes.insertion_mut(p).list);
                let mut queue = std::mem::take(&mut self.document.nodes.box_mut(head).list);
                self.document.nodes.splice(&mut queue, children);
                self.document.nodes.box_mut(head).list = queue;
                let node = self
                    .document
                    .nodes
                    .remove_after(&mut self.document.page.current, previous)
                    .unwrap();
                self.document
                    .nodes
                    .release_node(node, &mut self.page_release);
                if p == best {
                    self.document.page.classes[index].best = NodeId::NULL;
                    let contents = self.take_insertion_box(class)?;
                    let node = self.vpackage(contents, PackSpec::Natural, Scaled::MAX)?;
                    self.install_insertion_box(class, node);
                }
            } else {
                previous = p;
            }
            p = next;
        }
        Ok(())
    }
}
