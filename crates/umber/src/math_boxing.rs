//! Shared owned box operations for math construction.
use super::*;
impl Conversion<'_, '_, '_> {
    pub(super) fn box_geometry(&self, b: &DetachedNode) -> (i32, i32, i32) {
        let nodes::Kind::Box(id) = &self.engine.document.nodes.get(b.id()).kind else {
            unreachable!()
        };
        let b = self.engine.document.nodes.box_data(id);
        (b.width.0, b.height.0, b.depth.0)
    }

    pub(super) fn rebox(&mut self, mut b: DetachedNode, width: Scaled) -> Result<DetachedNode> {
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
impl Conversion<'_, '_, '_> {
    pub(super) fn char_box(&mut self, font: u32, character: u8) -> Result<DetachedNode> {
        let m = self
            .engine
            .character_metrics(font, character)
            .expect("selected math glyph");
        let mut list = List::default();
        self.append(&mut list, nodes::Kind::Character { font, character })?;
        self.engine.document.nodes.alloc_box(BoxData {
            list,
            width: Scaled(m.width.0.wrapping_add(m.italic.0)),
            height: m.height,
            depth: m.depth,
            ..BoxData::default()
        })
    }
    pub(super) fn fraction_rule(&mut self, list: &mut List, thickness: i32) -> Result<()> {
        self.append(
            list,
            nodes::Kind::Rule {
                width: Scaled(crate::page_output::RUNNING),
                height: Scaled(thickness),
                depth: Scaled::ZERO,
            },
        )
    }
    pub(super) fn overbar(
        &mut self,
        b: DetachedNode,
        clearance: i32,
        thickness: i32,
    ) -> Result<DetachedNode> {
        let mut b = Some(b);
        let mut list = List::default();
        let result = (|| {
            self.kern(&mut list, thickness)?;
            self.fraction_rule(&mut list, thickness)?;
            self.kern(&mut list, clearance)?;
            self.engine
                .document
                .nodes
                .append(&mut list, b.take().unwrap());
            self.engine
                .vpackage(std::mem::take(&mut list), PackSpec::Natural, Scaled::MAX)
        })();
        if let Some(b) = b {
            self.release_value(Value::Box(b));
        }
        self.release_value(Value::List(list));
        result
    }
}
