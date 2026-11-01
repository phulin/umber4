//! Narrow construction operations preserve affine fields and stable arena IDs.
use super::*;

#[derive(Clone, Copy)]
pub(crate) enum FieldSlot {
    Nucleus,
    Sub,
    Sup,
}
#[cfg(test)]
#[path = "math_delimiter_ownership_tests.rs"]
mod delimiter_tests;
#[derive(Clone, Copy)]
pub(crate) struct FieldTarget {
    pub atom: MathId,
    pub slot: FieldSlot,
}
impl FieldSlot {
    fn field(self, fields: &mut Fields) -> &mut Field {
        match self {
            Self::Nucleus => &mut fields.nucleus,
            Self::Sub => &mut fields.sub,
            Self::Sup => &mut fields.sup,
        }
    }
}
impl MathStore {
    pub fn set_boundary_delimiter(&mut self, id: MathId, value: Delimiter) {
        let Kind::Boundary { delimiter, .. } = &mut self.node_mut(id).kind else {
            unreachable!("boundary operand target")
        };
        *delimiter = value;
    }
    pub fn close_delimited(&mut self, mut list: MathList, right: MathList) -> MathList {
        debug_assert!(matches!(
            self.node(list.head).kind,
            Kind::Boundary {
                side: Side::Left,
                ..
            }
        ));
        debug_assert_eq!(right.head, right.tail);
        debug_assert!(matches!(
            self.node(right.head).kind,
            Kind::Boundary {
                side: Side::Right,
                ..
            }
        ));
        let tail = list.tail;
        self.insert_after(&mut list, tail, right);
        list
    }
    pub fn atom_class(&self, id: MathId) -> Option<Class> {
        match &self.node(id).kind {
            Kind::Atom(a) => Some(a.class),
            _ => None,
        }
    }
    pub fn field_empty(&mut self, target: FieldTarget) -> bool {
        match &mut self.node_mut(target.atom).kind {
            Kind::Atom(a) => target.slot.field(&mut a.fields).is_empty(),
            _ => false,
        }
    }
    pub fn fill_field(
        &mut self,
        target: FieldTarget,
        field: Field,
        nodes: &mut NodeStore,
    ) -> Result<()> {
        if let Kind::Atom(a) = &mut self.node_mut(target.atom).kind {
            let destination = target.slot.field(&mut a.fields);
            if destination.is_empty() {
                *destination = field;
                return Ok(());
            }
        }
        self.release_field(field, nodes);
        self.drain_release(nodes);
        Err(Error::Syntax)
    }
    pub fn set_limits(&mut self, id: MathId, limits: Limits) -> Result<()> {
        match &mut self.node_mut(id).kind {
            Kind::Atom(a) if a.class == Class::Op => {
                a.limits = limits;
                Ok(())
            }
            _ => Err(Error::Syntax),
        }
    }
    pub fn field_from_list(&mut self, mut list: MathList) -> Field {
        if !list.is_empty()
            && list.head == list.tail
            && let Kind::Atom(a) = &self.node(list.head).kind
            && a.class == Class::Ord
            && a.fields.sub.is_empty()
            && a.fields.sup.is_empty()
        {
            let Some(Kind::Atom(a)) = self.take_front(&mut list) else {
                unreachable!()
            };
            return a.fields.nucleus;
        }
        Field::SubMath(list)
    }
    /// Only for abandoning the complete matching ordinary-node root on restore.
    /// Numeric child owners are inert; never traverse them in restored storage.
    pub fn abandon(&mut self) {
        self.cells.clear();
        self.pending.clear();
        self.free = MathId::NULL;
        self.live = 0;
    }
    #[cfg(test)]
    pub(crate) fn live_count(&self) -> usize {
        self.live
    }
}
