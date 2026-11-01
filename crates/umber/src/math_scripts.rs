//! Ordinary scripts and operator limits after nucleus preparation.
use super::*;
use crate::layout_values::DimenParam;
impl Conversion<'_, '_, '_> {
    pub(super) fn finish_scripts(&mut self, w: &mut AtomWork) -> Result<()> {
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
    pub(super) fn finish_limits(&mut self, w: &mut AtomWork) -> Result<()> {
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
}
