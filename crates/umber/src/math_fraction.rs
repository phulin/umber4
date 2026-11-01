//! make_fraction with affine numerator/denominator and delimiter construction.
use super::*;
pub(super) struct FractionWork {
    fraction: Fraction,
    style: Style,
    stage: u8,
    numerator: Option<DetachedNode>,
    denominator: Option<DetachedNode>,
    temp: List,
    output: List,
    packed: Option<DetachedNode>,
}
pub(super) fn task(fraction: Fraction, style: Style) -> Task {
    Task::Fraction(FractionWork {
        fraction,
        style,
        stage: 0,
        numerator: None,
        denominator: None,
        temp: List::default(),
        output: List::default(),
        packed: None,
    })
}
impl Conversion<'_, '_, '_> {
    pub(super) fn release_fraction(&mut self, w: FractionWork) {
        self.math
            .release_kind(Kind::Fraction(w.fraction), &mut self.engine.document.nodes);
        for b in [w.numerator, w.denominator, w.packed].into_iter().flatten() {
            self.release_value(Value::Box(b));
        }
        self.release_value(Value::List(w.temp));
        self.release_value(Value::List(w.output));
    }
    pub(super) fn fraction_step(&mut self, w: &mut FractionWork) -> Result<Action> {
        match w.stage {
            0 => {
                w.stage = 1;
                return Ok(Action::Child(CleanWork::task(
                    Field::SubMath(std::mem::take(&mut w.fraction.numerator)),
                    w.style.numerator(),
                )));
            }
            1 => {
                w.numerator = Some(self.result().boxed());
                w.stage = 2;
                return Ok(Action::Child(CleanWork::task(
                    Field::SubMath(std::mem::take(&mut w.fraction.denominator)),
                    w.style.denominator(),
                )));
            }
            2 => {
                w.denominator = Some(self.result().boxed());
                w.stage = 3;
            }
            3 => {
                self.finish_fraction(w)?;
                return Ok(Action::Done(Value::List(std::mem::take(&mut w.output))));
            }
            _ => unreachable!(),
        }
        Ok(Action::Again)
    }
    fn finish_fraction(&mut self, w: &mut FractionWork) -> Result<()> {
        let nw = self.box_geometry(w.numerator.as_ref().unwrap()).0;
        let dw = self.box_geometry(w.denominator.as_ref().unwrap()).0;
        if nw < dw {
            w.numerator = Some(self.rebox(w.numerator.take().unwrap(), Scaled(dw))?)
        } else {
            w.denominator = Some(self.rebox(w.denominator.take().unwrap(), Scaled(nw))?)
        }
        let (width, nh, nd) = self.box_geometry(w.numerator.as_ref().unwrap());
        let (_, dh, dd) = self.box_geometry(w.denominator.as_ref().unwrap());
        let rule = self.extension(w.style, 8);
        let thickness = match w.fraction.thickness {
            RuleThickness::Default => rule,
            RuleThickness::Explicit(t) => t.0,
        };
        let mut up = self.symbol(
            w.style,
            if w.style.display() {
                8
            } else if thickness != 0 {
                9
            } else {
                10
            },
        );
        let mut down = self.symbol(w.style, if w.style.display() { 11 } else { 12 });
        let axis = self.symbol(w.style, 22);
        let delta = half(thickness);
        if thickness == 0 {
            let clearance = rule.wrapping_mul(if w.style.display() { 7 } else { 3 });
            let extra = half(
                clearance.wrapping_sub(up.wrapping_sub(nd).wrapping_sub(dh.wrapping_sub(down))),
            );
            if extra > 0 {
                up = up.wrapping_add(extra);
                down = down.wrapping_add(extra);
            }
        } else {
            let clearance = thickness.wrapping_mul(if w.style.display() { 3 } else { 1 });
            let d1 =
                clearance.wrapping_sub(up.wrapping_sub(nd).wrapping_sub(axis.wrapping_add(delta)));
            let d2 = clearance
                .wrapping_sub(axis.wrapping_sub(delta).wrapping_sub(dh.wrapping_sub(down)));
            if d1 > 0 {
                up = up.wrapping_add(d1);
            }
            if d2 > 0 {
                down = down.wrapping_add(d2);
            }
        }
        self.engine
            .document
            .nodes
            .append(&mut w.temp, w.numerator.take().unwrap());
        if thickness == 0 {
            self.kern(
                &mut w.temp,
                up.wrapping_sub(nd).wrapping_sub(dh.wrapping_sub(down)),
            )?;
        } else {
            self.kern(
                &mut w.temp,
                up.wrapping_sub(nd).wrapping_sub(axis.wrapping_add(delta)),
            )?;
            self.fraction_rule(&mut w.temp, thickness)?;
            self.kern(
                &mut w.temp,
                axis.wrapping_sub(delta).wrapping_sub(dh.wrapping_sub(down)),
            )?;
        }
        self.engine
            .document
            .nodes
            .append(&mut w.temp, w.denominator.take().unwrap());
        w.packed = Some(self.engine.document.nodes.alloc_box(BoxData {
            list: std::mem::take(&mut w.temp),
            vertical: true,
            width: Scaled(width),
            height: Scaled(up.wrapping_add(nh)),
            depth: Scaled(dd.wrapping_add(down)),
            ..BoxData::default()
        })?);
        let target = self.symbol(w.style, if w.style.display() { 20 } else { 21 });
        let left = self.delimiter(w.fraction.left, w.style, target)?;
        self.engine.document.nodes.append(&mut w.output, left);
        self.engine
            .document
            .nodes
            .append(&mut w.output, w.packed.take().unwrap());
        let right = self.delimiter(w.fraction.right, w.style, target)?;
        self.engine.document.nodes.append(&mut w.output, right);
        let b = self
            .engine
            .hpack(std::mem::take(&mut w.output), PackSpec::Natural)?;
        self.engine.document.nodes.append(&mut w.output, b);
        Ok(())
    }
}
