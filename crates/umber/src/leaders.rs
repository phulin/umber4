//! Leader operands use ordinary box construction and one affine template owner.
use crate::{
    Engine, Error, Result,
    boxes::Destination,
    nodes::{DetachedNode, GlueKind, LeaderData, List},
    token::*,
};
impl Engine<'_> {
    pub(crate) fn begin_leaders(&mut self, operation: i32) -> Result<()> {
        let alignment = match operation {
            0 => GlueKind::AlignedLeaders,
            1 => GlueKind::CenteredLeaders,
            _ => GlueKind::ExpandedLeaders,
        };
        self.scan_box(Destination::Leader(alignment))
    }
    pub(crate) fn finish_leader(&mut self, alignment: GlueKind, node: DetachedNode) -> Result<()> {
        let spec = (|| {
            let read = loop {
                let read = self.nonspace(true)?;
                if read.meaning.command != cmd::RELAX {
                    break read;
                }
            };
            let vertical = self.document.text.mode.is_vertical();
            if read.meaning.command != if vertical { cmd::VSKIP } else { cmd::HSKIP } {
                return Err(Error::Syntax);
            }
            self.scan_skip(read.meaning.value)
        })();
        let spec = match spec {
            Ok(spec) => spec,
            Err(error) => {
                self.document
                    .nodes
                    .release_node(node, &mut self.page_release);
                return Err(error);
            }
        };
        let mut template = List::default();
        self.document.nodes.append(&mut template, node);
        let node = self.document.nodes.alloc_leader(LeaderData {
            list: template,
            spec,
            subtype: alignment,
        })?;
        if self.document.text.mode.is_math() {
            self.append_math(crate::math::Kind::Material(node))?;
        } else {
            let list = if self.document.text.mode.is_vertical() {
                &mut self.document.text.vertical
            } else {
                &mut self.document.text.horizontal
            };
            self.document.nodes.append(list, node);
        }
        Ok(())
    }
}
