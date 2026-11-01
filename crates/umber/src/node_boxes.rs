//! Box payload roots travel with their matching node roots.
use super::{BoxData, BoxId};
use crate::tables::Table;

#[derive(Debug)]
enum Slot {
    Free(u32),
    Live(BoxData),
}
impl Default for Slot {
    fn default() -> Self {
        Self::Free(0)
    }
}
impl Clone for Slot {
    fn clone(&self) -> Self {
        // Table clones its default Free slot when retaining a root. The only
        // live-slot clone is IndexMut's COW page copy into that root version.
        // Table page drops discard numeric child owners without traversing;
        // the paired node root independently retains their backing chunks.
        match self {
            Self::Free(next) => Self::Free(*next),
            Self::Live(data) => Self::Live(BoxData {
                list: data.list.snapshot(),
                vertical: data.vertical,
                width: data.width,
                height: data.height,
                depth: data.depth,
                shift: data.shift,
                glue_set: data.glue_set,
            }),
        }
    }
}
#[derive(Clone)]
pub(super) struct Boxes {
    slots: Table<Slot>,
    free: u32,
}
impl Default for Boxes {
    fn default() -> Self {
        Self {
            slots: Table::new(0),
            free: 0,
        }
    }
}
impl Boxes {
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.slots.len()
    }
    pub fn alloc(&mut self, data: BoxData) -> BoxId {
        let id = if self.free == 0 {
            // Slots are dense and reused before growth; their high-water cannot
            // exceed the existing node address budget, which is below u32::MAX.
            let id = u32::try_from(self.slots.len() + 1).expect("box slot address");
            self.slots.grow(id as usize);
            id
        } else {
            let id = self.free;
            let Slot::Free(next) = self.slots[id as usize - 1] else {
                unreachable!("free box slot")
            };
            self.free = next;
            id
        };
        self.slots[id as usize - 1] = Slot::Live(data);
        BoxId(id)
    }
    pub fn get(&self, id: &BoxId) -> &BoxData {
        let Slot::Live(data) = &self.slots[id.0 as usize - 1] else {
            panic!("released box")
        };
        data
    }
    pub fn get_mut(&mut self, id: u32) -> &mut BoxData {
        let Slot::Live(data) = &mut self.slots[id as usize - 1] else {
            panic!("released box")
        };
        data
    }
    pub fn take(&mut self, id: BoxId) -> BoxData {
        let old = std::mem::replace(&mut self.slots[id.0 as usize - 1], Slot::Free(self.free));
        let Slot::Live(data) = old else {
            panic!("released box")
        };
        self.free = id.0;
        data
    }
}
