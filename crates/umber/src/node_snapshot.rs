//! Only the COW chunk copier duplicates numeric child owners into a new version.
use super::{BoxId, InsertionId, Kind, LeaderId, Node};
impl Node {
    // Copy an owned record ONLY into a different COW store version.
    pub(super) fn snapshot_copy(&self) -> Self {
        let kind = match &self.kind {
            Kind::Language(language) => Kind::Language(*language),
            Kind::Free => Kind::Free,
            Kind::Character { font, character } => Kind::Character {
                font: *font,
                character: *character,
            },
            Kind::Ligature {
                font,
                character,
                components,
                left_boundary,
                right_boundary,
            } => Kind::Ligature {
                font: *font,
                character: *character,
                components: components.snapshot(),
                left_boundary: *left_boundary,
                right_boundary: *right_boundary,
            },
            Kind::Kern { width, kind } => Kind::Kern {
                width: *width,
                kind: *kind,
            },
            Kind::MathBoundary { width, after } => Kind::MathBoundary {
                width: *width,
                after: *after,
            },
            Kind::Glue { spec, subtype } => Kind::Glue {
                spec: *spec,
                subtype: *subtype,
            },
            Kind::Penalty(p) => Kind::Penalty(*p),
            Kind::Rule {
                width,
                height,
                depth,
            } => Kind::Rule {
                width: *width,
                height: *height,
                depth: *depth,
            },
            Kind::Box(b) => Kind::Box(BoxId(b.0)),
            Kind::Unset(b) => Kind::Unset(BoxId(b.0)),
            Kind::Insertion(id) => Kind::Insertion(InsertionId(id.0)),
            Kind::Leader(id) => Kind::Leader(LeaderId(id.0)),
            Kind::Adjustment(list) => Kind::Adjustment(list.snapshot()),
            Kind::Discretionary {
                pre,
                post,
                replace_count,
            } => Kind::Discretionary {
                pre: pre.snapshot(),
                post: post.snapshot(),
                replace_count: *replace_count,
            },
        };
        Self {
            next: self.next,
            kind,
        }
    }
}
