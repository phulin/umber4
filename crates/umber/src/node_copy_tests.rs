use super::*;
use crate::{
    nodes::{CELLS, GlueKind, GlueSet, GlueSign},
    numeric::{Glue, GlueOrder, Scaled},
};

fn append(store: &mut NodeStore, list: &mut List, kind: Kind) -> NodeId {
    let node = store.alloc(kind).unwrap();
    let id = node.id();
    store.append(list, node);
    id
}
fn boxed(store: &mut NodeStore, list: &mut List, data: BoxData) {
    let node = store.alloc_box(data).unwrap();
    store.append(list, node);
}
fn glyph(font: u32, character: u8) -> Kind {
    Kind::Character { font, character }
}
fn penalties(store: &mut NodeStore, count: usize) -> List {
    let mut list = List::default();
    for i in 0..count {
        append(store, &mut list, Kind::Penalty(i as i32));
    }
    list
}
fn fixture(store: &mut NodeStore) -> List {
    let mut root = List::default();
    append(store, &mut root, glyph(7, 255));
    let mut components = List::default();
    append(store, &mut components, glyph(3, b'f'));
    append(
        store,
        &mut components,
        Kind::Kern {
            width: Scaled(-7),
            explicit: false,
        },
    );
    append(
        store,
        &mut root,
        Kind::ligature(3, b'X', components, true, false),
    );
    let mut pre = List::default();
    let mut contents = List::default();
    append(store, &mut contents, glyph(5, b'-'));
    boxed(
        store,
        &mut pre,
        BoxData {
            list: contents,
            width: Scaled(21),
            height: Scaled(-5),
            depth: Scaled(8),
            shift: Scaled(9),
            glue_set: GlueSet {
                sign: GlueSign::Stretching,
                order: GlueOrder::Fill,
                ratio: -0.0,
            },
            ..BoxData::default()
        },
    );
    let mut post = List::default();
    append(
        store,
        &mut post,
        Kind::Glue {
            spec: Glue {
                width: Scaled(-3),
                stretch: Scaled(17),
                shrink: Scaled(-11),
                stretch_order: GlueOrder::Filll,
                shrink_order: GlueOrder::Fil,
            },
            subtype: GlueKind::XSpaceSkip,
        },
    );
    append(store, &mut post, glyph(11, b'p'));
    append(store, &mut root, Kind::discretionary(pre, post, 2));
    append(store, &mut root, glyph(13, b'r'));
    let mut components = List::default();
    append(store, &mut components, glyph(2, b'a'));
    append(
        store,
        &mut root,
        Kind::ligature(2, b'z', components, false, true),
    );
    append(
        store,
        &mut root,
        Kind::Kern {
            width: Scaled(i32::MIN),
            explicit: true,
        },
    );
    append(
        store,
        &mut root,
        Kind::Glue {
            spec: Glue::default(),
            subtype: GlueKind::ParFillSkip,
        },
    );
    append(store, &mut root, Kind::Penalty(-10000));
    append(
        store,
        &mut root,
        Kind::Rule {
            width: Scaled(-1),
            height: Scaled(17),
            depth: Scaled(-4),
        },
    );
    let mut contents = penalties(store, 1);
    boxed(store, &mut contents, BoxData::default());
    boxed(
        store,
        &mut root,
        BoxData {
            list: contents,
            vertical: true,
            width: Scaled::MAX,
            height: Scaled(51),
            depth: Scaled(-12),
            shift: Scaled(-4),
            glue_set: GlueSet {
                sign: GlueSign::Shrinking,
                order: GlueOrder::Filll,
                ratio: -2.125,
            },
        },
    );
    root
}
// Exact source IDs, links and payloads must remain unchanged, including under COW.
fn records(store: &NodeStore, head: NodeId) -> Vec<(NodeId, String)> {
    let mut pending = vec![head];
    let mut out = Vec::new();
    while let Some(mut id) = pending.pop() {
        while !id.is_null() {
            let node = store.get(id);
            let mut text = format!("{node:?}");
            match &node.kind {
                Kind::Box(id) => {
                    let data = store.box_data(id);
                    text.push_str(&format!("{data:?}"));
                    pending.push(data.list.head());
                }
                Kind::Ligature { components, .. } => pending.push(components.head()),
                Kind::Discretionary { pre, post, .. } => {
                    pending.push(pre.head());
                    pending.push(post.head());
                }
                _ => {}
            }
            out.push((id, text));
            id = node.next;
        }
    }
    out
}
fn assert_copy(store: &NodeStore, source: NodeId, copy: NodeId) -> Vec<(NodeId, NodeId)> {
    let mut pending = vec![(source, copy)];
    let mut pairs = Vec::new();
    let mut tails = Vec::new();
    while let Some((mut a, mut b)) = pending.pop() {
        while !a.is_null() {
            assert!(!b.is_null());
            assert_ne!(a, b);
            let left = store.get(a);
            let right = store.get(b);
            match (&left.kind, &right.kind) {
                (Kind::Box(a), Kind::Box(b)) => {
                    assert_ne!(a.0, b.0, "box payload owner must be fresh");
                    let (a, b) = (store.box_data(a), store.box_data(b));
                    assert_eq!(
                        (
                            a.vertical,
                            a.width,
                            a.height,
                            a.depth,
                            a.shift,
                            a.glue_set.sign,
                            a.glue_set.order,
                            a.glue_set.ratio.to_bits()
                        ),
                        (
                            b.vertical,
                            b.width,
                            b.height,
                            b.depth,
                            b.shift,
                            b.glue_set.sign,
                            b.glue_set.order,
                            b.glue_set.ratio.to_bits()
                        )
                    );
                    pending.push((a.list.head(), b.list.head()));
                    tails.push((a.list.tail(), b.list.tail()));
                }
                (
                    Kind::Ligature {
                        font: af,
                        character: ac,
                        components: a,
                        left_boundary: al,
                        right_boundary: ar,
                    },
                    Kind::Ligature {
                        font: bf,
                        character: bc,
                        components: b,
                        left_boundary: bl,
                        right_boundary: br,
                    },
                ) => {
                    assert_eq!((af, ac, al, ar), (bf, bc, bl, br));
                    pending.push((a.head(), b.head()));
                    tails.push((a.tail(), b.tail()));
                }
                (
                    Kind::Discretionary {
                        pre: a,
                        post: ap,
                        replace_count: ac,
                    },
                    Kind::Discretionary {
                        pre: b,
                        post: bp,
                        replace_count: bc,
                    },
                ) => {
                    assert_eq!(ac, bc);
                    pending.push((a.head(), b.head()));
                    pending.push((ap.head(), bp.head()));
                    tails.extend([(a.tail(), b.tail()), (ap.tail(), bp.tail())]);
                }
                _ => assert_eq!(format!("{:?}", left.kind), format!("{:?}", right.kind)),
            }
            pairs.push((a, b));
            a = left.next;
            b = right.next;
        }
        assert!(b.is_null());
    }
    for (a, b) in tails {
        if a.is_null() {
            assert!(b.is_null());
        } else {
            assert!(pairs.contains(&(a, b)), "owned child tail was copied");
        }
    }
    let mut source: Vec<_> = pairs.iter().map(|(a, _)| a.0).collect();
    let mut copied: Vec<_> = pairs.iter().map(|(_, b)| b.0).collect();
    source.sort_unstable();
    copied.sort_unstable();
    assert!(source.windows(2).all(|pair| pair[0] != pair[1]));
    assert!(copied.windows(2).all(|pair| pair[0] != pair[1]));
    assert!(copied.iter().all(|id| source.binary_search(id).is_err()));
    pairs
}
fn live_nodes(store: &NodeStore) -> usize {
    store
        .pages
        .iter()
        .flatten()
        .map(|page| usize::from(page.data.as_ref().unwrap().live))
        .sum()
}

