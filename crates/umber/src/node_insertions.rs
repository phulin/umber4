//! Insertion payload roots travel with their matching node roots.
use super::{CELLS, DetachedNode, InsertionData, InsertionId, Kind, NodeId, NodeStore};
use crate::Result;
use crate::tables::Table;

#[derive(Debug)]
enum Slot {
    Free(u32),
    Live(InsertionData),
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
            Self::Live(data) => Self::Live(InsertionData {
                list: data.list.snapshot(),
                class: data.class,
                height: data.height,
                split_top: data.split_top,
                max_depth: data.max_depth,
                float_cost: data.float_cost,
            }),
        }
    }
}
#[derive(Clone)]
pub(super) struct Insertions {
    slots: Table<Slot>,
    free: u32,
}
impl Default for Insertions {
    fn default() -> Self {
        Self {
            slots: Table::new(0),
            free: 0,
        }
    }
}
impl Insertions {
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.slots.len()
    }
    pub fn alloc(&mut self, data: InsertionData) -> InsertionId {
        let id = if self.free == 0 {
            // Slots are dense and reused before growth; their high-water cannot
            // exceed the existing node address budget, which is below u32::MAX.
            let id = u32::try_from(self.slots.len() + 1).expect("insertion slot address");
            self.slots.grow(id as usize);
            id
        } else {
            let id = self.free;
            let Slot::Free(next) = self.slots[id as usize - 1] else {
                unreachable!("free insertion slot")
            };
            self.free = next;
            id
        };
        self.slots[id as usize - 1] = Slot::Live(data);
        InsertionId(id)
    }
    pub fn get(&self, id: &InsertionId) -> &InsertionData {
        let Slot::Live(data) = &self.slots[id.0 as usize - 1] else {
            panic!("released insertion")
        };
        data
    }
    pub fn get_mut(&mut self, id: u32) -> &mut InsertionData {
        let Slot::Live(data) = &mut self.slots[id as usize - 1] else {
            panic!("released insertion")
        };
        data
    }
    pub fn take(&mut self, id: InsertionId) -> InsertionData {
        let old = std::mem::replace(&mut self.slots[id.0 as usize - 1], Slot::Free(self.free));
        let Slot::Live(data) = old else {
            panic!("released insertion")
        };
        self.free = id.0;
        data
    }
}

impl NodeStore {
    pub fn insertion_data(&self, id: &InsertionId) -> &InsertionData {
        self.insertions.get(id)
    }
    pub fn insertion_mut(&mut self, node: NodeId) -> &mut InsertionData {
        let Kind::Insertion(id) = &self.get(node).kind else {
            panic!("not an insertion")
        };
        self.insertions.get_mut(id.0)
    }
    pub fn alloc_insertion(&mut self, data: InsertionData) -> Result<DetachedNode> {
        self.alloc_insertion_with_limit(data, (u32::MAX as usize) / CELLS)
    }
    pub(super) fn alloc_insertion_with_limit(
        &mut self,
        data: InsertionData,
        limit: usize,
    ) -> Result<DetachedNode> {
        let page = match self.allocation_page(limit) {
            Ok(page) => page,
            Err(error) => {
                let pool = self.pool.clone();
                self.release(data.list, &mut pool.failed_release.borrow_mut());
                return Err(error);
            }
        };
        let id = self.insertions.alloc(data);
        Ok(self.install(page, Kind::Insertion(id)))
    }
}
