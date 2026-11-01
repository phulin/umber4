use super::*;
#[test]
fn alignment_layout_unset_rejects_copy_pack_and_publication() {
    assert_eq!(std::mem::size_of::<crate::nodes::Node>(), 24);
    let mut e = engine();
    e.begin_alignment_layout(&[Glue::default(); 2]).unwrap();
    e.begin_alignment_row().unwrap();
    let list = items(&mut e, &[Item::Rule(3, 5, 2)]);
    e.alignment_cell(list, 1).unwrap();
    let head = e.document.alignment_layouts.last().unwrap().row.head();
    let cell = e.document.nodes.get(head).next;
    assert!(matches!(
        e.document.nodes.copy_list(cell, &mut e.box_copy),
        Err(Error::Unsupported)
    ));
    e.abort_alignment_layout().unwrap();
    assert_eq!(e.document.nodes.live_count(), 0);
    for mode in 0..3 {
        let list = items(&mut e, &[Item::Rule(3, 5, 2)]);
        let box_ = e.hpack(list, PackSpec::Natural).unwrap();
        let unset = e.document.nodes.mark_unset(box_);
        if mode == 0 {
            assert_eq!(e.publish_page(unset), Err(Error::Unsupported));
        } else {
            let mut list = List::default();
            e.document.nodes.append(&mut list, unset);
            let result = if mode == 1 {
                e.hpack(list, PackSpec::Natural)
            } else {
                e.vpackage(list, PackSpec::Natural, Scaled::MAX)
            };
            assert!(matches!(result, Err(Error::Unsupported)));
        }
        assert_eq!(e.document.nodes.live_count(), 0);
    }
}