#[test]
fn semantic_copy_preserves_fields_and_has_independent_children_and_snapshots() {
    let mut store = NodeStore::default();
    let source = fixture(&mut store);
    let before = records(&store, source.head());
    let old = store.clone();
    let mut scratch = CopyScratch::default();
    let copy = store.copy_list(source.head(), &mut scratch).unwrap();
    let pairs = assert_copy(&store, source.head(), copy.head());
    assert!(pairs.contains(&(source.tail(), copy.tail())));
    assert_eq!(records(&store, source.head()), before);
    assert_eq!(records(&old, source.head()), before);
    assert!(scratch.frames.is_empty() && scratch.release.pending.is_empty());
    for (original, copied) in pairs {
        match store.get(copied).kind {
            Kind::Character { .. } => *store.kind_mut(copied) = glyph(99, 0),
            Kind::Box(_) => store.box_mut(copied).shift = Scaled(123),
            _ => continue,
        }
        assert_eq!(records(&store, source.head()), before);
        assert_eq!(records(&old, source.head()), before);
        assert_ne!(original, copied);
    }
    store.release(source, &mut scratch.release);
    assert!(!records(&store, copy.head()).is_empty());
    store.release(copy, &mut scratch.release);
    assert_eq!(live_nodes(&store), 0);
    assert_eq!(records(&old, before[0].0), before);
}

