//! Narrow construction operations preserve affine fields and stable arena IDs.
use super::*;

#[derive(Clone, Copy)]
pub(crate) enum FieldSlot {
    Nucleus,
    Sub,
    Sup,
}
#[cfg(test)]
#[path = "math_choice_ownership_tests.rs"]
mod choice_tests;
#[cfg(test)]
#[path = "math_delimiter_ownership_tests.rs"]
mod delimiter_tests;
#[cfg(test)]
#[path = "math_forms_frontend_ownership_tests.rs"]
mod form_tests;
#[cfg(test)]
#[path = "math_fraction_ownership_tests.rs"]
mod fraction_tests;
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
    pub fn finish_choice_branch(&mut self, id: MathId, branch: u8, list: MathList) {
        let Kind::Choice(choices) = &mut self.node_mut(id).kind else {
            unreachable!("choice branch destination")
        };
        let destination = &mut choices[branch as usize];
        debug_assert!(destination.is_empty());
        *destination = list;
    }
    pub fn start_fraction(
        &mut self,
        current: &mut MathList,
        nodes: &mut NodeStore,
    ) -> Result<MathList> {
        let mut incomplete = MathList::default();
        let id = self.append(&mut incomplete, Kind::Fraction(Fraction::default()), nodes)?;
        // Rejection leaves the existing numerator untouched. All ownership
        // transfers after successful allocation are infallible.
        let Kind::Fraction(fraction) = &mut self.node_mut(id).kind else {
            unreachable!()
        };
        fraction.numerator = std::mem::take(current);
        Ok(incomplete)
    }
    pub fn set_boundary_delimiter(&mut self, id: MathId, value: Delimiter) {
        let Kind::Boundary { delimiter, .. } = &mut self.node_mut(id).kind else {
            unreachable!("boundary operand target")
        };
        *delimiter = value;
    }
    pub fn set_fraction_parameters(
        &mut self,
        id: MathId,
        left: Delimiter,
        right: Delimiter,
        thickness: RuleThickness,
    ) {
        let Kind::Fraction(fraction) = &mut self.node_mut(id).kind else {
            unreachable!("incomplete fraction operand target")
        };
        fraction.left = left;
        fraction.right = right;
        fraction.thickness = thickness;
    }
    /// Transfer the three disjoint construction owners without allocating.
    pub fn finish_list(
        &mut self,
        list: MathList,
        incomplete: Option<MathList>,
        right: Option<MathList>,
    ) -> MathList {
        let mut output = if let Some(fraction) = incomplete {
            debug_assert_eq!(fraction.head, fraction.tail);
            let numerator = {
                let Kind::Fraction(data) = &mut self.node_mut(fraction.head).kind else {
                    unreachable!("incomplete fraction owner")
                };
                debug_assert!(data.denominator.is_empty());
                data.denominator = list;
                right.as_ref().map(|_| std::mem::take(&mut data.numerator))
            };
            if let Some(mut numerator) = numerator {
                let left = numerator.head;
                debug_assert!(matches!(
                    self.node(left).kind,
                    Kind::Boundary {
                        side: Side::Left,
                        ..
                    }
                ));
                numerator.head = self.node(left).next;
                if numerator.head == MathId::NULL {
                    numerator.tail = MathId::NULL;
                }
                self.node_mut(left).next = MathId::NULL;
                let Kind::Fraction(data) = &mut self.node_mut(fraction.head).kind else {
                    unreachable!()
                };
                data.numerator = numerator;
                let mut output = MathList {
                    head: left,
                    tail: left,
                };
                self.insert_after(&mut output, left, fraction);
                output
            } else {
                fraction
            }
        } else {
            list
        };
        if let Some(right) = right {
            debug_assert!(matches!(
                self.node(output.head).kind,
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
            let tail = output.tail;
            self.insert_after(&mut output, tail, right);
        }
        output
    }
    pub fn scripts_allowed(&self, id: MathId) -> bool {
        matches!(
            self.node(id).kind,
            Kind::Atom(_)
                | Kind::Over(_)
                | Kind::Under(_)
                | Kind::Radical { .. }
                | Kind::Accent { .. }
                | Kind::VCenter(_)
        )
    }
    fn fields_mut(&mut self, id: MathId) -> Option<&mut Fields> {
        match &mut self.node_mut(id).kind {
            Kind::Atom(a) => Some(&mut a.fields),
            Kind::Over(fields)
            | Kind::Under(fields)
            | Kind::Radical { fields, .. }
            | Kind::Accent { fields, .. }
            | Kind::VCenter(fields) => Some(fields),
            _ => None,
        }
    }
    pub fn set_radical_delimiter(&mut self, id: MathId, value: Delimiter) {
        let Kind::Radical { delimiter, .. } = &mut self.node_mut(id).kind else {
            unreachable!("radical operand target")
        };
        *delimiter = value;
    }
    pub fn set_math_accent(&mut self, id: MathId, value: MathChar) {
        let Kind::Accent { accent, .. } = &mut self.node_mut(id).kind else {
            unreachable!("accent selector target")
        };
        *accent = value;
    }
    pub fn field_empty(&mut self, target: FieldTarget) -> bool {
        self.fields_mut(target.atom)
            .is_some_and(|fields| target.slot.field(fields).is_empty())
    }
    pub fn fill_field(
        &mut self,
        target: FieldTarget,
        field: Field,
        nodes: &mut NodeStore,
    ) -> Result<()> {
        if let Some(fields) = self.fields_mut(target.atom) {
            let destination = target.slot.field(fields);
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
    pub fn finish_field(
        &mut self,
        target: FieldTarget,
        mut list: MathList,
        tail: Option<MathId>,
        nodes: &mut NodeStore,
    ) -> Result<()> {
        if !list.is_empty()
            && list.head == list.tail
            && matches!(self.node(list.head).kind, Kind::Accent { .. })
            && matches!(target.slot, FieldSlot::Nucleus)
            && tail == Some(target.atom)
            && matches!(&self.node(target.atom).kind,Kind::Atom(a) if a.class==Class::Ord)
        {
            let replacement = self.take_front(&mut list).unwrap();
            let Kind::Atom(old) =
                std::mem::replace(&mut self.node_mut(target.atom).kind, replacement)
            else {
                unreachable!()
            };
            debug_assert!(
                old.fields.nucleus.is_empty()
                    && old.fields.sub.is_empty()
                    && old.fields.sup.is_empty()
            );
            return Ok(());
        }
        let field = self.field_from_list(list);
        self.fill_field(target, field, nodes)
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
