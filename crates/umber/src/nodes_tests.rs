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
    let parent = store.alloc(Kind::Box(data)).unwrap();
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
        store.alloc_with_limit(Kind::Box(BoxData::new(children)), 1),
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
        let node = store.alloc(Kind::Box(BoxData::new(children))).unwrap();
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
            0 => Kind::Box(BoxData::new(children)),
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
    let parent = live.alloc(Kind::Box(BoxData::new(contents))).unwrap();
    let id = parent.id();
    let mut root = List::default();
    live.append(&mut root, parent);
    let old = live.clone();
    live.release(root, &mut ReleaseScratch::default());
    assert!(live.pages.iter().all(Option::is_none));
    let Kind::Box(boxed) = &old.get(id).kind else {
        panic!("box lost")
    };
    let Kind::Discretionary { pre, post, .. } = &old.get(boxed.list.head()).kind else {
        panic!("disc lost")
    };
    let Kind::Ligature { components, .. } = &old.get(pre.head()).kind else {
        panic!("ligature lost")
    };
    assert_eq!(values(&old, components).len(), CELLS + 10);
    assert_eq!(values(&old, post).len(), CELLS + 20);
}
