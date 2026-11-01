//! Native display fitting and consuming transfer of disjoint box/list owners.
use super::*;
use crate::{
    layout_values::GlueParam,
    nodes::NodeId,
    packing::{HorizontalTotals, PackSpec},
};

pub(super) struct Work {
    pub formula: Option<DetachedNode>,
    pub number: Option<DetachedNode>,
    pub adjustments: List,
}
impl Work {
    pub fn release(&mut self, e: &mut Engine<'_>) {
        for node in [self.formula.take(), self.number.take()]
            .into_iter()
            .flatten()
        {
            e.document.nodes.release_node(node, &mut e.page_release);
        }
        e.document
            .nodes
            .release(std::mem::take(&mut self.adjustments), &mut e.page_release);
    }
}
impl Engine<'_> {
    fn display_box_width(&self, node: &DetachedNode) -> i32 {
        let nodes::Kind::Box(id) = &self.document.nodes.get(node.id()).kind else {
            unreachable!()
        };
        self.document.nodes.box_data(id).width.0
    }
    fn repack_display(&mut self, work: &mut Work, width: i32) -> Result<()> {
        let contents = self
            .document
            .nodes
            .take_box(work.formula.take().unwrap())
            .list;
        work.formula = Some(self.hpack(contents, PackSpec::To(Scaled(width)))?);
        Ok(())
    }
    fn display_node(&mut self, kind: nodes::Kind) -> Result<()> {
        let node = self.document.nodes.alloc(kind)?;
        self.document
            .nodes
            .append(&mut self.document.text.vertical, node);
        Ok(())
    }
    fn display_skip(&mut self, parameter: GlueParam) -> Result<()> {
        let spec = self.document.layout_values.glues[parameter.index()].value;
        self.display_node(nodes::Kind::Glue {
            spec,
            subtype: nodes::GlueKind::Normal,
        })
    }
    pub(super) fn layout_display(
        &mut self,
        work: &mut Work,
        totals: HorizontalTotals,
        left: bool,
    ) -> Result<()> {
        let mut w = self.display_box_width(work.formula.as_ref().unwrap());
        let z = self.document.layout_values.dimens[DimenParam::DisplayWidth.index()]
            .value
            .0;
        let s = self.document.layout_values.dimens[DimenParam::DisplayIndent.index()]
            .value
            .0;
        let pre = self.document.layout_values.dimens[DimenParam::PreDisplaySize.index()]
            .value
            .0;
        let mut e = work
            .number
            .as_ref()
            .map_or(0, |n| self.display_box_width(n));
        let q = if work.number.is_some() {
            let font = self.document.scalars[Target::family(0, 2)].value as usize;
            e.wrapping_add(self.font_parameter(font, 6).0)
        } else {
            0
        };
        let first = {
            let nodes::Kind::Box(id) = &self
                .document
                .nodes
                .get(work.formula.as_ref().unwrap().id())
                .kind
            else {
                unreachable!()
            };
            self.document.nodes.box_data(id).list.head()
        };
        if w.wrapping_add(q) > z {
            if e != 0
                && (w.wrapping_sub(totals.shrink[0]).wrapping_add(q) <= z
                    || totals.shrink[1..].iter().any(|&v| v != 0))
            {
                self.repack_display(work, z.wrapping_sub(q))?;
            } else {
                e = 0;
                if w > z {
                    self.repack_display(work, z)?;
                }
            }
            w = self.display_box_width(work.formula.as_ref().unwrap());
        }
        let mut d = crate::math::half(z.wrapping_sub(w));
        if e > 0 && d < e.wrapping_mul(2) {
            d = crate::math::half(z.wrapping_sub(w).wrapping_sub(e));
            if first != NodeId::NULL && self.document.nodes.get(first).kind.is_glue() {
                d = 0;
            }
        }
        self.display_node(nodes::Kind::Penalty(
            self.document.scalars[Target::PRE_DISPLAY].value,
        ))?;
        let normal = d.wrapping_add(s) <= pre || left;
        let above = if normal {
            GlueParam::AboveDisplay
        } else {
            GlueParam::AboveDisplayShort
        };
        let mut below = Some(if normal {
            GlueParam::BelowDisplay
        } else {
            GlueParam::BelowDisplayShort
        });
        if left && e == 0 {
            let number = work.number.take().expect("left number");
            self.document.nodes.box_mut(number.id()).shift = Scaled(s);
            self.append_to_vlist(number)?;
            self.display_node(nodes::Kind::Penalty(10000))?;
        } else {
            self.display_skip(above)?;
        }
        if e != 0 {
            let kern = self.document.nodes.alloc(nodes::Kind::Kern {
                width: Scaled(z.wrapping_sub(w).wrapping_sub(e).wrapping_sub(d)),
                kind: KernKind::Normal,
            })?;
            let mut combined = List::default();
            let formula = work.formula.take().unwrap();
            let number = work.number.take().unwrap();
            if left {
                self.document.nodes.append(&mut combined, number);
                self.document.nodes.append(&mut combined, kern);
                self.document.nodes.append(&mut combined, formula);
                d = 0;
            } else {
                self.document.nodes.append(&mut combined, formula);
                self.document.nodes.append(&mut combined, kern);
                self.document.nodes.append(&mut combined, number);
            }
            work.formula = Some(self.hpack(combined, PackSpec::Natural)?);
        }
        let formula = work.formula.take().unwrap();
        self.document.nodes.box_mut(formula.id()).shift = Scaled(s.wrapping_add(d));
        self.append_to_vlist(formula)?;
        if !left && e == 0 && work.number.is_some() {
            self.display_node(nodes::Kind::Penalty(10000))?;
            let number = work.number.take().unwrap();
            let width = self.display_box_width(&number);
            self.document.nodes.box_mut(number.id()).shift =
                Scaled(s.wrapping_add(z).wrapping_sub(width));
            self.append_to_vlist(number)?;
            below = None;
        }
        self.document.nodes.splice(
            &mut self.document.text.vertical,
            std::mem::take(&mut work.adjustments),
        );
        self.display_node(nodes::Kind::Penalty(
            self.document.scalars[Target::POST_DISPLAY].value,
        ))?;
        if let Some(below) = below {
            self.display_skip(below)?;
        }
        Ok(())
    }
}
