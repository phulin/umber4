//! Leader payload roots travel with their matching node roots.
use super::{CELLS, DetachedNode, GlueKind, Kind, List, NodeId, NodeStore, ReleaseScratch};
use crate::numeric::Glue;

#[derive(Debug)]
pub(crate) struct LeaderId(pub(super) u32);
#[derive(Debug, Default)]
pub(crate) struct LeaderData {
    pub list: List,
    pub spec: Glue,
    pub subtype: GlueKind,
}
use crate::Result;
use crate::tables::Table;

#[derive(Debug)]
enum Slot {
    Free(u32),
    Live(LeaderData),
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
            Self::Live(data) => Self::Live(LeaderData {
                list: data.list.snapshot(),
                spec: data.spec,
                subtype: data.subtype,
            }),
        }
    }
}
#[derive(Clone)]
pub(super) struct Leaders {
    slots: Table<Slot>,
    free: u32,
}
impl Default for Leaders {
    fn default() -> Self {
        Self {
            slots: Table::new(0),
            free: 0,
        }
    }
}
impl Leaders {
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.slots.len()
    }
    pub fn alloc(&mut self, data: LeaderData) -> LeaderId {
        let id = if self.free == 0 {
            // Slots are dense and reused before growth; their high-water cannot
            // exceed the existing node address budget, which is below u32::MAX.
            let id = u32::try_from(self.slots.len() + 1).expect("leader slot address");
            self.slots.grow(id as usize);
            id
        } else {
            let id = self.free;
            let Slot::Free(next) = self.slots[id as usize - 1] else {
                unreachable!("free leader slot")
            };
            self.free = next;
            id
        };
        self.slots[id as usize - 1] = Slot::Live(data);
        LeaderId(id)
    }
    pub fn get(&self, id: &LeaderId) -> &LeaderData {
        let Slot::Live(data) = &self.slots[id.0 as usize - 1] else {
            panic!("released leader")
        };
        data
    }
    pub fn get_mut(&mut self, id: u32) -> &mut LeaderData {
        let Slot::Live(data) = &mut self.slots[id as usize - 1] else {
            panic!("released leader")
        };
        data
    }
    pub fn take(&mut self, id: LeaderId) -> LeaderData {
        let old = std::mem::replace(&mut self.slots[id.0 as usize - 1], Slot::Free(self.free));
        let Slot::Live(data) = old else {
            panic!("released leader")
        };
        self.free = id.0;
        data
    }
}

impl NodeStore {
    pub fn leader_data(&self, id: &LeaderId) -> &LeaderData {
        self.leaders.get(id)
    }
    pub fn leader_mut(&mut self, node: NodeId) -> &mut LeaderData {
        let Kind::Leader(id) = &self.get(node).kind else {
            panic!("not an leader")
        };
        self.leaders.get_mut(id.0)
    }
    pub fn alloc_leader(&mut self, data: LeaderData) -> Result<DetachedNode> {
        self.alloc_leader_with_limit(data, (u32::MAX as usize) / CELLS)
    }
    pub(super) fn alloc_leader_with_limit(
        &mut self,
        data: LeaderData,
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
        let id = self.leaders.alloc(data);
        Ok(self.install(page, Kind::Leader(id)))
    }
}

impl Kind {
    pub fn is_glue(&self) -> bool {
        matches!(self, Self::Glue { .. } | Self::Leader(_))
    }
}
impl NodeStore {
    pub fn glue(&self, kind: &Kind) -> Option<(Glue, GlueKind)> {
        match kind {
            Kind::Glue { spec, subtype } => Some((*spec, *subtype)),
            Kind::Leader(id) => {
                let data = self.leader_data(id);
                Some((data.spec, data.subtype))
            }
            _ => None,
        }
    }
    pub fn set_glue(&mut self, node: NodeId, spec: Glue, subtype: GlueKind) {
        if matches!(self.get(node).kind, Kind::Leader(_)) {
            let data = self.leader_mut(node);
            data.spec = spec;
            data.subtype = subtype;
        } else {
            let Kind::Glue {
                spec: s,
                subtype: t,
            } = self.kind_mut(node)
            else {
                panic!("not glue")
            };
            *s = spec;
            *t = subtype;
        }
    }
    /// Replacing a semantic kind consumes and releases all of its former children.
    pub fn replace_kind(&mut self, node: NodeId, kind: Kind, scratch: &mut ReleaseScratch) {
        let old = std::mem::replace(self.kind_mut(node), kind);
        self.release_kind(old, scratch);
    }
}

impl NodeStore {
    pub fn leader_dimensions(
        &self,
        id: &LeaderId,
    ) -> (
        crate::numeric::Scaled,
        crate::numeric::Scaled,
        crate::numeric::Scaled,
    ) {
        let data = self.leader_data(id);
        if !data.subtype.is_leader() {
            return Default::default();
        }
        match &self.get(data.list.head()).kind {
            Kind::Box(id) => {
                let data = self.box_data(id);
                (data.width, data.height, data.depth)
            }
            Kind::Rule {
                width,
                height,
                depth,
            } => (*width, *height, *depth),
            _ => unreachable!("leader template"),
        }
    }
}
