//! Affine temporary math ownership; no active math arena is checkpoint state.
use crate::{
    Error, Result,
    nodes::{DetachedNode, List, NodeStore, ReleaseScratch},
    value_metadata::Buffer,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct MathId(u32);
impl MathId {
    const NULL: Self = Self(0);
}
#[must_use = "convert, transfer, or release this math list"]
#[derive(Debug, Default)]
pub(crate) struct MathList {
    head: MathId,
    tail: MathId,
}
impl MathList {
    pub fn is_empty(&self) -> bool {
        self.head == MathId::NULL
    }
    pub fn tail(&self) -> Option<MathId> {
        (!self.is_empty()).then_some(self.tail)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Class {
    Ord,
    Op,
    Bin,
    Rel,
    Open,
    Close,
    Punct,
    Inner,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Limits {
    #[default]
    Normal,
    AboveBelow,
    Side,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Style(u8);
impl Style {
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "display initial style awaits the math frontend")
    )]
    pub const DISPLAY: Self = Self(0);
    pub const TEXT: Self = Self(2);
    pub fn new(code: u8) -> Option<Self> {
        (code < 8).then_some(Self(code))
    }
    pub fn size(self) -> usize {
        if self.0 < 4 {
            0
        } else {
            usize::from((self.0 - 2) / 2)
        }
    }
    pub fn sup(self) -> Self {
        Self(2 * (self.0 / 4) + 4 + self.0 % 2)
    }
    pub fn sub(self) -> Self {
        Self(2 * (self.0 / 4) + 5)
    }
    pub fn cramp(self) -> Self {
        Self(2 * (self.0 / 2) + 1)
    }
    pub fn numerator(self) -> Self {
        Self(self.0 + 2 - 2 * (self.0 / 6))
    }
    pub fn denominator(self) -> Self {
        Self(2 * (self.0 / 2) + 3 - 2 * (self.0 / 6))
    }
    pub fn choice(self) -> usize {
        usize::from(self.0 / 2)
    }
    pub fn script(self) -> bool {
        self.0 >= 4
    }
    pub fn display(self) -> bool {
        self.0 < 2
    }
    pub fn cramped(self) -> bool {
        self.0 % 2 == 1
    }
}
#[derive(Debug, Default)]
pub(crate) enum Field {
    #[default]
    Empty,
    Character {
        family: u8,
        character: u8,
    },
    TextCharacter {
        family: u8,
        character: u8,
    },
    Box(DetachedNode),
    SubMath(MathList),
}
impl Field {
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }
}
#[derive(Debug, Default)]
pub(crate) struct Fields {
    pub nucleus: Field,
    pub sub: Field,
    pub sup: Field,
}
impl Fields {
    pub fn new(nucleus: Field) -> Self {
        Self {
            nucleus,
            ..Self::default()
        }
    }
}
#[derive(Debug)]
pub(crate) struct Atom {
    pub class: Class,
    pub limits: Limits,
    pub fields: Fields,
}
impl Atom {
    pub fn new(class: Class, nucleus: Field) -> Self {
        Self {
            class,
            limits: Limits::Normal,
            fields: Fields::new(nucleus),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct MathChar {
    pub family: u8,
    pub character: u8,
}
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Delimiter {
    pub small_family: u8,
    pub small_char: u8,
    pub large_family: u8,
    pub large_char: u8,
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum Side {
    Left,
    Right,
}
#[derive(Clone, Copy, Debug, Default)]
pub(crate) enum RuleThickness {
    #[default]
    Default,
    Explicit(crate::numeric::Scaled),
}
#[derive(Debug, Default)]
pub(crate) struct Fraction {
    pub numerator: MathList,
    pub denominator: MathList,
    pub left: Delimiter,
    pub right: Delimiter,
    pub thickness: RuleThickness,
}
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "style, choice, and explicit material construction awaits the math frontend"
    )
)]
#[derive(Debug, Default)]
pub(crate) enum Kind {
    #[default]
    Free,
    Atom(Atom),
    Fraction(Fraction),
    Over(Fields),
    Under(Fields),
    VCenter(Fields),
    Radical {
        fields: Fields,
        delimiter: Delimiter,
    },
    Accent {
        accent: MathChar,
        fields: Fields,
    },
    Boundary {
        side: Side,
        delimiter: Delimiter,
    },
    Style(Style),
    Choice([MathList; 4]),
    MuKern(crate::numeric::Scaled),
    Material(DetachedNode),
    Working(Class),
    Converted {
        class: Class,
        output: List,
    },
}
#[path = "math_frontend_store.rs"]
mod frontend_store;
pub(crate) use frontend_store::{FieldSlot, FieldTarget};
impl Kind {
    fn class(&self) -> Option<Class> {
        match self {
            Self::Atom(a) => Some(a.class),
            Self::Fraction(_)
            | Self::Over(_)
            | Self::Under(_)
            | Self::VCenter(_)
            | Self::Radical { .. }
            | Self::Accent { .. } => Some(Class::Ord),
            Self::Boundary {
                side: Side::Left, ..
            } => Some(Class::Open),
            Self::Boundary {
                side: Side::Right, ..
            } => Some(Class::Close),
            Self::Working(c) | Self::Converted { class: c, .. } => Some(*c),
            _ => None,
        }
    }
    fn set_class(&mut self, class: Class) {
        match self {
            Self::Atom(a) => a.class = class,
            Self::Converted { class: c, .. } => *c = class,
            _ => unreachable!("classed math node"),
        }
    }
}
#[derive(Debug, Default)]
struct Node {
    next: MathId,
    kind: Kind,
}
pub(crate) struct MathStore {
    cells: Buffer<Node>,
    free: MathId,
    live: usize,
    pending: Buffer<MathId>,
    node_release: ReleaseScratch,
    #[cfg(test)]
    limit: u32,
}
impl Default for MathStore {
    fn default() -> Self {
        Self {
            cells: Buffer::new(&std::rc::Rc::default()),
            free: MathId::NULL,
            live: 0,
            pending: Buffer::new(&std::rc::Rc::default()),
            node_release: ReleaseScratch::default(),
            #[cfg(test)]
            limit: u32::MAX,
        }
    }
}
impl MathStore {
    fn node(&self, id: MathId) -> &Node {
        assert_ne!(id, MathId::NULL);
        let n = &self.cells[id.0 as usize - 1];
        assert!(!matches!(n.kind, Kind::Free));
        n
    }
    fn node_mut(&mut self, id: MathId) -> &mut Node {
        assert_ne!(id, MathId::NULL);
        let n = &mut self.cells[id.0 as usize - 1];
        assert!(!matches!(n.kind, Kind::Free));
        n
    }
    pub fn append(
        &mut self,
        list: &mut MathList,
        kind: Kind,
        nodes: &mut NodeStore,
    ) -> Result<MathId> {
        let id = self.allocate(kind, nodes)?;
        if list.is_empty() {
            list.head = id;
        } else {
            self.node_mut(list.tail).next = id;
        }
        list.tail = id;
        Ok(id)
    }
    fn allocate(&mut self, kind: Kind, nodes: &mut NodeStore) -> Result<MathId> {
        assert!(!matches!(kind, Kind::Free));
        let limit = {
            #[cfg(test)]
            {
                self.limit
            }
            #[cfg(not(test))]
            {
                u32::MAX
            }
        };
        let id = if self.free != MathId::NULL {
            let id = self.free;
            self.free = self.cells[id.0 as usize - 1].next;
            id
        } else if self.cells.len() < limit as usize {
            self.cells.push(Node::default());
            MathId(self.cells.len() as u32)
        } else {
            self.release_kind(kind, nodes);
            return Err(Error::Capacity);
        };
        self.cells[id.0 as usize - 1] = Node {
            next: MathId::NULL,
            kind,
        };
        self.live += 1;
        Ok(id)
    }
    fn free(&mut self, id: MathId) -> Node {
        let old = std::mem::replace(
            &mut self.cells[id.0 as usize - 1],
            Node {
                next: self.free,
                kind: Kind::Free,
            },
        );
        assert!(!matches!(old.kind, Kind::Free));
        self.free = id;
        self.live -= 1;
        old
    }
    fn insert_after(&mut self, list: &mut MathList, at: MathId, inserted: MathList) {
        if inserted.is_empty() {
            return;
        }
        let next = self.node(at).next;
        self.node_mut(at).next = inserted.head;
        self.node_mut(inserted.tail).next = next;
        if list.tail == at {
            list.tail = inserted.tail;
        }
    }
    fn take_front(&mut self, list: &mut MathList) -> Option<Kind> {
        if list.is_empty() {
            return None;
        }
        let node = self.free(list.head);
        list.head = node.next;
        if list.is_empty() {
            list.tail = MathId::NULL;
        }
        Some(node.kind)
    }
    fn release_field(&mut self, field: Field, nodes: &mut NodeStore) {
        match field {
            Field::Box(b) => nodes.release_node(b, &mut self.node_release),
            Field::SubMath(l) => self.pending.push(l.head),
            _ => {}
        }
    }
    fn release_fields(&mut self, fields: Fields, nodes: &mut NodeStore) {
        self.release_field(fields.nucleus, nodes);
        self.release_field(fields.sub, nodes);
        self.release_field(fields.sup, nodes);
    }
    fn release_children(&mut self, kind: Kind, nodes: &mut NodeStore) {
        match kind {
            Kind::Atom(a) => self.release_fields(a.fields, nodes),
            Kind::Over(fields)
            | Kind::Under(fields)
            | Kind::VCenter(fields)
            | Kind::Radical { fields, .. }
            | Kind::Accent { fields, .. } => self.release_fields(fields, nodes),
            Kind::Fraction(f) => {
                self.pending.push(f.numerator.head);
                self.pending.push(f.denominator.head);
            }
            Kind::Choice(lists) => {
                for l in lists {
                    self.pending.push(l.head);
                }
            }
            Kind::Material(n) => nodes.release_node(n, &mut self.node_release),
            Kind::Converted { output, .. } => nodes.release(output, &mut self.node_release),
            _ => {}
        }
    }
    fn drain_release(&mut self, nodes: &mut NodeStore) {
        while let Some(mut id) = self.pending.pop() {
            while id != MathId::NULL {
                let n = self.free(id);
                id = n.next;
                self.release_children(n.kind, nodes);
            }
        }
    }
    fn release_kind(&mut self, kind: Kind, nodes: &mut NodeStore) {
        self.release_children(kind, nodes);
        self.drain_release(nodes);
    }
    pub fn release(&mut self, list: MathList, nodes: &mut NodeStore) {
        assert!(self.pending.is_empty());
        self.pending.push(list.head);
        self.drain_release(nodes);
    }
}
#[path = "math_convert.rs"]
mod converting;
pub(crate) use converting::Scratch;
#[cfg(test)]
#[path = "math_oracle_tests.rs"]
mod oracle_tests;
#[cfg(test)]
#[path = "math_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "math_forms_tests.rs"]
mod forms_tests;

#[cfg(test)]
#[path = "math_accent_tests.rs"]
mod accent_tests;
