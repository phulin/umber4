//! Packing existing owned lists; the page builder can share glue calculations.
use crate::{
    Engine, Result,
    layout_values::{DimenParam, GlueParam},
    nodes::{BoxData, DetachedNode, GlueKind, GlueSet, GlueSign, Kind, List},
    numeric::{GlueOrder, Scaled},
};

#[derive(Clone, Copy, Debug)]
pub(crate) enum PackSpec {
    Natural,
    To(Scaled),
    Spread(Scaled),
}
impl PackSpec {
    fn target(self, natural: i32) -> Scaled {
        match self {
            Self::Natural => Scaled(natural),
            Self::To(target) => target,
            Self::Spread(amount) => Scaled(natural.wrapping_add(amount.0)),
        }
    }
}

pub(crate) fn glue_setting(
    excess: i32,
    stretch: [i32; 4],
    shrink: [i32; 4],
    nonempty: bool,
) -> GlueSet {
    if excess == 0 {
        return GlueSet::default();
    }
    let totals = if excess > 0 { stretch } else { shrink };
    let order = (0..4).rev().find(|&i| totals[i] != 0).unwrap_or(0);
    let ord = [
        GlueOrder::Normal,
        GlueOrder::Fil,
        GlueOrder::Fill,
        GlueOrder::Filll,
    ][order];
    let mut setting = GlueSet {
        sign: if excess > 0 {
            GlueSign::Stretching
        } else {
            GlueSign::Shrinking
        },
        order: ord,
        ratio: 0.0,
    };
    if totals[order] != 0 {
        setting.ratio = if excess > 0 {
            f64::from(excess)
        } else {
            -f64::from(excess)
        } / f64::from(totals[order]);
    } else {
        setting.sign = GlueSign::Normal;
    }
    if excess < 0 && order == 0 && i64::from(totals[0]) < -i64::from(excess) && nonempty {
        setting.ratio = 1.0;
    }
    setting
}
impl Engine<'_> {
    pub(crate) fn hpack(&mut self, list: List, spec: PackSpec) -> Result<DetachedNode> {
        let mut natural = 0i32;
        let mut height = 0;
        let mut depth = 0;
        let mut stretch = [0i32; 4];
        let mut shrink = [0i32; 4];
        let mut metrics = self.metric_reader();
        let mut p = list.head();
        while !p.is_null() {
            let n = self.document.nodes.get(p);
            match &n.kind {
                Kind::Character { font, character }
                | Kind::Ligature {
                    font, character, ..
                } => {
                    let m = metrics
                        .get(*font, *character)
                        .expect("constructed glyph has metrics");
                    natural = natural.wrapping_add(m.width.0);
                    height = height.max(m.height.0);
                    depth = depth.max(m.depth.0);
                }
                Kind::Box(b) => {
                    let b = self.document.nodes.box_data(b);
                    natural = natural.wrapping_add(b.width.0);
                    height = height.max(b.height.0.wrapping_sub(b.shift.0));
                    depth = depth.max(b.depth.0.wrapping_add(b.shift.0));
                }
                Kind::Rule {
                    width,
                    height: h,
                    depth: d,
                } => {
                    natural = natural.wrapping_add(width.0);
                    height = height.max(h.0);
                    depth = depth.max(d.0);
                }
                Kind::Kern { width, .. } => natural = natural.wrapping_add(width.0),
                Kind::Glue { spec, .. } => {
                    natural = natural.wrapping_add(spec.width.0);
                    let o = spec.stretch_order as usize;
                    stretch[o] = stretch[o].wrapping_add(spec.stretch.0);
                    let o = spec.shrink_order as usize;
                    shrink[o] = shrink[o].wrapping_add(spec.shrink.0);
                }
                _ => {}
            }
            p = n.next;
        }
        let width = spec.target(natural);
        let glue_set = glue_setting(
            width.0.wrapping_sub(natural),
            stretch,
            shrink,
            !list.is_empty(),
        );
        self.document.nodes.alloc_box(BoxData {
            list,
            width,
            height: Scaled(height),
            depth: Scaled(depth),
            glue_set,
            ..BoxData::default()
        })
    }
    pub(crate) fn vpackage(
        &mut self,
        list: List,
        spec: PackSpec,
        max_depth: Scaled,
    ) -> Result<DetachedNode> {
        let (mut natural, mut depth, mut width) = (0i32, 0i32, 0i32);
        let (mut stretch, mut shrink) = ([0i32; 4], [0i32; 4]);
        let mut p = list.head();
        while !p.is_null() {
            let n = self.document.nodes.get(p);
            match &n.kind {
                Kind::Box(b) => {
                    let b = self.document.nodes.box_data(b);
                    natural = natural.wrapping_add(depth).wrapping_add(b.height.0);
                    depth = b.depth.0;
                    width = width.max(b.width.0.wrapping_add(b.shift.0));
                }
                Kind::Rule {
                    width: w,
                    height: h,
                    depth: d,
                } => {
                    natural = natural.wrapping_add(depth).wrapping_add(h.0);
                    depth = d.0;
                    width = width.max(w.0);
                }
                Kind::Kern { width, .. } => {
                    natural = natural.wrapping_add(depth).wrapping_add(width.0);
                    depth = 0;
                }
                Kind::Glue { spec, .. } => {
                    natural = natural.wrapping_add(depth).wrapping_add(spec.width.0);
                    depth = 0;
                    let i = spec.stretch_order as usize;
                    stretch[i] = stretch[i].wrapping_add(spec.stretch.0);
                    let i = spec.shrink_order as usize;
                    shrink[i] = shrink[i].wrapping_add(spec.shrink.0);
                }
                _ => {}
            }
            p = n.next;
        }
        if depth > max_depth.0 {
            natural = natural.wrapping_add(depth).wrapping_sub(max_depth.0);
            depth = max_depth.0;
        }
        let height = spec.target(natural);
        let glue_set = glue_setting(
            height.0.wrapping_sub(natural),
            stretch,
            shrink,
            !list.is_empty(),
        );
        self.document.nodes.alloc_box(BoxData {
            list,
            vertical: true,
            width: Scaled(width),
            height,
            depth: Scaled(depth),
            glue_set,
            ..BoxData::default()
        })
    }
    pub(crate) fn append_to_vlist(&mut self, b: DetachedNode) -> Result<()> {
        let (height, depth) = match &self.document.nodes.get(b.id()).kind {
            Kind::Box(b) => {
                let b = self.document.nodes.box_data(b);
                (b.height, b.depth)
            }
            _ => unreachable!(),
        };
        if self.document.text.prev_depth.0 > -65_536_000 {
            let mut glue = self.document.layout_values.glues[GlueParam::Baseline.index()].value;
            let d = glue
                .width
                .0
                .wrapping_sub(self.document.text.prev_depth.0)
                .wrapping_sub(height.0);
            let limit = self.document.layout_values.dimens[DimenParam::LineSkipLimit.index()]
                .value
                .0;
            let subtype = if d < limit {
                glue = self.document.layout_values.glues[GlueParam::Line.index()].value;
                GlueKind::LineSkip
            } else {
                glue.width = Scaled(d);
                GlueKind::BaselineSkip
            };
            // If adding the baseline node fails, retain the still-owned box in
            // the vertical root so error/restore bulk release reaches its list.
            match self.document.nodes.alloc(Kind::Glue {
                spec: glue,
                subtype,
            }) {
                Ok(node) => self
                    .document
                    .nodes
                    .append(&mut self.document.text.vertical, node),
                Err(error) => {
                    self.document
                        .nodes
                        .append(&mut self.document.text.vertical, b);
                    return Err(error);
                }
            }
        }
        self.document
            .nodes
            .append(&mut self.document.text.vertical, b);
        self.document.text.prev_depth = depth;
        Ok(())
    }
}

#[cfg(test)]
#[path = "packing_tests.rs"]
mod tests;