fn rejected_copy(budget: usize, retained: bool, retry: bool) {
    let mut store = NodeStore::default();
    let source = fixture(&mut store);
    let before = records(&store, source.head());
    let boxes = before
        .iter()
        .filter(|(id, _)| matches!(store.get(*id).kind, Kind::Box(_)))
        .count();
    let old = retained.then(|| store.clone());
    let filler_count = if retained {
        CELLS - budget
    } else {
        CELLS - before.len() - budget
    };
    let filler = penalties(&mut store, filler_count);
    let mut scratch = CopyScratch::default();
    assert!(matches!(
        copy_list(
            &mut store,
            source.head(),
            &mut scratch,
            if retained { 2 } else { 1 }
        ),
        Err(crate::Error::Capacity)
    ));
    assert!(scratch.frames.is_empty() && scratch.release.pending.is_empty());
    assert!(store.pool.failed_release.borrow().pending.is_empty());
    assert_eq!(records(&store, source.head()), before);
    assert_eq!(live_nodes(&store), before.len() + filler_count);
    if let Some(old) = &old {
        assert_eq!(records(old, source.head()), before);
    }
    store.release(filler, &mut scratch.release);
    if retry {
        let copy = store.copy_list(source.head(), &mut scratch).unwrap();
        assert_copy(&store, source.head(), copy.head());
        assert_eq!(
            store.boxes.len(),
            2 * boxes,
            "failed copy leaked no box slots"
        );
        store.release(copy, &mut scratch.release);
    }
    store.release(source, &mut scratch.release);
    assert_eq!(live_nodes(&store), 0);
    drop(store);
    if let Some(old) = old {
        assert_eq!(records(&old, before[0].0), before);
    }
}
#[test]
fn capacity_error_releases_attached_and_pending_child_forests() {
    // Six successes reach into discretionary pre contents, with a pending post
    // frame and already completed ligature components owned by the root frame.
    rejected_copy(6, true, false);
}
#[test]
fn every_node_and_box_allocation_rejection_preserves_source_and_reuses_slots() {
    let mut store = NodeStore::default();
    let source = fixture(&mut store);
    let count = records(&store, source.head()).len();
    store.release(source, &mut ReleaseScratch::default());
    for retained in [false, true] {
        for budget in 0..count {
            rejected_copy(budget, retained, true);
        }
    }
}
#[test]
fn repeated_copies_reuse_scratch_and_survive_either_owner_drop_order() {
    for source_first in [false, true] {
        let mut store = NodeStore::default();
        let source = fixture(&mut store);
        let before = records(&store, source.head());
        let mut scratch = CopyScratch::default();
        assert!(
            store
                .copy_list(NodeId::NULL, &mut scratch)
                .unwrap()
                .is_empty()
        );
        assert_eq!(scratch.frames.capacity(), 0);
        let first = store.copy_list(source.head(), &mut scratch).unwrap();
        let capacity = scratch.frames.capacity();
        let slots = store.boxes.len();
        store.release(first, &mut scratch.release);
        for _ in 0..8 {
            let copy = store.copy_list(source.head(), &mut scratch).unwrap();
            assert_copy(&store, source.head(), copy.head());
            store.release(copy, &mut scratch.release);
            assert_eq!(scratch.frames.capacity(), capacity);
            assert_eq!(store.boxes.len(), slots);
            assert_eq!(records(&store, source.head()), before);
        }
        let copy = store.copy_list(source.head(), &mut scratch).unwrap();
        let copied = records(&store, copy.head());
        let saved = store.clone();
        if source_first {
            store.release(source, &mut scratch.release);
            assert_eq!(records(&store, copy.head()), copied);
            store.release(copy, &mut scratch.release);
        } else {
            store.release(copy, &mut scratch.release);
            assert_eq!(records(&store, source.head()), before);
            store.release(source, &mut scratch.release);
        }
        assert_eq!(live_nodes(&store), 0);
        drop(store);
        assert_eq!(records(&saved, before[0].0), before);
        assert_eq!(records(&saved, copied[0].0), copied);
    }
}
#[test]
fn deep_copy_and_release_use_work_stacks_across_multiple_chunks() {
    let mut store = NodeStore::default();
    let mut source = penalties(&mut store, 3 * CELLS);
    for _ in 0..2048 {
        let parent = store.alloc_box(BoxData::new(source)).unwrap();
        source = List::default();
        store.append(&mut source, parent);
    }
    let mut scratch = CopyScratch::default();
    let copy = store.copy_list(source.head(), &mut scratch).unwrap();
    assert_eq!(
        assert_copy(&store, source.head(), copy.head()).len(),
        2048 + 3 * CELLS
    );
    store.release(source, &mut scratch.release);
    assert_eq!(records(&store, copy.head()).len(), 2048 + 3 * CELLS);
    store.release(copy, &mut scratch.release);
    assert!(scratch.frames.is_empty() && scratch.release.pending.is_empty());
    assert_eq!(live_nodes(&store), 0);
}
