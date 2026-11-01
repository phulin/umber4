//! Ordinary ligatures, nucleus continuations, scripts, and displayed limits.
use super::*;
use crate::layout_values::DimenParam;
impl Conversion<'_, '_, '_> {
    pub(super) fn make_ord(&mut self, q: MathId, style: Style, list: &mut MathList) -> Result<()> {
        loop {
            let (family, left, p) = {
                let n = self.math.node(q);
                let Kind::Atom(a) = &n.kind else {
                    unreachable!()
                };
                if !a.sub.is_empty() || !a.sup.is_empty() {
                    return Ok(());
                }
                let Field::Character { family, character } = a.nucleus else {
                    return Ok(());
                };
                (family, character, n.next)
            };
            if p == MathId::NULL {
                return Ok(());
            }
            let right = match &self.math.node(p).kind {
                Kind::Atom(a) if a.class as u8 <= Class::Punct as u8 => match a.nucleus {
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
            a.nucleus = Field::TextCharacter {
                family,
                character: left,
            };
            let Some((font, _)) = self.glyph(family, left, style)? else {
                let Kind::Atom(a) = &mut self.math.node_mut(q).kind else {
                    unreachable!()
                };
                a.nucleus = Field::Empty;
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
                    explicit: false,
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
                    a.nucleus = Field::TextCharacter {
                        family,
                        character: rem,
                    };
                }
                2 | 6 => {
                    let Kind::Atom(a) = &mut self.math.node_mut(p).kind else {
                        unreachable!()
                    };
                    a.nucleus = Field::Character {
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
                    a.nucleus = Field::TextCharacter {
                        family,
                        character: rem,
                    };
                    a.sub = std::mem::take(&mut following.sub);
                    a.sup = std::mem::take(&mut following.sup);
                }
            }
            if op > 3 {
                return Ok(());
            }
            let Kind::Atom(a) = &mut self.math.node_mut(q).kind else {
                unreachable!()
            };
            if let Field::TextCharacter { family, character } = a.nucleus {
                a.nucleus = Field::Character { family, character };
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
                    } = w.atom.nucleus
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
                            w.atom.nucleus = Field::Character { family, character };
                        } else {
                            w.atom.nucleus = Field::Empty;
                        }
                    }
                    w.stage = AtomStage::NucleusReady;
                    return Ok(Action::Child(CleanWork::task(
                        std::mem::take(&mut w.atom.nucleus),
                        w.style,
                    )));
                }
                let field = std::mem::take(&mut w.atom.nucleus);
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
                        std::mem::take(&mut w.atom.sup),
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
                        std::mem::take(&mut w.atom.sub),
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
    fn box_geometry(&self, b: &DetachedNode) -> (i32, i32, i32) {
        let nodes::Kind::Box(id) = &self.engine.document.nodes.get(b.id()).kind else {
            unreachable!()
        };
        let b = self.engine.document.nodes.box_data(id);
        (b.width.0, b.height.0, b.depth.0)
    }
    fn finish_scripts(&mut self, w: &mut AtomWork) -> Result<()> {
        let script_space = self.engine.document.layout_values.dimens
            [DimenParam::ScriptSpace.index()]
        .value
        .0;
        for b in [&w.upper, &w.lower].into_iter().flatten() {
            let data = self.engine.document.nodes.box_mut(b.id());
            data.width.0 = data.width.0.wrapping_add(script_space);
        }
        let xheight = self.symbol(w.style, 5);
        if let Some(upper) = &w.upper {
            let (_, _, depth) = self.box_geometry(upper);
            let parameter = if w.style.cramped() {
                15
            } else if w.style.display() {
                13
            } else {
                14
            };
            w.up =
                w.up.max(self.symbol(w.style, parameter))
                    .max(depth.wrapping_add(xheight.wrapping_abs() / 4));
        }
        if let Some(lower) = &w.lower {
            let (_, height, _) = self.box_geometry(lower);
            if let Some(upper) = &w.upper {
                w.down = w.down.max(self.symbol(w.style, 17));
                let (_, _, depth) = self.box_geometry(upper);
                let clearance = self.extension(w.style, 8).wrapping_mul(4).wrapping_sub(
                    w.up.wrapping_sub(depth)
                        .wrapping_sub(height.wrapping_sub(w.down)),
                );
                if clearance > 0 {
                    w.down = w.down.wrapping_add(clearance);
                    let extra = (xheight.wrapping_mul(4).wrapping_abs() / 5)
                        .wrapping_sub(w.up.wrapping_sub(depth));
                    if extra > 0 {
                        w.up = w.up.wrapping_add(extra);
                        w.down = w.down.wrapping_sub(extra);
                    }
                }
            } else {
                w.down = w
                    .down
                    .max(self.symbol(w.style, 16))
                    .max(height.wrapping_sub(xheight.wrapping_mul(4).wrapping_abs() / 5));
            }
        }
        let b = match (w.upper.is_some(), w.lower.is_some()) {
            (true, true) => {
                let (_, _, d) = self.box_geometry(w.upper.as_ref().unwrap());
                let (_, h, _) = self.box_geometry(w.lower.as_ref().unwrap());
                let upper = w.upper.take().unwrap();
                self.engine.document.nodes.box_mut(upper.id()).shift = w.delta;
                self.engine.document.nodes.append(&mut w.temp, upper);
                self.kern(
                    &mut w.temp,
                    w.up.wrapping_sub(d).wrapping_sub(h.wrapping_sub(w.down)),
                )?;
                self.engine
                    .document
                    .nodes
                    .append(&mut w.temp, w.lower.take().unwrap());
                let b = self.engine.vpackage(
                    std::mem::take(&mut w.temp),
                    PackSpec::Natural,
                    Scaled::MAX,
                )?;
                self.engine.document.nodes.box_mut(b.id()).shift = Scaled(w.down);
                b
            }
            (true, false) => {
                let b = w.upper.take().unwrap();
                self.engine.document.nodes.box_mut(b.id()).shift = Scaled(w.up.wrapping_neg());
                b
            }
            (false, true) => {
                let b = w.lower.take().unwrap();
                self.engine.document.nodes.box_mut(b.id()).shift = Scaled(w.down);
                b
            }
            _ => unreachable!("nonempty scripts"),
        };
        self.engine.document.nodes.append(&mut w.output, b);
        Ok(())
    }
    fn finish_limits(&mut self, w: &mut AtomWork) -> Result<()> {
        let width = [&w.base, &w.upper, &w.lower]
            .into_iter()
            .flatten()
            .map(|b| self.box_geometry(b).0)
            .max()
            .unwrap();
        for target in [&mut w.base, &mut w.upper, &mut w.lower] {
            *target = Some(self.rebox(target.take().unwrap(), Scaled(width))?);
        }
        let (_, mut height, mut depth) = self.box_geometry(w.base.as_ref().unwrap());
        let padding = self.extension(w.style, 13);
        if w.has_sup {
            let (_, h, d) = self.box_geometry(w.upper.as_ref().unwrap());
            let gap = self
                .extension(w.style, 11)
                .wrapping_sub(d)
                .max(self.extension(w.style, 9));
            height = height
                .wrapping_add(padding)
                .wrapping_add(h)
                .wrapping_add(d)
                .wrapping_add(gap);
            self.kern(&mut w.temp, padding)?;
            let upper = w.upper.take().unwrap();
            self.engine.document.nodes.box_mut(upper.id()).shift = Scaled(half(w.delta.0));
            self.engine.document.nodes.append(&mut w.temp, upper);
            self.kern(&mut w.temp, gap)?;
        } else {
            self.release_value(Value::Box(w.upper.take().unwrap()));
        }
        self.engine
            .document
            .nodes
            .append(&mut w.temp, w.base.take().unwrap());
        if w.has_sub {
            let (_, h, d) = self.box_geometry(w.lower.as_ref().unwrap());
            let gap = self
                .extension(w.style, 12)
                .wrapping_sub(h)
                .max(self.extension(w.style, 10));
            depth = depth
                .wrapping_add(padding)
                .wrapping_add(h)
                .wrapping_add(d)
                .wrapping_add(gap);
            self.kern(&mut w.temp, gap)?;
            let lower = w.lower.take().unwrap();
            self.engine.document.nodes.box_mut(lower.id()).shift =
                Scaled(half(w.delta.0).wrapping_neg());
            self.engine.document.nodes.append(&mut w.temp, lower);
            self.kern(&mut w.temp, padding)?;
        } else {
            self.release_value(Value::Box(w.lower.take().unwrap()));
        }
        let b = self.engine.document.nodes.alloc_box(BoxData {
            list: std::mem::take(&mut w.temp),
            vertical: true,
            width: Scaled(width),
            height: Scaled(height),
            depth: Scaled(depth),
            ..BoxData::default()
        })?;
        self.engine.document.nodes.append(&mut w.output, b);
        Ok(())
    }
    fn rebox(&mut self, mut b: DetachedNode, width: Scaled) -> Result<DetachedNode> {
        let (old_width, _, _) = self.box_geometry(&b);
        let data = self.engine.document.nodes.box_mut(b.id());
        if old_width == width.0 || data.list.is_empty() {
            data.width = width;
            return Ok(b);
        }
        if data.vertical {
            let mut list = List::default();
            self.engine.document.nodes.append(&mut list, b);
            b = self.engine.hpack(list, PackSpec::Natural)?;
        }
        let mut contents = std::mem::take(&mut self.engine.document.nodes.box_mut(b.id()).list);
        let data_width = self.engine.document.nodes.box_mut(b.id()).width;
        self.release_value(Value::Box(b));
        let mut result = List::default();
        let converted = (|| {
            if contents.head() == contents.tail()
                && let nodes::Kind::Character { font, character } =
                    self.engine.document.nodes.get(contents.head()).kind
            {
                let natural = self
                    .engine
                    .character_metrics(font, character)
                    .unwrap()
                    .width;
                if natural != data_width {
                    self.kern(&mut contents, data_width.0.wrapping_sub(natural.0))?;
                }
            }
            let glue = Glue {
                stretch: Scaled::ONE,
                shrink: Scaled::ONE,
                stretch_order: GlueOrder::Fil,
                shrink_order: GlueOrder::Fil,
                ..Glue::default()
            };
            self.append(
                &mut result,
                nodes::Kind::Glue {
                    spec: glue,
                    subtype: GlueKind::Normal,
                },
            )?;
            self.engine
                .document
                .nodes
                .splice(&mut result, std::mem::take(&mut contents));
            self.append(
                &mut result,
                nodes::Kind::Glue {
                    spec: glue,
                    subtype: GlueKind::Normal,
                },
            )?;
            self.engine
                .hpack(std::mem::take(&mut result), PackSpec::To(width))
        })();
        self.release_value(Value::List(contents));
        self.release_value(Value::List(result));
        converted
    }
}
