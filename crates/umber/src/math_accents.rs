//! TeX make_math_accent, including original-style character script transfer.
use super::*;
#[derive(Clone, Copy)]
enum Stage {
    Start,
    Nucleus,
    Scripted,
    Finish,
}
pub(super) struct AccentWork {
    accent: MathChar,
    fields: Fields,
    style: Style,
    stage: Stage,
    font: u32,
    character: u8,
    skew: i32,
    width: i32,
    height: i32,
    clearance: i32,
    base: Option<DetachedNode>,
    temp: List,
}
pub(super) fn task(accent: MathChar, fields: Fields, style: Style) -> Task {
    Task::Accent(AccentWork {
        accent,
        fields,
        style,
        stage: Stage::Start,
        font: 0,
        character: accent.character,
        skew: 0,
        width: 0,
        height: 0,
        clearance: 0,
        base: None,
        temp: List::default(),
    })
}
impl Conversion<'_, '_, '_> {
    pub(super) fn release_accent(&mut self, w: AccentWork) {
        self.math
            .release_fields(w.fields, &mut self.engine.document.nodes);
        self.math.drain_release(&mut self.engine.document.nodes);
        if let Some(b) = w.base {
            self.release_value(Value::Box(b));
        }
        self.release_value(Value::List(w.temp));
    }
    pub(super) fn accent_step(&mut self, w: &mut AccentWork) -> Result<Action> {
        match w.stage {
            Stage::Start => {
                let Some((font, _)) = self.glyph(w.accent.family, w.accent.character, w.style)?
                else {
                    return Ok(self.accent_atom(w));
                };
                w.font = font;
                // clean_box copies a character field; owned box/mlist fields
                // transfer instead. Retain the scalar character for script swap.
                let field = if let Field::Character { family, character } = w.fields.nucleus {
                    if let Some((font, _)) = self.glyph(family, character, w.style)? {
                        if let Ok(skew) = u8::try_from(self.descriptor(font).skew) {
                            let scaled = self.descriptor(font).scaled.unwrap();
                            let r = self.engine.font_catalog.scaled[scaled].source();
                            if let Some([_, _, op, rem]) =
                                r.lig_instruction(character as u16, skew as u16, None)
                                && op >= 128
                            {
                                w.skew = self.engine.font_catalog.scaled[scaled]
                                    .descriptor()
                                    .kern(256 * (op as usize - 128) + rem as usize)
                                    .0;
                            }
                        }
                        Field::Character { family, character }
                    } else {
                        w.fields.nucleus = Field::Empty;
                        Field::Empty
                    }
                } else {
                    std::mem::take(&mut w.fields.nucleus)
                };
                w.stage = Stage::Nucleus;
                return Ok(Action::Child(CleanWork::task(field, w.style.cramp())));
            }
            Stage::Nucleus => {
                w.base = Some(self.result().boxed());
                let (width, height, _) = self.box_geometry(w.base.as_ref().unwrap());
                w.width = width;
                w.height = height;
                let scaled = self.descriptor(w.font).scaled.unwrap();
                let mut character = w.character;
                while let Some(next) = self.engine.font_catalog.scaled[scaled]
                    .source()
                    .larger_character(character)
                {
                    let Some(m) = self.engine.character_metrics(w.font, next) else {
                        break;
                    };
                    if m.width.0 > w.width {
                        break;
                    }
                    character = next;
                }
                w.character = character;
                w.clearance = height.min(self.engine.font_parameter(w.font as usize, 5).0);
                if matches!(w.fields.nucleus, Field::Character { .. })
                    && (!w.fields.sub.is_empty() || !w.fields.sup.is_empty())
                {
                    self.release_value(Value::Box(w.base.take().unwrap()));
                    let atom = Atom {
                        class: Class::Ord,
                        limits: Limits::Normal,
                        fields: std::mem::take(&mut w.fields),
                    };
                    w.stage = Stage::Scripted;
                    return Ok(Action::Child(Task::Atom(AtomWork::new(atom, w.style))));
                }
                w.stage = Stage::Finish;
            }
            Stage::Scripted => {
                let output = self.result().list();
                let b = self.clean_list(output)?;
                let (_, height, _) = self.box_geometry(&b);
                w.base = Some(b);
                w.clearance = w.clearance.wrapping_add(height.wrapping_sub(w.height));
                w.height = height;
                w.stage = Stage::Finish;
            }
            Stage::Finish => {
                let width = self.box_geometry(w.base.as_ref().unwrap()).0;
                let y = self.char_box(w.font, w.character)?;
                let data = self.engine.document.nodes.box_mut(y.id());
                data.shift = Scaled(
                    w.skew
                        .wrapping_add(half(w.width.wrapping_sub(data.width.0))),
                );
                data.width = Scaled::ZERO;
                self.engine.document.nodes.append(&mut w.temp, y);
                self.kern(&mut w.temp, w.clearance.wrapping_neg())?;
                self.engine
                    .document
                    .nodes
                    .append(&mut w.temp, w.base.take().unwrap());
                w.base = Some(self.engine.vpackage(
                    std::mem::take(&mut w.temp),
                    PackSpec::Natural,
                    Scaled::MAX,
                )?);
                let id = w.base.as_ref().unwrap().id();
                let data = self.engine.document.nodes.box_mut(id);
                data.width = Scaled(width);
                if data.height.0 < w.height {
                    let extra = w.height.wrapping_sub(data.height.0);
                    self.kern(&mut w.temp, extra)?;
                    let contents = std::mem::take(&mut self.engine.document.nodes.box_mut(id).list);
                    self.engine.document.nodes.splice(&mut w.temp, contents);
                    let data = self.engine.document.nodes.box_mut(id);
                    data.list = std::mem::take(&mut w.temp);
                    data.height = Scaled(w.height);
                }
                w.fields.nucleus = Field::Box(w.base.take().unwrap());
                return Ok(self.accent_atom(w));
            }
        }
        Ok(Action::Again)
    }
    fn accent_atom(&mut self, w: &mut AccentWork) -> Action {
        Action::Replace(Task::Atom(AtomWork::new(
            Atom {
                class: Class::Ord,
                limits: Limits::Normal,
                fields: std::mem::take(&mut w.fields),
            },
            w.style,
        )))
    }
}
