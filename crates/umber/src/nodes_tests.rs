use super::*;
fn leaf(store: &mut NodeStore, value: i32) -> DetachedNode {
    store.alloc(Kind::Penalty(value)).unwrap()
}
fn list(store: &mut NodeStore, size: usize) -> List {
    let mut list = List::default();
    for index in 0..size {
        let node = leaf(store, index as i32);
        store.append(&mut list, node);
    }
    list
}
fn values(store: &NodeStore, list: &List) -> Vec<i32> {
    let mut next = list.head();
    let mut out = Vec::new();
    while !next.is_null() {
        let node = store.get(next);
        if let Kind::Penalty(value) = &node.kind {
            out.push(*value);
        }
        next = node.next;
    }
    out
}
#[test]
fn boundary_range_rejection_releases_original_and_preserved_effects() {
    for op in [0, 1, 128] {
        let bytes = crate::font_test_data::tfm(
            &[
                [255, 200, 0, 1],
                [128, 200, op, if op == 128 { 0 } else { b'c' }],
                [255, 0, 0, 1],
            ],
            &[],
        );
        let mut e = crate::Engine::new(br"\font\f=test \f\relax");
        e.register_tfm(b"test", &bytes).unwrap();
        e.advance(2).unwrap();
        e.document.text.mode = crate::text::Mode::Horizontal;
        e.character_run(200).unwrap();
        let list = std::mem::take(&mut e.document.text.horizontal);
        e.document
            .nodes
            .release(list, &mut ReleaseScratch::default());
        assert!(
            e.document.nodes.pages.iter().all(Option::is_none),
            "opcode {op}"
        );
    }
}

#[test]
fn box_register_replacement_global_overrides_and_saved_owners_reclaim_all_nodes() {
    let mut source = String::from("\\catcode123=1\\catcode125=2\\endlinechar=-1\\relax\n");
    for _ in 0..64 {
        source.push_str("\\setbox1=\\hbox{\\hbox{\\kern1pt}}{\\setbox1=\\hbox{\\hbox{\\kern2pt}}{\\global\\setbox1=\\copy1}}\\setbox1=\\box255\n");
        source.push_str("{\\setbox0=\\hbox{\\hbox{\\kern1pt}}\\global\\setbox0=\\copy0\\global\\setbox0=\\box255}\n");
    }
    source.push_str("\\end");
    let mut engine = crate::Engine::new(source.as_bytes());
    engine.run_input().unwrap();
    assert!(engine.document.nodes.pages.iter().all(Option::is_none));
    assert!(engine.document.nodes.boxes.len() <= 6);
    assert!(engine.document.box_frames.is_empty());
    assert!(engine.document.save.is_empty());
    assert!(engine.document.nodes.pool.free.borrow().len() <= CACHED);
}

#[test]
fn restricted_box_nesting_and_reclamation_use_owned_work_stacks() {
    let mut source =
        String::from("\\catcode123=1\\catcode125=2\\endlinechar=-1\\relax\n\\setbox0=\\hbox{\n");
    for _ in 0..2048 {
        source.push_str("\\hbox{\n");
    }
    source.push_str("\\kern1pt\n");
    for _ in 0..2049 {
        source.push_str("}\n");
    }
    source.push_str("\\count0=1\\relax\\setbox0=\\box255\\end");
    let mut engine = crate::Engine::new(source.as_bytes());
    while engine.outcome().counts[0] != 1 {
        engine.advance(1).unwrap();
    }
    assert!(engine.document.box_frames.is_empty());
    assert_eq!(engine.document.level, 0);
    let live: usize = engine
        .document
        .nodes
        .pages
        .iter()
        .flatten()
        .map(|page| usize::from(page.data.as_ref().unwrap().live))
        .sum();
    assert_eq!(live, 2050);
    engine.run_input().unwrap();
    assert!(engine.document.nodes.pages.iter().all(Option::is_none));
}

