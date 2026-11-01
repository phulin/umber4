//! Interleaved TeX first pass and spacing/penalty second pass.
use super::*;
impl Conversion<'_, '_, '_> {
    pub(super) fn list_step(&mut self, w: &mut ListWork) -> Result<Action> {
        match w.pass {
            Pass::Atom => {
                let mut output = self.result().list();
                if w.delimited {
                    let b = self.engine.hpack(output, PackSpec::Natural)?;
                    let b = self.engine.document.nodes.take_box(b);
                    w.max_height = w.max_height.max(b.height.0);
                    w.max_depth = w.max_depth.max(b.depth.0);
                    output = b.list;
                }
                let class = self.math.node(w.cursor).kind.class().unwrap();
                self.math.node_mut(w.cursor).kind = Kind::Converted { class, output };
                w.previous = w.cursor;
                w.previous_class = Some(class);
                w.cursor = self.math.node(w.cursor).next;
                w.pass = Pass::First;
            }
            Pass::First => {
                if w.cursor == MathId::NULL {
                    if w.delimited && !w.closed {
                        return Err(Error::Syntax);
                    }
                    self.cancel_binary(w.previous, w.previous_class);
                    w.pass = Pass::Second;
                    w.style = w.initial;
                    w.previous_class = None;
                } else {
                    let q = w.cursor;
                    match &self.math.node(q).kind {
                        Kind::Style(style) => {
                            w.style = *style;
                            w.cursor = self.math.node(q).next;
                        }
                        Kind::Choice(_) => {
                            let Kind::Choice(mut choices) = std::mem::replace(
                                &mut self.math.node_mut(q).kind,
                                Kind::Style(w.style),
                            ) else {
                                unreachable!()
                            };
                            let selected = std::mem::take(&mut choices[w.style.choice()]);
                            for l in choices {
                                self.math.release(l, &mut self.engine.document.nodes);
                            }
                            self.math.insert_after(&mut w.list, q, selected);
                            w.cursor = self.math.node(q).next;
                        }
                        Kind::Material(n) => {
                            if !matches!(
                                self.engine.document.nodes.get(n.id()).kind,
                                nodes::Kind::Kern { .. }
                                    | nodes::Kind::Glue { .. }
                                    | nodes::Kind::Rule { .. }
                                    | nodes::Kind::Penalty(_)
                            ) {
                                return Err(Error::Unsupported);
                            }
                            if w.delimited
                                && let nodes::Kind::Rule { height, depth, .. } =
                                    self.engine.document.nodes.get(n.id()).kind
                            {
                                w.max_height = w.max_height.max(height.0);
                                w.max_depth = w.max_depth.max(depth.0);
                            }
                            w.cursor = self.math.node(q).next;
                        }
                        Kind::Boundary { side, .. } => {
                            match side {
                                Side::Left => {
                                    if q != w.list.head || w.delimited {
                                        return Err(Error::Syntax);
                                    }
                                    w.delimited = true;
                                }
                                Side::Right => {
                                    if !w.delimited || w.closed || q != w.list.tail {
                                        return Err(Error::Syntax);
                                    }
                                    self.cancel_binary(w.previous, w.previous_class);
                                    w.closed = true;
                                    w.style = w.initial;
                                }
                            }
                            w.previous = q;
                            w.previous_class = Some(Class::Open);
                            w.cursor = self.math.node(q).next;
                        }
                        Kind::Accent { .. } => {
                            let Kind::Accent { accent, fields } = std::mem::replace(
                                &mut self.math.node_mut(q).kind,
                                Kind::Working(Class::Ord),
                            ) else {
                                unreachable!()
                            };
                            w.pass = Pass::Atom;
                            return Ok(Action::Child(accents::task(accent, fields, w.style)));
                        }
                        Kind::Fraction(_)
                        | Kind::Over(_)
                        | Kind::Under(_)
                        | Kind::Radical { .. }
                        | Kind::VCenter(_) => {
                            let kind = std::mem::replace(
                                &mut self.math.node_mut(q).kind,
                                Kind::Working(Class::Ord),
                            );
                            let child = match kind {
                                Kind::Fraction(f) => fraction::task(f, w.style),
                                other => forms::task(other, w.style),
                            };
                            w.pass = Pass::Atom;
                            return Ok(Action::Child(child));
                        }
                        Kind::Atom(a) => {
                            let mut class = a.class;
                            if class == Class::Bin
                                && matches!(
                                    w.previous_class,
                                    Some(
                                        Class::Bin
                                            | Class::Op
                                            | Class::Rel
                                            | Class::Open
                                            | Class::Punct
                                    )
                                )
                            {
                                class = Class::Ord;
                                self.math.node_mut(q).kind.set_class(class);
                            }
                            if matches!(class, Class::Rel | Class::Close | Class::Punct) {
                                self.cancel_binary(w.previous, w.previous_class);
                            }
                            if class == Class::Ord {
                                self.make_ord(q, w.style, &mut w.list)?;
                            }
                            let Kind::Atom(atom) = std::mem::replace(
                                &mut self.math.node_mut(q).kind,
                                Kind::Working(class),
                            ) else {
                                unreachable!()
                            };
                            w.pass = Pass::Atom;
                            return Ok(Action::Child(Task::Atom(AtomWork::new(atom, w.style))));
                        }
                        _ => unreachable!("unconverted first pass"),
                    }
                }
            }
            Pass::Second => {
                let Some(kind) = self.math.take_front(&mut w.list) else {
                    return Ok(Action::Done(Value::List(std::mem::take(&mut w.output))));
                };
                match kind {
                    Kind::Style(s) => w.style = s,
                    Kind::Material(n) => self.engine.document.nodes.append(&mut w.output, n),
                    Kind::Boundary { side, delimiter } => {
                        w.style = w.initial;
                        let axis = self.symbol(w.initial, 22);
                        let delta2 = w.max_depth.wrapping_add(axis);
                        let delta1 = w
                            .max_height
                            .wrapping_add(w.max_depth)
                            .wrapping_sub(delta2)
                            .max(delta2);
                        let factor = self.engine.document.scalars[Target::parameter(33)].value;
                        let shortfall = self.engine.document.layout_values.dimens
                            [crate::layout_values::DimenParam::DelimiterShortfall.index()]
                        .value
                        .0;
                        let target = (delta1 / 500)
                            .wrapping_mul(factor)
                            .max(delta1.wrapping_add(delta1).wrapping_sub(shortfall));
                        let b = self.delimiter(delimiter, w.initial, target)?;
                        self.engine.document.nodes.append(&mut w.pending, b);
                        let class = match side {
                            Side::Left => Class::Open,
                            Side::Right => Class::Close,
                        };
                        self.append_noad(w, class)?;
                        if matches!(side, Side::Right) {
                            w.previous_class = Some(Class::Open);
                        }
                    }
                    Kind::Converted { class, output } => {
                        w.pending = output;
                        self.append_noad(w, class)?;
                    }
                    _ => unreachable!("translated second pass"),
                }
            }
        }
        Ok(Action::Again)
    }
    fn append_noad(&mut self, w: &mut ListWork, class: Class) -> Result<()> {
        if let Some(previous) = w.previous_class {
            self.spacing(&mut w.output, previous, class, w.style)?;
        }
        self.engine
            .document
            .nodes
            .splice(&mut w.output, std::mem::take(&mut w.pending));
        let pen = match class {
            Class::Bin => Some(26),
            Class::Rel => Some(27),
            _ => None,
        };
        if w.penalties
            && !w.list.is_empty()
            && let Some(index) = pen
        {
            let next = &self.math.node(w.list.head).kind;
            let suppress = next.class() == Some(Class::Rel)
                || matches!(next, Kind::Material(n) if matches!(self.engine.document.nodes.get(n.id()).kind, nodes::Kind::Penalty(_)));
            let penalty = self.engine.document.scalars[Target::parameter(index)].value;
            if !suppress && penalty < 10000 {
                self.append(&mut w.output, nodes::Kind::Penalty(penalty))?;
            }
        }
        w.previous_class = Some(class);
        Ok(())
    }
}
