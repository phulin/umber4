//! Ordinary ligatures, nucleus continuations, scripts, and displayed limits.
use super::*;
use crate::nodes::KernKind;
impl Conversion<'_, '_, '_> {
    pub(super) fn make_ord(&mut self, q: MathId, style: Style, list: &mut MathList) -> Result<()> {
        loop {
            let (family, left, p) = {
                let n = self.math.node(q);
                let Kind::Atom(a) = &n.kind else {
                    unreachable!()
                };
                if !a.fields.sub.is_empty() || !a.fields.sup.is_empty() {
                    return Ok(());
                }
                let Field::Character { family, character } = a.fields.nucleus else {
                    return Ok(());
                };
                (family, character, n.next)
            };
            if p == MathId::NULL {
                return Ok(());
            }
            let right = match &self.math.node(p).kind {
                Kind::Atom(a) if a.class as u8 <= Class::Punct as u8 => match a.fields.nucleus {
                    Field::Character {
                        family: f,
                        character,
                    } if f == family => character,
                    _ => return Ok(()),
                },
                _ => return Ok(()),
            };
            let Kind::Atom(a) = &mut self.math.node_mut(q).kind else {
                unreachable!()
            };
            a.fields.nucleus = Field::TextCharacter {
                family,
                character: left,
            };
            let Some((font, _)) = self.glyph(family, left, style)? else {
                let Kind::Atom(a) = &mut self.math.node_mut(q).kind else {
                    unreachable!()
                };
                a.fields.nucleus = Field::Empty;
                return Ok(());
            };
            let scaled = self.descriptor(font).scaled.unwrap();
            let instruction = self.engine.font_catalog.scaled[scaled]
                .source()
                .lig_instruction(left as u16, right as u16, None);
            let Some([_, _, op, rem]) = instruction else {
                return Ok(());
            };
            if op >= 128 {
                let width = self.engine.font_catalog.scaled[scaled]
                    .descriptor()
                    .kern(256 * (op as usize - 128) + rem as usize);
                let node = self.engine.document.nodes.alloc(nodes::Kind::Kern {
                    width,
                    kind: KernKind::Normal,
                })?;
                let mut inserted = MathList::default();
                self.math.append(
                    &mut inserted,
                    Kind::Material(node),
                    &mut self.engine.document.nodes,
                )?;
                self.math.insert_after(list, q, inserted);
                return Ok(());
            }
            match op {
                1 | 5 => {
                    let Kind::Atom(a) = &mut self.math.node_mut(q).kind else {
                        unreachable!()
                    };
                    a.fields.nucleus = Field::TextCharacter {
                        family,
                        character: rem,
                    };
                }
                2 | 6 => {
                    let Kind::Atom(a) = &mut self.math.node_mut(p).kind else {
                        unreachable!()
                    };
                    a.fields.nucleus = Field::Character {
                        family,
                        character: rem,
                    };
                }
                3 | 7 | 11 => {
                    let field = if op == 11 {
                        Field::TextCharacter {
                            family,
                            character: rem,
                        }
                    } else {
                        Field::Character {
                            family,
                            character: rem,
                        }
                    };
                    let mut inserted = MathList::default();
                    self.math.append(
                        &mut inserted,
                        Kind::Atom(Atom::new(Class::Ord, field)),
                        &mut self.engine.document.nodes,
                    )?;
                    self.math.insert_after(list, q, inserted);
                }
                _ => {
                    let next = self.math.node(p).next;
                    self.math.node_mut(q).next = next;
                    if list.tail == p {
                        list.tail = q;
                    }
                    let Kind::Atom(mut following) = self.math.free(p).kind else {
                        unreachable!()
                    };
                    let Kind::Atom(a) = &mut self.math.node_mut(q).kind else {
                        unreachable!()
                    };
                    a.fields.nucleus = Field::TextCharacter {
                        family,
                        character: rem,
                    };
                    a.fields.sub = std::mem::take(&mut following.fields.sub);
                    a.fields.sup = std::mem::take(&mut following.fields.sup);
                }
            }
            if op > 3 {
                return Ok(());
            }
            let Kind::Atom(a) = &mut self.math.node_mut(q).kind else {
                unreachable!()
            };
            if let Field::TextCharacter { family, character } = a.fields.nucleus {
                a.fields.nucleus = Field::Character { family, character };
            }
        }
    }
    pub(super) fn atom_step(&mut self, w: &mut AtomWork) -> Result<Action> {
        match w.stage {
            AtomStage::Nucleus => {
                if w.op_clean {
                    if let Field::Character {
                        family,
                        mut character,
                    } = w.atom.fields.nucleus
                    {
                        if let Some((font, mut metrics)) = self.glyph(family, character, w.style)? {
                            if w.style.display() {
                                let scaled = self.descriptor(font).scaled.unwrap();
                                if let Some(larger) = self.engine.font_catalog.scaled[scaled]
                                    .source()
                                    .larger_character(character)
                                    && let Some(m) = self.engine.character_metrics(font, larger)
                                {
                                    character = larger;
                                    metrics = m;
                                }
                            }
                            w.delta = metrics.italic;
                            w.atom.fields.nucleus = Field::Character { family, character };
                        } else {
                            w.atom.fields.nucleus = Field::Empty;
                        }
                    }
                    w.stage = AtomStage::NucleusReady;
                    return Ok(Action::Child(CleanWork::task(
                        std::mem::take(&mut w.atom.fields.nucleus),
                        w.style,
                    )));
                }
                let field = std::mem::take(&mut w.atom.fields.nucleus);
                match field {
                    Field::Character { family, character }
                    | Field::TextCharacter { family, character } => {
                        let text = matches!(field, Field::TextCharacter { .. });
                        if let Some((font, metrics)) = self.glyph(family, character, w.style)? {
                            w.delta = metrics.italic;
                            if text && self.engine.font_parameter(font as usize, 2) != Scaled::ZERO
                            {
                                w.delta = Scaled::ZERO;
                            }
                            self.append(&mut w.output, nodes::Kind::Character { font, character })?;
                            if !w.has_sub && w.delta != Scaled::ZERO {
                                self.kern(&mut w.output, w.delta.0)?;
                                w.delta = Scaled::ZERO;
                            }
                        }
                    }
                    Field::Box(b) => self.engine.document.nodes.append(&mut w.output, b),
                    Field::SubMath(list) => {
                        w.stage = AtomStage::NucleusReady;
                        return Ok(Action::Child(Task::List(ListWork::new(
                            list, w.style, false,
                        ))));
                    }
                    Field::Empty => {}
                }
                w.stage = AtomStage::Scripts;
            }
            AtomStage::NucleusReady => {
                let result = self.result();
                if w.op_clean {
                    let b = result.boxed();
                    let axis = self.symbol(w.style, 22);
                    if w.op_character {
                        let data = self.engine.document.nodes.box_mut(b.id());
                        if w.has_sub && !w.limits {
                            data.width.0 = data.width.0.wrapping_sub(w.delta.0);
                        }
                        data.shift = Scaled(
                            half(data.height.0.wrapping_sub(data.depth.0)).wrapping_sub(axis),
                        );
                    }
                    self.engine.document.nodes.append(&mut w.output, b);
                    if w.limits {
                        let b = self.clean_list(std::mem::take(&mut w.output))?;
                        w.base = Some(b);
                    }
                } else {
                    let b = self.engine.hpack(result.list(), PackSpec::Natural)?;
                    self.engine.document.nodes.append(&mut w.output, b);
                }
                w.stage = AtomStage::Scripts;
            }
            AtomStage::Scripts => {
                if !w.limits && !w.has_sub && !w.has_sup {
                    return Ok(Action::Done(Value::List(std::mem::take(&mut w.output))));
                }
                if !w.limits
                    && (w.output.is_empty()
                        || !matches!(
                            self.engine.document.nodes.get(w.output.head()).kind,
                            nodes::Kind::Character { .. }
                        ))
                {
                    let b = self
                        .engine
                        .hpack(std::mem::take(&mut w.output), PackSpec::Natural)?;
                    let size = if w.style.script() { 2 } else { 1 };
                    let data = self.engine.document.nodes.box_mut(b.id());
                    let (height, depth) = (data.height.0, data.depth.0);
                    w.output = std::mem::take(&mut data.list);
                    self.release_value(Value::Box(b));
                    w.up = height.wrapping_sub(self.parameter(2, size, 18));
                    w.down = depth.wrapping_add(self.parameter(2, size, 19));
                }
                w.stage = AtomStage::SupReady;
                if w.has_sup || w.limits {
                    return Ok(Action::Child(CleanWork::task(
                        std::mem::take(&mut w.atom.fields.sup),
                        w.style.sup(),
                    )));
                }
                w.stage = AtomStage::Sub;
            }
            AtomStage::SupReady => {
                let b = self.result().boxed();
                w.upper = Some(b);
                w.stage = AtomStage::Sub;
            }
            AtomStage::Sub => {
                w.stage = AtomStage::SubReady;
                if w.has_sub || w.limits {
                    return Ok(Action::Child(CleanWork::task(
                        std::mem::take(&mut w.atom.fields.sub),
                        w.style.sub(),
                    )));
                }
                w.stage = AtomStage::Finish;
            }
            AtomStage::SubReady => {
                w.lower = Some(self.result().boxed());
                w.stage = AtomStage::Finish;
            }
            AtomStage::Finish => {
                if w.limits {
                    self.finish_limits(w)?;
                } else {
                    self.finish_scripts(w)?;
                }
                return Ok(Action::Done(Value::List(std::mem::take(&mut w.output))));
            }
        }
        Ok(Action::Again)
    }
}
