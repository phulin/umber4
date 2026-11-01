use super::*;
fn leaf(store: &mut NodeStore) -> List {
    let node = store.alloc(Kind::Penalty(17)).unwrap();
    let mut list = List::default();
    store.append(&mut list, node);
    list
}
fn live(store: &NodeStore) -> usize {
    store
        .pages
        .iter()
        .filter_map(Option::as_ref)
        .map(|p| usize::from(p.data.as_ref().unwrap().live))
        .sum()
}
#[test]
fn insertion_payload_capacity_failure_releases_transferred_children() {
    assert_eq!(std::mem::size_of::<Node>(), 24);
    let mut store = NodeStore::default();
    // Fill the only writable chunk, then force allocation to reject a new page.
    let mut root = leaf(&mut store);
    while live(&store) < CELLS {
        let node = store.alloc(Kind::Penalty(0)).unwrap();
        store.append(&mut root, node);
    }
    let child = store.pop_front(&mut root).unwrap();
    let children = child.into_list();
    assert!(matches!(
        store.alloc_insertion_with_limit(
            InsertionData {
                list: children,
                ..InsertionData::default()
            },
            0
        ),
        Err(Error::Capacity)
    ));
    assert_eq!(live(&store), CELLS - 1);
    store.release(root, &mut ReleaseScratch::default());
    assert_eq!(live(&store), 0);
}
#[test]
fn insertion_payload_copy_snapshot_slot_reuse_and_iterative_release() {
    let mut store = NodeStore::default();
    let mut root = leaf(&mut store);
    for i in 0..256 {
        let node = if i % 2 == 0 {
            store
                .alloc_insertion(InsertionData {
                    list: root,
                    class: 7,
                    float_cost: 19,
                    height: Scaled(21),
                    split_top: Glue {
                        width: Scaled(27),
                        stretch: Scaled(31),
                        ..Glue::default()
                    },
                    max_depth: Scaled(35),
                })
                .unwrap()
        } else {
            store.alloc(Kind::Adjustment(root)).unwrap()
        };
        root = node.into_list();
    }
    let mut scratch = CopyScratch::default();
    let copy = store.copy_list(root.head(), &mut scratch).unwrap();
    let (mut a, mut b) = (root.head(), copy.head());
    while !a.is_null() {
        assert_ne!(a, b);
        (a, b) = match (&store.get(a).kind, &store.get(b).kind) {
            (Kind::Insertion(a), Kind::Insertion(b)) => {
                assert_ne!(a.0, b.0);
                let (a, b) = (store.insertion_data(a), store.insertion_data(b));
                assert_eq!(
                    (a.class, a.float_cost, a.height, a.split_top, a.max_depth),
                    (b.class, b.float_cost, b.height, b.split_top, b.max_depth)
                );
                (a.list.head(), b.list.head())
            }
            (Kind::Adjustment(a), Kind::Adjustment(b)) => (a.head(), b.head()),
            (Kind::Penalty(a), Kind::Penalty(b)) => {
                assert_eq!(a, b);
                (NodeId::NULL, NodeId::NULL)
            }
            _ => panic!("semantic copy changed kind"),
        };
    }
    assert!(b.is_null());
    let expected = live(&store);
    assert_eq!(expected, 514);
    let saved = store.clone();
    let mut saved_root = root.snapshot();
    store.release(root, &mut ReleaseScratch::default());
    assert_eq!(live(&store), 257);
    store.release(copy, &mut ReleaseScratch::default());
    assert_eq!(live(&store), 0);
    let slots = store.insertions.len();
    for _ in 0..8 {
        let children = leaf(&mut store);
        let node = store
            .alloc_insertion(InsertionData {
                list: children,
                ..InsertionData::default()
            })
            .unwrap();
        store.release_node(node, &mut ReleaseScratch::default());
        assert_eq!(store.insertions.len(), slots);
    }
    let mut saved = saved;
    let head = saved_root.head();
    let node = saved.pop_front(&mut saved_root).unwrap();
    assert_eq!(head, node.id());
    saved.release_node(node, &mut ReleaseScratch::default());
    assert_eq!(live(&saved), 257); // The independently owned semantic copy remains.
    drop(saved);
    drop(store);
}

#[test]
fn insertion_payload_rejected_semantic_copy_cleans_partial_children() {
    for retained in [false, true] {
        for budget in 0..4 {
            let mut store = NodeStore::default();
            let children = leaf(&mut store);
            let node = store
                .alloc_insertion(InsertionData {
                    list: children,
                    class: 9,
                    ..InsertionData::default()
                })
                .unwrap();
            let source = node.into_list();
            let old = retained.then(|| store.clone());
            let mut filler = List::default();
            let count = if retained {
                CELLS - budget
            } else {
                CELLS - 2 - budget
            };
            for _ in 0..count {
                let node = store.alloc(Kind::Penalty(0)).unwrap();
                store.append(&mut filler, node);
            }
            let before = live(&store);
            let mut scratch = CopyScratch::default();
            let result = copying::copy_list(
                &mut store,
                source.head(),
                &mut scratch,
                if retained { 2 } else { 1 },
            );
            match result {
                Ok(copy) => {
                    assert!(budget >= 2);
                    store.release(copy, &mut ReleaseScratch::default());
                }
                Err(Error::Capacity) => assert!(budget < 2),
                Err(error) => panic!("unexpected {error:?}"),
            }
            assert_eq!(live(&store), before);
            let Kind::Insertion(id) = &store.get(source.head()).kind else {
                panic!()
            };
            assert_eq!(store.insertion_data(id).class, 9);
            store.release(source, &mut ReleaseScratch::default());
            store.release(filler, &mut ReleaseScratch::default());
            assert_eq!(live(&store), 0);
            drop(old);
        }
    }
}