#[test]
fn unboxing_reclaims_consumed_headers_and_only_retains_transferred_children() {
    let mut source = String::from("\\catcode123=1\\catcode125=2\\endlinechar=-1\\relax\n");
    for _ in 0..32 {
        source.push_str("\\setbox0=\\vbox{\\hbox{\\kern1pt}}\\setbox1=\\vbox{\\unvcopy0\\unvbox0\\global\\setbox2=\\lastbox}\\setbox3=\\hbox{\\unhcopy2\\unhbox2}\\setbox1=\\box255\\setbox3=\\box255\n");
    }
    source.push_str("\\end");
    let mut engine = crate::Engine::new(source.as_bytes());
    engine.run_input().unwrap();
    assert!(engine.document.nodes.pages.iter().all(Option::is_none));
    assert!(engine.document.nodes.boxes.len() <= 8);
}

#[test]
fn transfer_split_pop_and_nested_release_reuse_backing() {
    let mut store = NodeStore::default();
    let mut scratch = ReleaseScratch::default();
    let mut a = list(&mut store, 3);
    let second = store.get(a.head()).next;
    let suffix = store.split_after(&mut a, second);
    assert_eq!(values(&store, &a), [0, 1]);
    assert_eq!(values(&store, &suffix), [2]);
    store.splice(&mut a, suffix);
    let first = store.pop_front(&mut a).unwrap();
    store.release_node(first, &mut scratch);
    let data = BoxData::new(a);
    let parent = store.alloc_box(data).unwrap();
    store.release_node(parent, &mut scratch);
    assert!(scratch.pending.is_empty());
    assert!(store.pages.iter().all(Option::is_none));
    let pointer = Rc::as_ptr(store.pool.free.borrow().last().unwrap());
    let next = leaf(&mut store, 9);
    assert_eq!(pointer, Rc::as_ptr(store.page(0)));
    store.release_node(next, &mut scratch);
}
#[test]
fn old_versions_survive_link_mutation_free_and_reuse() {
    let mut live = NodeStore::default();
    let mut chain = list(&mut live, CELLS + 3);
    let old_list = chain.snapshot();
    let old = live.clone();
    let old_pointer = Rc::as_ptr(old.page(0));
    let first = live.pop_front(&mut chain).unwrap();
    let first_id = first.id();
    assert_ne!(old_pointer, Rc::as_ptr(live.page(0)));
    let copied = Rc::as_ptr(live.page(0));
    live.get_mut(chain.head()).kind = Kind::Penalty(77);
    assert_eq!(
        copied,
        Rc::as_ptr(live.page(0)),
        "unique edits do not copy again"
    );
    let mut scratch = ReleaseScratch::default();
    live.release_node(first, &mut scratch);
    let new = leaf(&mut live, 999);
    assert_eq!(new.id(), first_id);
    live.append(&mut chain, new);
    assert_eq!(
        values(&old, &old_list),
        (0..(CELLS + 3) as i32).collect::<Vec<_>>()
    );
    assert_eq!(values(&live, &chain)[0], 77);
    drop(live);
    assert_eq!(values(&old, &old_list).len(), CELLS + 3);
}
#[test]
fn appending_after_snapshot_uses_fresh_chunk_without_copying_shared_tail() {
    let mut live = NodeStore::default();
    let mut chain = list(&mut live, 2);
    let old = live.clone();
    let old_pointer = Rc::as_ptr(old.page(0));
    let detached = leaf(&mut live, 2);
    assert_eq!(
        old_pointer,
        Rc::as_ptr(live.page(0)),
        "allocation alone leaves shared tail intact"
    );
    assert_ne!(NodeStore::address(detached.id()).0, 0);
    live.append(&mut chain, detached);
    assert!(old.get(NodeId(2)).next.is_null());
}
#[test]
fn ligature_and_discretionary_children_release_and_capacity_failure_cleans_up() {
    let mut store = NodeStore::default();
    let mut scratch = ReleaseScratch::default();
    let components = list(&mut store, 3);
    let lig = store
        .alloc(Kind::ligature(1, b'f', components, true, false))
        .unwrap();
    let mut pre = List::default();
    store.append(&mut pre, lig);
    let post = list(&mut store, 2);
    let disc = store.alloc(Kind::discretionary(pre, post, 0)).unwrap();
    store.release_node(disc, &mut scratch);
    let children = list(&mut store, CELLS);
    let weak = Rc::downgrade(&store.pool);
    assert!(matches!(
        store.alloc_box_with_limit(BoxData::new(children), 1),
        Err(Error::Capacity)
    ));
    assert!(store.pages.iter().all(Option::is_none));
    assert!(store.pool.failed_release.borrow().pending.is_empty());
    drop(store);
    assert!(weak.upgrade().is_none());
}
#[test]
fn deeply_nested_lists_release_iteratively_and_old_roots_outlive_live_store() {
    let mut store = NodeStore::default();
    let mut children = List::default();
    for _ in 0..2048 {
        let node = store.alloc_box(BoxData::new(children)).unwrap();
        children = List::default();
        store.append(&mut children, node);
    }
    let old_list = children.snapshot();
    let old = store.clone();
    let pool = Rc::downgrade(&store.pool);
    store.release(children, &mut ReleaseScratch::default());
    drop(store);
    assert!(!old_list.is_empty());
    assert!(matches!(old.get(old_list.head()).kind, Kind::Box(_)));
    drop(old);
    assert!(pool.upgrade().is_none(), "pool has no retention cycle");
}

