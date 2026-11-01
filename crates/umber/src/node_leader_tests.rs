use super::*;
fn leaf(store: &mut NodeStore) -> List {
    let node = store
        .alloc(Kind::Rule {
            width: Scaled(3),
            height: Scaled(2),
            depth: Scaled(1),
        })
        .unwrap();
    node.into_list()
}
#[test]
fn leader_payload_rejection_reclaims_template_without_growing_slots() {
    assert_eq!(std::mem::size_of::<Node>(), 24);
    let mut store = NodeStore::default();
    let template = leaf(&mut store);
    let mut filler = List::default();
    for _ in 1..CELLS {
        let node = store.alloc(Kind::Penalty(0)).unwrap();
        store.append(&mut filler, node);
    }
    assert!(matches!(
        store.alloc_leader_with_limit(
            LeaderData {
                list: template,
                spec: Glue::default(),
                subtype: GlueKind::AlignedLeaders
            },
            0
        ),
        Err(Error::Capacity)
    ));
    assert_eq!(store.leaders.len(), 0);
    assert_eq!(store.live_count(), CELLS - 1);
    store.release(filler, &mut ReleaseScratch::default());
    assert_eq!(store.live_count(), 0);
}
#[test]
fn leader_payload_copy_cow_demotion_and_iterative_release() {
    let mut store = NodeStore::default();
    let mut root = leaf(&mut store);
    for _ in 0..128 {
        let b = store
            .alloc_box(BoxData {
                list: root,
                width: Scaled(3),
                height: Scaled(2),
                depth: Scaled(1),
                ..BoxData::default()
            })
            .unwrap();
        let leader = store
            .alloc_leader(LeaderData {
                list: b.into_list(),
                spec: Glue {
                    width: Scaled(31),
                    ..Glue::default()
                },
                subtype: GlueKind::ExpandedLeaders,
            })
            .unwrap();
        root = leader.into_list();
    }
    let head = root.head();
    let copy = store.copy_list(head, &mut CopyScratch::default()).unwrap();
    let copy_head = copy.head();
    let saved = store.clone();
    store.set_glue(
        copy_head,
        Glue {
            width: Scaled(47),
            ..Glue::default()
        },
        GlueKind::RightSkip,
    );
    assert_eq!(
        store.glue(&store.get(head).kind).unwrap().0.width,
        Scaled(31)
    );
    assert_eq!(
        saved.glue(&saved.get(copy_head).kind).unwrap().0.width,
        Scaled(31)
    );
    let Kind::Leader(id) = &store.get(copy_head).kind else {
        panic!()
    };
    assert!(!store.leader_data(id).list.is_empty());
    assert_eq!(store.leader_dimensions(id), Default::default());
    store.replace_kind(
        copy_head,
        Kind::Penalty(10000),
        &mut ReleaseScratch::default(),
    );
    assert_eq!(store.live_count(), 258); // Original257 + replacement penalty.
    store.release(copy, &mut ReleaseScratch::default());
    store.release(root, &mut ReleaseScratch::default());
    assert_eq!(store.live_count(), 0);
    let high_water = store.leaders.len();
    for _ in 0..16 {
        let template = leaf(&mut store);
        let node = store
            .alloc_leader(LeaderData {
                list: template,
                ..LeaderData::default()
            })
            .unwrap();
        store.release_node(node, &mut ReleaseScratch::default());
        assert_eq!(store.leaders.len(), high_water);
    }
    assert_eq!(saved.live_count(), 514);
    drop(store);
    drop(saved);
}
#[test]
fn leader_payload_failed_semantic_copy_releases_partial_forest() {
    for retained in [false, true] {
        for budget in 0..4 {
            let mut store = NodeStore::default();
            let template = leaf(&mut store);
            let b = store.alloc_box(BoxData::new(template)).unwrap();
            let node = store
                .alloc_leader(LeaderData {
                    list: b.into_list(),
                    ..LeaderData::default()
                })
                .unwrap();
            let source = node.into_list();
            let old = retained.then(|| store.clone());
            let count = if retained {
                CELLS - budget
            } else {
                CELLS - 3 - budget
            };
            let mut filler = List::default();
            for _ in 0..count {
                let node = store.alloc(Kind::Penalty(0)).unwrap();
                store.append(&mut filler, node);
            }
            let before = store.live_count();
            let result = copying::copy_list(
                &mut store,
                source.head(),
                &mut CopyScratch::default(),
                if retained { 2 } else { 1 },
            );
            match result {
                Ok(copy) => {
                    assert_eq!(budget, 3);
                    store.release(copy, &mut ReleaseScratch::default());
                }
                Err(Error::Capacity) => assert!(budget < 3),
                Err(error) => panic!("{error:?}"),
            }
            assert_eq!(store.live_count(), before);
            store.release(source, &mut ReleaseScratch::default());
            store.release(filler, &mut ReleaseScratch::default());
            assert_eq!(store.live_count(), 0);
            drop(old);
        }
    }
}
