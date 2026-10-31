//! Commit selected breaks by transferring ranges of the original paragraph.
use crate::{
    Engine, Result,
    nodes::{GlueKind, Kind, List, NodeId},
    paragraph::{Parameters, Scratch},
    scalars::ParagraphParam as P,
};
impl Engine<'_> {
    pub(crate) fn construct_lines(
        &mut self,
        parameters: &Parameters,
        scratch: &mut Scratch,
        best_line: i32,
    ) -> Result<()> {
        let first_line = self.document.text.prev_graf + 1;
        let mut line_number = first_line;
        for index in 0..scratch.breaks.len() {
            let point = scratch.breaks[index];
            let glue_break = !point.is_null()
                && matches!(self.document.nodes.get(point).kind, Kind::Glue { .. });
            let left = if parameters.left != crate::numeric::Glue::default() {
                Some(self.document.nodes.alloc(Kind::Glue {
                    spec: parameters.left,
                    subtype: GlueKind::LeftSkip,
                })?)
            } else {
                None
            };
            let right = if glue_break {
                None
            } else {
                match self.document.nodes.alloc(Kind::Glue {
                    spec: parameters.right,
                    subtype: GlueKind::RightSkip,
                }) {
                    Ok(node) => Some(node),
                    Err(error) => {
                        if let Some(node) = left {
                            self.document.nodes.release_node(node, &mut scratch.release);
                        }
                        return Err(error);
                    }
                }
            };
            let mut disc_break = false;
            let mut post_disc_break = false;
            let mut remainder = if point.is_null() {
                List::default()
            } else {
                self.document
                    .nodes
                    .split_after(&mut self.document.text.horizontal, point)
            };
            if !point.is_null() {
                let kind = &mut self.document.nodes.get_mut(point).kind;
                match kind {
                    Kind::Glue { spec, subtype } => {
                        *spec = parameters.right;
                        *subtype = GlueKind::RightSkip;
                    }
                    Kind::Kern { width, .. } => *width = crate::numeric::Scaled::ZERO,
                    Kind::Discretionary {
                        pre,
                        post,
                        replace_count,
                    } => {
                        disc_break = true;
                        let count = std::mem::take(replace_count);
                        let pre = std::mem::take(pre);
                        let mut post = std::mem::take(post);
                        for _ in 0..count {
                            let node = self
                                .document
                                .nodes
                                .pop_front(&mut remainder)
                                .expect("discretionary replacement range");
                            self.document.nodes.release_node(node, &mut scratch.release);
                        }
                        post_disc_break = !post.is_empty();
                        self.document.nodes.splice(&mut post, remainder);
                        remainder = post;
                        self.document
                            .nodes
                            .splice(&mut self.document.text.horizontal, pre);
                    }
                    _ => {}
                }
            }
            if let Some(right) = right {
                self.document
                    .nodes
                    .append(&mut self.document.text.horizontal, right);
            }
            let contents = std::mem::replace(&mut self.document.text.horizontal, remainder);
            let contents = if let Some(left) = left {
                let mut with_left = List::default();
                self.document.nodes.append(&mut with_left, left);
                self.document.nodes.splice(&mut with_left, contents);
                with_left
            } else {
                contents
            };
            let shape = parameters.line(line_number);
            let b = self.hpack(contents, shape.width)?;
            if let Kind::Box(data) = &mut self.document.nodes.get_mut(b.id()).kind {
                data.shift = shape.indent;
            }
            self.append_to_vlist(b)?;
            if line_number + 1 != best_line {
                let mut penalty = parameters.integer(P::InterlinePenalty);
                if line_number == first_line {
                    penalty = penalty.wrapping_add(parameters.integer(P::ClubPenalty));
                }
                if line_number + 2 == best_line {
                    penalty = penalty.wrapping_add(parameters.integer(P::WidowPenalty));
                }
                if disc_break {
                    penalty = penalty.wrapping_add(parameters.integer(P::BrokenPenalty));
                }
                if penalty != 0 {
                    let node = self.document.nodes.alloc(Kind::Penalty(penalty))?;
                    self.document
                        .nodes
                        .append(&mut self.document.text.vertical, node);
                }
            }
            line_number += 1;
            if index + 1 < scratch.breaks.len() && !post_disc_break {
                self.prune_line_start(scratch.breaks[index + 1], scratch);
            }
        }
        assert_eq!(line_number, best_line);
        assert!(self.document.text.horizontal.is_empty());
        self.document.text.prev_graf = best_line - 1;
        Ok(())
    }
    fn prune_line_start(&mut self, next_break: NodeId, scratch: &mut Scratch) {
        loop {
            let p = self.document.text.horizontal.head();
            if p.is_null() || p == next_break {
                break;
            }
            if !matches!(
                self.document.nodes.get(p).kind,
                Kind::Glue { .. } | Kind::Penalty(_) | Kind::Kern { explicit: true, .. }
            ) {
                break;
            }
            let node = self
                .document
                .nodes
                .pop_front(&mut self.document.text.horizontal)
                .unwrap();
            self.document.nodes.release_node(node, &mut scratch.release);
        }
    }
}