fn capacities(e: &Engine<'_>, depth: usize) -> (usize, usize, usize) {
    let buffers = e.alignment_layout_pool[depth].as_ref().unwrap();
    (
        buffers.columns.capacity(),
        buffers.spans.capacity(),
        buffers.cells.capacity(),
    )
}
#[test]
fn alignment_layout_nested_and_sequential_owners_reuse_distinct_depth_buffers() {
    let mut e = engine();
    let inner = Fixture {
        tabs: vec![Glue::default(); 3],
        rows: vec![row(vec![entry(2, 19)]), row(vec![entry(1, 3), entry(1, 5)])],
        spec: PackSpec::Natural,
    };
    let mut retained = None;
    for _ in 0..4 {
        e.document.text.prev_depth = Scaled(-65_536_000);
        e.begin_alignment_layout(&[Glue::default(); 3]).unwrap();
        e.begin_alignment_row().unwrap();
        let list = items(&mut e, &[Item::Rule(7, 3, 1)]);
        e.alignment_cell(list, 1).unwrap();
        let original = e.document.alignment_layouts[0].buffers.cells[0].node;
        let outer_depth = e.document.text.prev_depth;
        e.document.text.prev_depth = Scaled(-65_536_000);
        let nested = build(&mut e, &inner);
        e.document.text.prev_depth = outer_depth;
        assert_eq!(
            e.document.alignment_layouts[0].buffers.cells[0].node,
            original
        );
        assert_eq!(e.document.alignment_layouts[0].buffers.cells.len(), 1);
        let b = e.vpackage(nested, PackSpec::Natural, Scaled::MAX).unwrap();
        let mut contents = List::default();
        e.document.nodes.append(&mut contents, b);
        e.alignment_cell(contents, 1).unwrap();
        e.end_alignment_row().unwrap();
        let list = e.finish_alignment_layout(PackSpec::Natural).unwrap();
        e.document.nodes.release(list, &mut e.page_release);
        assert_eq!(e.document.nodes.live_count(), 0);
        let now = (capacities(&e, 0), capacities(&e, 1));
        if let Some(previous) = retained {
            assert_eq!(previous, now);
        }
        retained = Some(now);
    }
}
#[test]
fn alignment_layout_cow_unset_headers_and_completed_roots_are_independent() {
    let mut e = engine();
    e.begin_alignment_layout(&[Glue::default(); 3]).unwrap();
    e.begin_alignment_row().unwrap();
    let list = items(&mut e, &[Item::Rule(11, 7, 2)]);
    e.alignment_cell(list, 2).unwrap();
    e.end_alignment_row().unwrap();
    let row = e.document.alignment_layouts[0].body.head();
    let old_body = e.document.alignment_layouts[0].body.snapshot();
    let mut old = e.document.nodes.clone();
    assert!(matches!(old.get(row).kind, Kind::Unset(_)));
    let list = e.finish_alignment_layout(PackSpec::To(Scaled(19))).unwrap();
    assert!(matches!(e.document.nodes.get(row).kind, Kind::Box(_)));
    let Kind::Unset(id) = &old.get(row).kind else {
        panic!()
    };
    assert_eq!(old.box_data(id).width, Scaled(11));
    let copy = e
        .document
        .nodes
        .copy_list(list.head(), &mut e.box_copy)
        .unwrap();
    e.document.nodes.release(list, &mut e.page_release);
    e.document.nodes.release(copy, &mut e.page_release);
    assert_eq!(e.document.nodes.live_count(), 0);
    old.release(old_body, &mut crate::nodes::ReleaseScratch::default());
    assert_eq!(old.live_count(), 0);
}
#[test]
fn alignment_layout_rejects_partial_cell_row_baseline_and_placeholder_allocations() {
    // Cell-header rejection also tests adjustment extraction cleanup.
    let mut e = engine();
    e.begin_alignment_layout(&[Glue::default(); 2]).unwrap();
    e.begin_alignment_row().unwrap();
    let contents = items(
        &mut e,
        &[
            Item::Rule(3, 4, 1),
            Item::Adjustment(vec![Item::Rule(5, 6, 0)]),
        ],
    );
    e.document.nodes.reject_after_allocations(0);
    assert_eq!(e.alignment_cell(contents, 1), Err(Error::Capacity));
    e.abort_alignment_layout().unwrap();
    assert_eq!(e.document.nodes.live_count(), 0);
    for budget in 0..=1 {
        let mut e = engine();
        e.document.text.prev_depth = Scaled::ZERO;
        e.begin_alignment_layout(&[Glue::default(); 2]).unwrap();
        e.begin_alignment_row().unwrap();
        let contents = items(&mut e, &[Item::Rule(3, 4, 1)]);
        e.alignment_cell(contents, 1).unwrap();
        e.document.nodes.reject_after_allocations(budget);
        assert_eq!(e.end_alignment_row(), Err(Error::Capacity));
        assert_eq!(e.document.text.prev_depth, Scaled::ZERO);
        e.abort_alignment_layout().unwrap();
        assert_eq!(e.document.nodes.live_count(), 0);
    }
    for budget in 0..4 {
        let mut e = engine();
        e.begin_alignment_layout(&[Glue::default(); 4]).unwrap();
        e.begin_alignment_row().unwrap();
        let contents = items(&mut e, &[Item::Rule(17, 4, 1)]);
        e.alignment_cell(contents, 3).unwrap();
        e.end_alignment_row().unwrap();
        e.document.nodes.reject_after_allocations(budget);
        assert!(matches!(
            e.finish_alignment_layout(PackSpec::Natural),
            Err(Error::Capacity)
        ));
        e.abort_alignment_layout().unwrap();
        assert_eq!(e.document.nodes.live_count(), 0);
    }
}
#[test]
fn alignment_layout_restore_abandons_active_and_executing_body_roots() {
    let mut e = engine();
    let checkpoint = e.checkpoint().unwrap();
    for body_out in [false, true] {
        for _ in 0..2 {
            e.begin_alignment_layout(&[Glue::default(); 3]).unwrap();
            e.begin_alignment_row().unwrap();
            let contents = items(&mut e, &[Item::Rule(17, 4, 1)]);
            e.alignment_cell(contents, 2).unwrap();
            e.end_alignment_row().unwrap();
            if body_out {
                e.document.text.vertical = e.take_alignment_body().unwrap();
                e.document.text.mode = crate::text::Mode::InternalVertical;
            }
            e.begin_alignment_layout(&[Glue::default(); 2]).unwrap();
            e.begin_alignment_row().unwrap();
            let contents = items(&mut e, &[Item::Rule(3, 2, 1)]);
            e.alignment_cell(contents, 1).unwrap();
            assert!(!e.can_checkpoint());
            assert!(matches!(e.checkpoint(), Err(Error::Checkpoint)));
            e.restore(&checkpoint).unwrap();
            assert!(e.document.alignment_layouts.is_empty());
            assert!(e.can_checkpoint());
            assert_eq!(e.document.nodes.live_count(), 0);
            assert!(e.alignment_layout_pool.iter().all(Option::is_some));
        }
    }
}
