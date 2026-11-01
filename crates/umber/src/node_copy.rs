//! Semantic copies own new cells; COW snapshots are a separate operation.
use super::{BoxData, DetachedNode, InsertionData, Kind, List, NodeId, NodeStore, ReleaseScratch};
use crate::Result;

#[derive(Clone, Copy)]
enum Child {
    Box,
    Insertion,
    Adjustment,
    Components,
    Pre,
    Post,
}
type Children = [(NodeId, Child); 2];
struct Frame {
    source: NodeId,
    output: List,
    attach: Option<(NodeId, Child)>,
}
#[derive(Default)]
pub(crate) struct CopyScratch {
    frames: Vec<Frame>,
    release: ReleaseScratch,
}

pub(super) fn copy_list(
    store: &mut NodeStore,
    source: NodeId,
    scratch: &mut CopyScratch,
    limit: usize,
) -> Result<List> {
    assert!(scratch.frames.is_empty());
    assert!(scratch.release.pending.is_empty());
    if source.is_null() {
        return Ok(List::default());
    }
    scratch.frames.push(Frame {
        source,
        output: List::default(),
        attach: None,
    });
    let result = (|| loop {
        let source = scratch.frames.last().unwrap().source;
        if source.is_null() {
            let frame = scratch.frames.pop().unwrap();
            if let Some((parent, child)) = frame.attach {
                store.attach_copy(parent, child, frame.output);
                continue;
            }
            assert!(scratch.frames.is_empty());
            return Ok(frame.output);
        }
        let (node, next, children) = store.copy_node(source, limit)?;
        let parent = node.id();
        let frame = scratch.frames.last_mut().unwrap();
        frame.source = next;
        // No fallible operation separates allocation from establishing ownership.
        store.append(&mut frame.output, node);
        for (source, child) in children.into_iter().rev() {
            if !source.is_null() {
                scratch.frames.push(Frame {
                    source,
                    output: List::default(),
                    attach: Some((parent, child)),
                });
            }
        }
    })();
    if result.is_err() {
        // Frames own disjoint trees. Completed children already belong to a
        // parent's tree; their frames were removed when ownership transferred.
        while let Some(frame) = scratch.frames.pop() {
            store.release(frame.output, &mut scratch.release);
        }
    }
    result
}
impl NodeStore {
    fn copy_node(
        &mut self,
        source: NodeId,
        limit: usize,
    ) -> Result<(DetachedNode, NodeId, Children)> {
        let node = self.get(source);
        let next = node.next;
        let mut children = [(NodeId::NULL, Child::Box); 2];
        // Copy scalar fields and child references before allocation can COW or
        // grow either directory. No source owner is moved or snapshot-copied.
        let copied = match &node.kind {
            Kind::Language(language) => self.alloc_with_limit(Kind::Language(*language), limit)?,
            Kind::Free => unreachable!(),
            Kind::Character { font, character } => self.alloc_with_limit(
                Kind::Character {
                    font: *font,
                    character: *character,
                },
                limit,
            )?,
            Kind::Ligature {
                font,
                character,
                components,
                left_boundary,
                right_boundary,
            } => {
                children[0] = (components.head(), Child::Components);
                self.alloc_with_limit(
                    Kind::ligature(
                        *font,
                        *character,
                        List::default(),
                        *left_boundary,
                        *right_boundary,
                    ),
                    limit,
                )?
            }
            Kind::MathBoundary { width, after } => self.alloc_with_limit(
                Kind::MathBoundary {
                    width: *width,
                    after: *after,
                },
                limit,
            )?,
            Kind::Kern { width, explicit } => self.alloc_with_limit(
                Kind::Kern {
                    width: *width,
                    explicit: *explicit,
                },
                limit,
            )?,
            Kind::Glue { spec, subtype } => self.alloc_with_limit(
                Kind::Glue {
                    spec: *spec,
                    subtype: *subtype,
                },
                limit,
            )?,
            Kind::Penalty(value) => self.alloc_with_limit(Kind::Penalty(*value), limit)?,
            Kind::Rule {
                width,
                height,
                depth,
            } => self.alloc_with_limit(
                Kind::Rule {
                    width: *width,
                    height: *height,
                    depth: *depth,
                },
                limit,
            )?,
            Kind::Box(id) => {
                let data = self.box_data(id);
                children[0] = (data.list.head(), Child::Box);
                let copy = BoxData {
                    list: List::default(),
                    vertical: data.vertical,
                    width: data.width,
                    height: data.height,
                    depth: data.depth,
                    shift: data.shift,
                    glue_set: data.glue_set,
                };
                self.alloc_box_with_limit(copy, limit)?
            }
            Kind::Insertion(id) => {
                let data = self.insertion_data(id);
                children[0] = (data.list.head(), Child::Insertion);
                let copy = InsertionData {
                    list: List::default(),
                    class: data.class,
                    height: data.height,
                    split_top: data.split_top,
                    max_depth: data.max_depth,
                    float_cost: data.float_cost,
                };
                self.alloc_insertion_with_limit(copy, limit)?
            }
            Kind::Adjustment(list) => {
                children[0] = (list.head(), Child::Adjustment);
                self.alloc_with_limit(Kind::Adjustment(List::default()), limit)?
            }
            Kind::Discretionary {
                pre,
                post,
                replace_count,
            } => {
                children = [(pre.head(), Child::Pre), (post.head(), Child::Post)];
                self.alloc_with_limit(
                    Kind::discretionary(List::default(), List::default(), *replace_count),
                    limit,
                )?
            }
        };
        Ok((copied, next, children))
    }
    fn attach_copy(&mut self, parent: NodeId, child: Child, list: List) {
        let target = match child {
            Child::Box => &mut self.box_mut(parent).list,
            Child::Insertion => &mut self.insertion_mut(parent).list,
            Child::Adjustment => {
                let Kind::Adjustment(list) = self.kind_mut(parent) else {
                    unreachable!("copied adjustment")
                };
                list
            }
            Child::Components => {
                let Kind::Ligature { components, .. } = self.kind_mut(parent) else {
                    unreachable!("copied ligature")
                };
                components
            }
            Child::Pre | Child::Post => {
                let Kind::Discretionary { pre, post, .. } = self.kind_mut(parent) else {
                    unreachable!("copied discretionary")
                };
                if matches!(child, Child::Pre) {
                    pre
                } else {
                    post
                }
            }
        };
        assert!(target.is_empty());
        *target = list;
    }
}

#[cfg(test)]
#[path = "node_copy_tests.rs"]
mod tests;