#[test]
fn rejected_owned_payloads_free_live_children_without_touching_old_versions() {
    for form in 0..3 {
        let mut live = NodeStore::default();
        let mut children = list(&mut live, CELLS);
        let old_children = children.snapshot();
        let old = live.clone();
        let kind = match form {
            0 => Kind::Box(live.boxes.alloc(BoxData::new(children))),
            1 => Kind::ligature(3, b'f', children, false, true),
            _ => {
                let mut midpoint = children.head();
                for _ in 1..CELLS / 2 {
                    midpoint = live.get(midpoint).next;
                }
                let post = live.split_after(&mut children, midpoint);
                Kind::discretionary(children, post, 0)
            }
        };
        assert!(matches!(
            live.alloc_with_limit(kind, 1),
            Err(Error::Capacity)
        ));
        assert!(live.pages.iter().all(Option::is_none));
        assert_eq!(
            values(&old, &old_children),
            (0..CELLS as i32).collect::<Vec<_>>()
        );
        // Both the live and retained root can be the first one to disappear.
        if form == 0 {
            drop(live);
            assert_eq!(values(&old, &old_children).len(), CELLS);
        } else {
            drop(old);
            let new = list(&mut live, CELLS + 1);
            live.release(new, &mut ReleaseScratch::default());
        }
    }
}

#[test]
fn mixed_owned_children_span_chunks_and_release_without_recursion() {
    let mut live = NodeStore::default();
    let components = list(&mut live, CELLS + 10);
    let lig = live
        .alloc(Kind::ligature(1, b'f', components, false, false))
        .unwrap();
    let mut pre = List::default();
    live.append(&mut pre, lig);
    let post = list(&mut live, CELLS + 20);
    let disc = live.alloc(Kind::discretionary(pre, post, 0)).unwrap();
    let mut contents = List::default();
    live.append(&mut contents, disc);
    let parent = live.alloc_box(BoxData::new(contents)).unwrap();
    let id = parent.id();
    let mut root = List::default();
    live.append(&mut root, parent);
    let old = live.clone();
    live.release(root, &mut ReleaseScratch::default());
    assert!(live.pages.iter().all(Option::is_none));
    let Kind::Box(boxed) = &old.get(id).kind else {
        panic!("box lost")
    };
    let boxed = old.box_data(boxed);
    let Kind::Discretionary { pre, post, .. } = &old.get(boxed.list.head()).kind else {
        panic!("disc lost")
    };
    let Kind::Ligature { components, .. } = &old.get(pre.head()).kind else {
        panic!("ligature lost")
    };
    assert_eq!(values(&old, components).len(), CELLS + 10);
    assert_eq!(values(&old, post).len(), CELLS + 20);
}

