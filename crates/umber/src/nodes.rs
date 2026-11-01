//! Mutable list cells are addressed within one retained store version.
//! Only chunks own allocation/RC metadata; child links transfer semantic ownership.
use crate::{
    Error, Result,
    numeric::{Glue, GlueOrder, Scaled},
    value_metadata::Buffer,
};
use std::{cell::RefCell, rc::Rc};

#[path = "node_boxes.rs"]
mod box_slots;
#[path = "node_copy.rs"]
mod copying;
#[path = "node_insertions.rs"]
mod insertion_slots;
#[path = "node_leaders.rs"]
mod leader_slots;
pub(crate) use leader_slots::{LeaderData, LeaderId};
#[path = "node_snapshot.rs"]
mod snapshot;
#[path = "node_transfer.rs"]
mod transfer;
pub(crate) use copying::CopyScratch;

const CELLS: usize = 256;
const CACHED: usize = 16;
const NONE: u16 = CELLS as u16;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct NodeId(pub u32);
impl NodeId {
    pub const NULL: Self = Self(0);
    pub fn is_null(self) -> bool {
        self == Self::NULL
    }
}

/// An owned chain in the matching NodeStore. Dropping a list handle alone does
/// not traverse it: callers transfer it or explicitly release it through its store.
#[must_use = "transfer or release this owned list"]
#[derive(Debug, Default)]
pub(crate) struct List {
    head: NodeId,
    tail: NodeId,
}
impl List {
    pub fn head(&self) -> NodeId {
        self.head
    }
    pub fn tail(&self) -> NodeId {
        self.tail
    }
    pub fn is_empty(&self) -> bool {
        self.head.is_null()
    }
    /// Retain only alongside a cloned store root, never as a second mutable list
    /// owner in the same version. This is not TeX's semantic copy_node_list.
    pub fn snapshot(&self) -> Self {
        Self {
            head: self.head,
            tail: self.tail,
        }
    }
    /// The caller transfers an existing detached chain with these endpoints.
    fn from_parts(head: NodeId, tail: NodeId) -> Self {
        assert_eq!(head.is_null(), tail.is_null());
        Self { head, tail }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum GlueSign {
    #[default]
    Normal,
    Stretching,
    Shrinking,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct GlueSet {
    pub sign: GlueSign,
    pub order: GlueOrder,
    pub ratio: f64,
}
#[derive(Debug, Default)]
pub(crate) struct BoxData {
    pub list: List,
    pub vertical: bool,
    pub width: Scaled,
    pub height: Scaled,
    pub depth: Scaled,
    pub shift: Scaled,
    pub glue_set: GlueSet,
}
impl BoxData {
    pub fn new(contents: List) -> Self {
        Self {
            list: contents,
            ..Self::default()
        }
    }
}
/// An affine owner of one payload slot in the matching node-store version.
#[derive(Debug)]
pub(crate) struct BoxId(u32);
#[derive(Debug)]
pub(crate) struct InsertionId(u32);
#[derive(Debug, Default)]
pub(crate) struct InsertionData {
    pub list: List,
    pub class: u8,
    pub height: Scaled,
    pub split_top: Glue,
    pub max_depth: Scaled,
    pub float_cost: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum GlueKind {
    #[default]
    Normal,
    LeftSkip,
    RightSkip,
    ParFillSkip,
    BaselineSkip,
    LineSkip,
    ParSkip,
    TopSkip,
    SpaceSkip,
    XSpaceSkip,
    AlignedLeaders,
    CenteredLeaders,
    ExpandedLeaders,
    TabSkip,
    NonScript,
}
impl GlueKind {
    pub fn is_leader(self) -> bool {
        matches!(
            self,
            Self::AlignedLeaders | Self::CenteredLeaders | Self::ExpandedLeaders
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KernKind {
    Normal,
    Explicit,
    Accent,
}
#[derive(Debug, Default)]
pub(crate) enum Kind {
    Language(crate::language::Language),
    #[default]
    Free,
    Character {
        font: u32,
        character: u8,
    },
    Ligature {
        font: u32,
        character: u8,
        components: List,
        left_boundary: bool,
        right_boundary: bool,
    },
    Kern {
        width: Scaled,
        kind: KernKind,
    },
    MathBoundary {
        width: Scaled,
        after: bool,
    },
    Glue {
        spec: Glue,
        subtype: GlueKind,
    },
    Penalty(i32),
    Rule {
        width: Scaled,
        height: Scaled,
        depth: Scaled,
    },
    Box(BoxId),
    Unset(BoxId),
    Insertion(InsertionId),
    Leader(LeaderId),
    Adjustment(List),
    Discretionary {
        pre: List,
        post: List,
        replace_count: u8,
    },
}
impl Kind {
    pub fn ligature(
        font: u32,
        character: u8,
        components: List,
        left_boundary: bool,
        right_boundary: bool,
    ) -> Self {
        Self::Ligature {
            font,
            character,
            components,
            left_boundary,
            right_boundary,
        }
    }
    pub fn discretionary(pre: List, post: List, replace_count: u8) -> Self {
        Self::Discretionary {
            pre,
            post,
            replace_count,
        }
    }
}
#[derive(Debug, Default)]
pub(crate) struct Node {
    pub next: NodeId,
    pub kind: Kind,
}
/// A detached node is an owner; NodeId remains a copyable reference only.
#[must_use = "append or release this detached node"]
#[derive(Debug)]
pub(crate) struct DetachedNode(NodeId);
impl DetachedNode {
    pub fn id(&self) -> NodeId {
        self.0
    }
    /// Consume this singleton owner in its matching store version.
    fn into_list(self) -> List {
        List {
            head: self.0,
            tail: self.0,
        }
    }
}
struct Chunk {
    nodes: [Node; CELLS],
    next: u16,
    free: u16,
    live: u16,
}
impl Chunk {
    fn room(&self) -> bool {
        self.next < NONE || self.free != NONE
    }
}
#[derive(Default)]
struct Pool {
    free: RefCell<Vec<Rc<Chunk>>>,
    failed_release: RefCell<ReleaseScratch>,
}
impl Pool {
    fn take(&self) -> Rc<Chunk> {
        if let Some(chunk) = self.free.borrow_mut().pop() {
            return chunk;
        }
        let mut chunk = Rc::<Chunk>::new_uninit();
        let pointer = Rc::get_mut(&mut chunk).unwrap().as_mut_ptr();
        // SAFETY: this exclusive, correctly aligned allocation is unpublished.
        // Initialize every field and array cell in its final destination before
        // assuming initialization. Nodes contain values and numeric links only.
        unsafe {
            let cells = std::ptr::addr_of_mut!((*pointer).nodes).cast::<Node>();
            for index in 0..CELLS {
                cells.add(index).write(Node::default());
            }
            std::ptr::addr_of_mut!((*pointer).next).write(0);
            std::ptr::addr_of_mut!((*pointer).free).write(NONE);
            std::ptr::addr_of_mut!((*pointer).live).write(0);
            chunk.assume_init()
        }
    }
}
struct Page {
    data: Option<Rc<Chunk>>,
    pool: Rc<Pool>,
}
impl Clone for Page {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            pool: self.pool.clone(),
        }
    }
}
impl Drop for Page {
    fn drop(&mut self) {
        let mut data = self.data.take().unwrap();
        if let Some(chunk) = Rc::get_mut(&mut data) {
            chunk.nodes.fill_with(Node::default);
            chunk.next = 0;
            chunk.free = NONE;
            chunk.live = 0;
            let mut free = self.pool.free.borrow_mut();
            if free.len() < CACHED {
                free.push(data);
            }
        }
    }
}
#[derive(Clone)]
pub(crate) struct NodeStore {
    pages: Buffer<Option<Page>>,
    pool: Rc<Pool>,
    allocation: Option<usize>,
    boxes: box_slots::Boxes,
    insertions: insertion_slots::Insertions,
    leaders: leader_slots::Leaders,
    #[cfg(test)]
    allocation_budget: Option<usize>,
}
impl Default for NodeStore {
    fn default() -> Self {
        Self {
            pages: Buffer::new(&Rc::default()),
            pool: Rc::default(),
            allocation: None,
            boxes: box_slots::Boxes::default(),
            insertions: insertion_slots::Insertions::default(),
            leaders: leader_slots::Leaders::default(),
            #[cfg(test)]
            allocation_budget: None,
        }
    }
}
#[derive(Default)]
pub(crate) struct ReleaseScratch {
    pending: Vec<NodeId>,
}
impl NodeStore {
    #[cfg(test)]
    pub(crate) fn box_slot_count(&self) -> usize {
        self.boxes.len()
    }
    #[cfg(test)]
    pub(crate) fn live_count(&self) -> usize {
        self.pages
            .iter()
            .flatten()
            .map(|page| usize::from(page.data.as_ref().unwrap().live))
            .sum()
    }
    /// Exercise the same consuming rejection path as an exhausted node address space.
    #[cfg(test)]
    pub(crate) fn reject_after_allocations(&mut self, successful: usize) {
        self.allocation_budget = Some(successful);
    }
    pub fn copy_list(&mut self, source: NodeId, scratch: &mut CopyScratch) -> Result<List> {
        copying::copy_list(self, source, scratch, (u32::MAX as usize) / CELLS)
    }
    fn page(&self, index: usize) -> &Rc<Chunk> {
        self.pages[index]
            .as_ref()
            .expect("node chunk")
            .data
            .as_ref()
            .unwrap()
    }
    fn writable(&mut self, index: usize) -> &mut Chunk {
        if Rc::strong_count(self.page(index)) != 1 {
            let mut copy = self.pool.take();
            let to = Rc::get_mut(&mut copy).unwrap();
            let from = self.page(index);
            for (to, from) in to.nodes.iter_mut().zip(&from.nodes) {
                *to = from.snapshot_copy();
            }
            to.next = from.next;
            to.free = from.free;
            to.live = from.live;
            self.pages[index].as_mut().unwrap().data = Some(copy);
        }
        Rc::get_mut(self.pages[index].as_mut().unwrap().data.as_mut().unwrap()).unwrap()
    }
    fn address(id: NodeId) -> (usize, usize) {
        assert!(!id.is_null(), "null node");
        let offset = (id.0 - 1) as usize;
        (offset / CELLS, offset % CELLS)
    }
    pub fn get(&self, id: NodeId) -> &Node {
        let (page, offset) = Self::address(id);
        let node = &self.page(page).nodes[offset];
        assert!(!matches!(node.kind, Kind::Free), "released node");
        node
    }
    fn get_mut(&mut self, id: NodeId) -> &mut Node {
        let (page, offset) = Self::address(id);
        let node = &mut self.writable(page).nodes[offset];
        assert!(!matches!(node.kind, Kind::Free), "released node");
        node
    }
    pub fn kind_mut(&mut self, id: NodeId) -> &mut Kind {
        &mut self.get_mut(id).kind
    }
    fn available(&self, index: usize) -> bool {
        self.pages[index].as_ref().is_some_and(|p| {
            let data = p.data.as_ref().unwrap();
            Rc::strong_count(data) == 1 && data.room()
        })
    }
    pub fn alloc(&mut self, kind: Kind) -> Result<DetachedNode> {
        self.alloc_with_limit(kind, (u32::MAX as usize) / CELLS)
    }
    fn alloc_with_limit(&mut self, kind: Kind, limit: usize) -> Result<DetachedNode> {
        assert!(!matches!(kind, Kind::Free));
        let page = match self.allocation_page(limit) {
            Ok(page) => page,
            Err(error) => {
                let pool = self.pool.clone();
                let mut scratch = pool.failed_release.borrow_mut();
                self.release_kind(kind, &mut scratch);
                return Err(error);
            }
        };
        Ok(self.install(page, kind))
    }
    pub fn alloc_box(&mut self, data: BoxData) -> Result<DetachedNode> {
        self.alloc_box_with_limit(data, (u32::MAX as usize) / CELLS)
    }
    fn alloc_box_with_limit(&mut self, data: BoxData, limit: usize) -> Result<DetachedNode> {
        let page = match self.allocation_page(limit) {
            Ok(page) => page,
            Err(error) => {
                let pool = self.pool.clone();
                let mut scratch = pool.failed_release.borrow_mut();
                self.release(data.list, &mut scratch);
                return Err(error);
            }
        };
        let id = self.boxes.alloc(data);
        Ok(self.install(page, Kind::Box(id)))
    }
    pub fn box_data(&self, id: &BoxId) -> &BoxData {
        self.boxes.get(id)
    }
    pub fn box_mut(&mut self, node: NodeId) -> &mut BoxData {
        let Kind::Box(id) = &self.get(node).kind else {
            panic!("not a box")
        };
        let id = id.0;
        self.boxes.get_mut(id)
    }
    fn allocation_page(&mut self, limit: usize) -> Result<usize> {
        #[cfg(test)]
        if let Some(budget) = &mut self.allocation_budget {
            if *budget == 0 {
                return Err(Error::Capacity);
            }
            *budget -= 1;
        }

        let page = if let Some(index) = self.allocation.filter(|&i| self.available(i)) {
            index
        } else if let Some(index) = (0..self.pages.len()).find(|&i| self.available(i)) {
            index
        } else {
            let index = self
                .pages
                .iter()
                .position(Option::is_none)
                .unwrap_or(self.pages.len());
            if index >= limit {
                return Err(Error::Capacity);
            }
            let entry = Some(Page {
                data: Some(self.pool.take()),
                pool: self.pool.clone(),
            });
            if index == self.pages.len() {
                self.pages.push(entry);
            } else {
                self.pages[index] = entry;
            }
            index
        };
        Ok(page)
    }
    fn install(&mut self, page: usize, kind: Kind) -> DetachedNode {
        self.allocation = Some(page);
        let chunk = self.writable(page);
        let offset = if chunk.free != NONE {
            let offset = chunk.free;
            chunk.free = chunk.nodes[usize::from(offset)].next.0 as u16;
            offset
        } else {
            let offset = chunk.next;
            chunk.next += 1;
            offset
        };
        chunk.nodes[usize::from(offset)] = Node {
            next: NodeId::NULL,
            kind,
        };
        chunk.live += 1;
        DetachedNode(NodeId((page * CELLS + usize::from(offset) + 1) as u32))
    }
    fn free(&mut self, id: NodeId) -> Node {
        let (page, offset) = Self::address(id);
        let chunk = self.writable(page);
        let node = std::mem::replace(
            &mut chunk.nodes[offset],
            Node {
                kind: Kind::Free,
                next: NodeId(u32::from(chunk.free)),
            },
        );
        assert!(!matches!(node.kind, Kind::Free));
        chunk.free = offset as u16;
        chunk.live -= 1;
        if chunk.live == 0 {
            self.pages[page] = None;
            if self.allocation == Some(page) {
                self.allocation = None;
            }
        } else {
            self.allocation = Some(page);
        }
        node
    }
    pub fn release_node(&mut self, node: DetachedNode, scratch: &mut ReleaseScratch) {
        self.release(node.into_list(), scratch);
    }
    fn release_kind(&mut self, kind: Kind, scratch: &mut ReleaseScratch) {
        match kind {
            Kind::Box(id) | Kind::Unset(id) => {
                let data = self.boxes.take(id);
                self.release(data.list, scratch);
            }
            Kind::Insertion(id) => {
                let data = self.insertions.take(id);
                self.release(data.list, scratch);
            }
            Kind::Leader(id) => {
                let data = self.leaders.take(id);
                self.release(data.list, scratch);
            }
            Kind::Adjustment(list) => self.release(list, scratch),
            Kind::Ligature { components, .. } => self.release(components, scratch),
            Kind::Discretionary { pre, post, .. } => {
                self.release(pre, scratch);
                self.release(post, scratch);
            }
            _ => {}
        }
    }
    pub fn release(&mut self, list: List, scratch: &mut ReleaseScratch) {
        assert!(scratch.pending.is_empty());
        scratch.pending.push(list.head);
        while let Some(mut id) = scratch.pending.pop() {
            while !id.is_null() {
                let node = self.free(id);
                match node.kind {
                    Kind::Box(id) | Kind::Unset(id) => {
                        let data = self.boxes.take(id);
                        scratch.pending.push(data.list.head);
                    }
                    Kind::Insertion(id) => {
                        let data = self.insertions.take(id);
                        scratch.pending.push(data.list.head);
                    }
                    Kind::Leader(id) => {
                        let data = self.leaders.take(id);
                        scratch.pending.push(data.list.head);
                    }
                    Kind::Adjustment(list) => scratch.pending.push(list.head),
                    Kind::Ligature { components, .. } => scratch.pending.push(components.head),
                    Kind::Discretionary { pre, post, .. } => {
                        scratch.pending.push(pre.head);
                        scratch.pending.push(post.head);
                    }
                    _ => {}
                }
                id = node.next;
            }
        }
    }
}

#[cfg(test)]
#[path = "nodes_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "node_insertion_tests.rs"]
mod insertion_tests;

#[cfg(test)]
#[path = "node_leader_tests.rs"]
mod leader_tests;
