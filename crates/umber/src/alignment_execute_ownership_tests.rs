use super::*;
use crate::nodes::List;

#[test]
fn partial_preamble_and_active_row_body_forests_survive_errors_until_restore() {
    for body in [
        r"\halign{abc\cr",
        r"\halign{#&#\cr\hbox{\kern2pt}&\undefined}",
        r"\halign{#\cr a\cr\noalign{\hbox{\kern2pt}\undefined}",
    ] {
        let source = format!(r"{BOOT}\font\f=test\f {SETTINGS}\setbox0=\hbox{{z}}{body}");
        let mut e = engine(source.as_bytes());
        let before = e.checkpoint().unwrap();
        let mut sizes = None;
        for _ in 0..3 {
            assert!(e.advance(usize::MAX).is_err());
            assert_eq!(e.document.alignment_frames.len(), 1);
            assert!(!e.document.boxes.head(0).is_null());
            let frame = e.document.alignment_frames.last().unwrap();
            if frame.phase == Phase::Preamble {
                assert!(frame.preamble.is_some());
            } else if e.current_group() == Some(GroupKind::NoAlign) {
                assert!(!e.document.text.vertical.is_empty());
                assert_eq!(e.abort_alignment_layout(), Err(Error::Group));
                assert_eq!(
                    e.finish_alignment_layout(PackSpec::Natural).err(),
                    Some(Error::Group)
                );
                assert_eq!(e.alignment_material(List::default()), Err(Error::Group));
            } else {
                assert!(frame.body.is_some());
            }
            let foreign = Engine::new(b"").checkpoint().unwrap();
            assert_eq!(e.restore(&foreign), Err(Error::Checkpoint));
            assert!(e.alignment_reader.active());
            e.restore(&before).unwrap();
            assert_eq!(e.document.nodes.live_count(), 0);
            let current = (
                e.document.alignment_frames.capacity(),
                e.alignment_execution_pool.len(),
                e.alignment_layout_pool.len(),
            );
            if let Some(sizes) = sizes {
                assert_eq!(current, sizes);
            } else {
                sizes = Some(current);
            }
        }
    }
}

#[test]
fn group_capacity_errors_root_unpublished_and_published_preambles() {
    for level in [u16::MAX, u16::MAX - 1] {
        let source = format!(r"{BOOT}\relax\halign{{#\cr a\cr}}");
        let mut e = engine(source.as_bytes());
        loop {
            let read = e.next_expanded().unwrap();
            if read.meaning.command == cmd::HALIGN {
                e.back(read.token).unwrap();
                break;
            }
            e.execute(read).unwrap();
        }
        let before = e.checkpoint().unwrap();
        e.document.level = level;
        assert_eq!(e.advance(usize::MAX), Err(Error::Capacity));
        assert_eq!(e.document.alignment_frames.len(), 1);
        assert!(e.alignment_reader.active());
        assert_eq!(
            e.document
                .alignment_frames
                .last()
                .unwrap()
                .preamble
                .is_some(),
            level == u16::MAX
        );
        assert_eq!(
            e.document.alignment_layouts.len(),
            usize::from(level == u16::MAX - 1)
        );
        e.restore(&before).unwrap();
        assert_eq!(e.document.nodes.live_count(), 0);
    }
}

#[test]
fn aftergroup_closer_reifies_exhausted_v_and_replays_pending_sources() {
    let source = format!(
        r"{BOOT}\font\f=test\f {SETTINGS}\let\egroup=}}\def\pending{{\message{{PENDING}}}}\halign{{#\cr a\aftergroup\egroup\aftergroup\pending\cr\relax\end"
    );
    let mut e = engine(source.as_bytes());
    while e.alignment_execution_pool.is_empty() {
        e.advance(1).unwrap();
    }
    assert!(e.can_checkpoint());
    assert!(
        e.document
            .sources
            .iter()
            .any(|s| s.role() == SourceRole::RetiredAlignmentV)
    );
    assert!(e.outcome().message_bytes().is_empty());
    let before = e.checkpoint().unwrap();
    for _ in 0..2 {
        assert_eq!(e.advance(usize::MAX), Ok(Progress::Complete));
        assert_eq!(e.outcome().message_bytes(), [b"PENDING".to_vec()]);
        assert_eq!(e.document.nodes.live_count(), 0);
        e.restore(&before).unwrap();
    }
}

#[test]
fn suspended_body_aux_and_parent_line_count_are_not_shadowed_by_cells() {
    let source = format!(
        r"{BOOT}\font\f=test\f {SETTINGS}\halign{{#\cr\noalign{{\noindent ab\par}}\message{{CELL}}a\cr}}\relax\end"
    );
    let mut e = engine(source.as_bytes());
    loop {
        let read = e.next_expanded().unwrap();
        if read.meaning.command == cmd::MESSAGE {
            e.back(read.token).unwrap();
            break;
        }
        e.execute(read).unwrap();
    }
    let frame = e.document.alignment_frames.last().unwrap();
    assert_eq!(frame.body.as_ref().unwrap().prev_graf, 1);
    assert_eq!(e.document.text.prev_graf, 0);
    assert_eq!(frame.outer.prev_graf, 0);
    assert!(e.document.page.current.is_empty());
    assert!(e.outcome().pages.is_empty());
    e.advance(usize::MAX).unwrap();
    assert_eq!(e.document.text.prev_graf, 0);
}