fn box_id(store: &NodeStore, node: NodeId) -> &BoxId {
    let Kind::Box(id) = &store.get(node).kind else {
        panic!("box lost")
    };
    id
}
#[test]
fn box_payload_mutation_and_slot_reuse_preserve_snapshot_children() {
    let mut live = NodeStore::default();
    let contents = list(&mut live, CELLS + 3);
    let parent = live
        .alloc_box(BoxData {
            list: contents,
            width: Scaled(77),
            ..BoxData::default()
        })
        .unwrap();
    let node = parent.id();
    let slot = box_id(&live, node).0;
    let mut root = List::default();
    live.append(&mut root, parent);
    let old_root = root.snapshot();
    let mut old = live.clone();
    let page = NodeStore::address(node).0;
    let shared = Rc::as_ptr(old.page(page));
    live.box_mut(node).shift = Scaled(-9);
    assert_eq!(
        shared,
        Rc::as_ptr(live.page(page)),
        "header mutation preserves node sharing"
    );
    assert_eq!(old.box_data(box_id(&old, node)).shift, Scaled(0));
    assert_eq!(live.box_data(box_id(&live, node)).shift, Scaled(-9));
    live.release(root, &mut ReleaseScratch::default());
    let replacement = live.alloc_box(BoxData::default()).unwrap();
    assert_eq!(box_id(&live, replacement.id()).0, slot);
    let data = old.box_data(box_id(&old, node));
    assert_eq!(data.width, Scaled(77));
    assert_eq!(
        values(&old, &data.list),
        (0..(CELLS + 3) as i32).collect::<Vec<_>>()
    );
    assert!(
        live.box_data(box_id(&live, replacement.id()))
            .list
            .is_empty()
    );
    live.release_node(replacement, &mut ReleaseScratch::default());
    drop(live);
    old.release(old_root, &mut ReleaseScratch::default());
    assert!(old.pages.iter().all(Option::is_none));
}
#[test]
fn box_capacity_rejection_precedes_payload_growth_and_releases_nested_children() {
    let mut live = NodeStore::default();
    assert!(matches!(
        live.alloc_box_with_limit(BoxData::default(), 0),
        Err(Error::Capacity)
    ));
    assert_eq!(live.boxes.len(), 0);
    let child = leaf(&mut live, 1);
    let mut contents = List::default();
    live.append(&mut contents, child);
    let child = live.alloc_box(BoxData::new(contents)).unwrap();
    let mut contents = List::default();
    live.append(&mut contents, child);
    let fillers = list(&mut live, CELLS - 2);
    let old_contents = contents.snapshot();
    let old = live.clone();
    assert_eq!(live.boxes.len(), 1);
    assert!(matches!(
        live.alloc_box_with_limit(BoxData::new(contents), 1),
        Err(Error::Capacity)
    ));
    assert_eq!(live.boxes.len(), 1, "rejection allocates no outer payload");
    let old_child = old.box_data(box_id(&old, old_contents.head()));
    assert_eq!(values(&old, &old_child.list), [1]);
    live.release(fillers, &mut ReleaseScratch::default());
    assert!(live.pages.iter().all(Option::is_none));
    assert!(live.pool.failed_release.borrow().pending.is_empty());
    let empty = live.alloc_box(BoxData::default()).unwrap();
    assert_eq!(
        box_id(&live, empty.id()).0,
        1,
        "rejected nested box slot is reusable"
    );
    live.release_node(empty, &mut ReleaseScratch::default());
}
