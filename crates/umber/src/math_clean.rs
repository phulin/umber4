//! TeX clean_box: retain width while removing a trivial italic kern.
use super::*;
impl Conversion<'_, '_, '_> {
    pub(super) fn clean_step(&mut self, w: &mut CleanWork) -> Result<Action> {
        if w.waiting {
            let list = self.result().list();
            let b = self.clean_list(list)?;
            return Ok(Action::Done(Value::Box(b)));
        }
        let field = std::mem::take(&mut w.field);
        let b = match field {
            Field::SubMath(list) => {
                w.waiting = true;
                return Ok(Action::Child(Task::List(ListWork::new(
                    list, w.style, false,
                ))));
            }
            Field::Box(b) => {
                let mut list = List::default();
                self.engine.document.nodes.append(&mut list, b);
                self.clean_list(list)?
            }
            Field::Character { family, character } => {
                let mut output = List::default();
                let result = (|| {
                    if let Some((font, m)) = self.glyph(family, character, w.style)? {
                        self.append(&mut output, nodes::Kind::Character { font, character })?;
                        if m.italic.0 != 0 {
                            self.kern(&mut output, m.italic.0)?;
                        }
                    }
                    self.engine
                        .hpack(std::mem::take(&mut output), PackSpec::Natural)
                })();
                self.release_value(Value::List(output));
                result?
            }
            Field::Empty | Field::TextCharacter { .. } => {
                self.engine.hpack(List::default(), PackSpec::Natural)?
            }
        };
        self.simplify_box(b.id());
        Ok(Action::Done(Value::Box(b)))
    }
    pub(super) fn clean_list(&mut self, mut list: List) -> Result<DetachedNode> {
        let b = if list.head() == list.tail()
            && !list.is_empty()
            && matches!(&self.engine.document.nodes.get(list.head()).kind, nodes::Kind::Box(id) if self.engine.document.nodes.box_data(id).shift == Scaled::ZERO)
        {
            self.engine.document.nodes.pop_front(&mut list).unwrap()
        } else {
            self.engine.hpack(list, PackSpec::Natural)?
        };
        self.simplify_box(b.id());
        Ok(b)
    }
    fn simplify_box(&mut self, id: nodes::NodeId) {
        let (first, tail) = {
            let b = self.engine.document.nodes.box_mut(id);
            (b.list.head(), b.list.tail())
        };
        if !first.is_null()
            && first != tail
            && matches!(
                self.engine.document.nodes.get(first).kind,
                nodes::Kind::Character { .. }
            )
            && self.engine.document.nodes.get(first).next == tail
            && matches!(
                self.engine.document.nodes.get(tail).kind,
                nodes::Kind::Kern { .. }
            )
        {
            let mut contents = std::mem::take(&mut self.engine.document.nodes.box_mut(id).list);
            let suffix = self.engine.document.nodes.split_after(&mut contents, first);
            self.engine.document.nodes.box_mut(id).list = contents;
            self.release_value(Value::List(suffix));
        }
    }
}
