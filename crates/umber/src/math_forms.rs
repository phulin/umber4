//! Scriptable nucleus preparations, resumed through ordinary atom work.
use super::*;
#[derive(Clone, Copy)]
enum Form {
    Over,
    Under,
    Radical(Delimiter),
    VCenter,
}
pub(super) struct FormWork {
    form: Form,
    fields: Fields,
    style: Style,
    waiting: bool,
    base: Option<DetachedNode>,
    temp: List,
}
pub(super) fn task(kind: Kind, style: Style) -> Task {
    let (form, fields) = match kind {
        Kind::Over(f) => (Form::Over, f),
        Kind::Under(f) => (Form::Under, f),
        Kind::Radical { fields, delimiter } => (Form::Radical(delimiter), fields),
        Kind::VCenter(f) => (Form::VCenter, f),
        _ => unreachable!("scriptable construction"),
    };
    Task::Form(FormWork {
        form,
        fields,
        style,
        waiting: false,
        base: None,
        temp: List::default(),
    })
}
impl Conversion<'_, '_, '_> {
    pub(super) fn release_form(&mut self, w: FormWork) {
        self.math
            .release_fields(w.fields, &mut self.engine.document.nodes);
        self.math.drain_release(&mut self.engine.document.nodes);
        if let Some(b) = w.base {
            self.release_value(Value::Box(b));
        }
        self.release_value(Value::List(w.temp));
    }
    pub(super) fn form_step(&mut self, w: &mut FormWork) -> Result<Action> {
        if matches!(w.form, Form::VCenter) {
            let Field::Box(b) = &w.fields.nucleus else {
                return Err(Error::Syntax);
            };
            let axis = self.symbol(w.style, 22);
            let data = self.engine.document.nodes.box_mut(b.id());
            if !data.vertical {
                return Err(Error::Syntax);
            }
            let total = data.height.0.wrapping_add(data.depth.0);
            data.height = Scaled(axis.wrapping_add(half(total)));
            data.depth = Scaled(total.wrapping_sub(data.height.0));
        } else {
            if !w.waiting {
                w.waiting = true;
                let style = if matches!(w.form, Form::Under) {
                    w.style
                } else {
                    w.style.cramp()
                };
                return Ok(Action::Child(CleanWork::task(
                    std::mem::take(&mut w.fields.nucleus),
                    style,
                )));
            }
            w.base = Some(self.result().boxed());
            let thickness = self.extension(w.style, 8);
            let b = match w.form {
                Form::Over => {
                    self.overbar(w.base.take().unwrap(), thickness.wrapping_mul(3), thickness)?
                }
                Form::Under => {
                    let (_, height, _) = self.box_geometry(w.base.as_ref().unwrap());
                    self.engine
                        .document
                        .nodes
                        .append(&mut w.temp, w.base.take().unwrap());
                    self.kern(&mut w.temp, thickness.wrapping_mul(3))?;
                    self.fraction_rule(&mut w.temp, thickness)?;
                    let b = self.engine.vpackage(
                        std::mem::take(&mut w.temp),
                        PackSpec::Natural,
                        Scaled::MAX,
                    )?;
                    let data = self.engine.document.nodes.box_mut(b.id());
                    let total = data
                        .height
                        .0
                        .wrapping_add(data.depth.0)
                        .wrapping_add(thickness);
                    data.height = Scaled(height);
                    data.depth = Scaled(total.wrapping_sub(height));
                    b
                }
                Form::Radical(delimiter) => {
                    let (_, height, depth) = self.box_geometry(w.base.as_ref().unwrap());
                    let mut clearance = thickness.wrapping_add(if w.style.display() {
                        self.symbol(w.style, 5).wrapping_abs() / 4
                    } else {
                        thickness.wrapping_abs() / 4
                    });
                    let y = self.delimiter(
                        delimiter,
                        w.style,
                        height
                            .wrapping_add(depth)
                            .wrapping_add(clearance)
                            .wrapping_add(thickness),
                    )?;
                    let (_, yh, yd) = self.box_geometry(&y);
                    let delta = yd.wrapping_sub(height.wrapping_add(depth).wrapping_add(clearance));
                    if delta > 0 {
                        clearance = clearance.wrapping_add(half(delta));
                    }
                    self.engine.document.nodes.box_mut(y.id()).shift =
                        Scaled(height.wrapping_add(clearance).wrapping_neg());
                    self.engine.document.nodes.append(&mut w.temp, y);
                    let b = self.overbar(w.base.take().unwrap(), clearance, yh)?;
                    self.engine.document.nodes.append(&mut w.temp, b);
                    self.engine
                        .hpack(std::mem::take(&mut w.temp), PackSpec::Natural)?
                }
                Form::VCenter => unreachable!(),
            };
            w.fields.nucleus = Field::Box(b);
        }
        let atom = Atom {
            class: Class::Ord,
            limits: Limits::Normal,
            fields: std::mem::take(&mut w.fields),
        };
        Ok(Action::Replace(Task::Atom(AtomWork::new(atom, w.style))))
    }
}
