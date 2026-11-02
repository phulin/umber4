//! Local lowering frames borrow the still-owned tree, including on early exit.
use super::*;
use crate::{
    Placement, font_test_data,
    nodes::{GlueKind, LeaderData, List},
    numeric::Scaled,
};

fn append(e: &mut Engine<'_>, list: &mut List, kind: Kind) -> NodeId {
    let node = e.document.nodes.alloc(kind).unwrap();
    let id = node.id();
    e.document.nodes.append(list, node);
    id
}
fn boxed(e: &mut Engine<'_>, list: List) -> DetachedNode {
    e.document
        .nodes
        .alloc_box(BoxData {
            list,
            width: Scaled(20),
            height: Scaled(2),
            ..BoxData::default()
        })
        .unwrap()
}
#[test]
fn lowering_local_error_with_suspended_parent_and_repeat_keeps_one_tree_owner() {
    let mut e = Engine::new(br"\font\f=test\f\end");
    e.register_tfm(b"test", &font_test_data::basic()).unwrap();
    e.advance(usize::MAX).unwrap();
    let mut leaf = List::default();
    append(
        &mut e,
        &mut leaf,
        Kind::Character {
            font: 1,
            character: b'a',
        },
    );
    let bad = append(
        &mut e,
        &mut leaf,
        Kind::Character {
            font: 1,
            character: 0,
        },
    );
    let leaf = boxed(&mut e, leaf);
    let mut template = List::default();
    e.document.nodes.append(&mut template, leaf);
    append(
        &mut e,
        &mut template,
        Kind::Character {
            font: 1,
            character: b'b',
        },
    );
    let template = boxed(&mut e, template);
    let mut owner = List::default();
    e.document.nodes.append(&mut owner, template);
    let leader = e
        .document
        .nodes
        .alloc_leader(LeaderData {
            list: owner,
            spec: Glue {
                width: Scaled(60),
                ..Glue::default()
            },
            subtype: GlueKind::AlignedLeaders,
        })
        .unwrap();
    let mut root = List::default();
    append(
        &mut e,
        &mut root,
        Kind::Character {
            font: 1,
            character: b'A',
        },
    );
    e.document.nodes.append(&mut root, leader);
    append(
        &mut e,
        &mut root,
        Kind::Character {
            font: 1,
            character: b'y',
        },
    );
    let empty = boxed(&mut e, List::default());
    e.document.nodes.append(&mut root, empty);
    append(
        &mut e,
        &mut root,
        Kind::Character {
            font: 1,
            character: b'z',
        },
    );
    let root = boxed(&mut e, root);
    e.document.nodes.append(&mut e.document.text.vertical, root);
    let live = e.document.nodes.live_count();
    let checkpoint = e.checkpoint().unwrap();
    let root = e
        .document
        .nodes
        .pop_front(&mut e.document.text.vertical)
        .unwrap();
    assert_eq!(e.publish_page(root), Err(Error::Font));
    assert_eq!(e.document.nodes.live_count(), 0);
    assert!(e.document.page.output.is_empty());
    assert!(e.page_lowering.as_ref().unwrap().frames.is_empty());
    let mut old: Option<crate::Outcome> = None;
    for _ in 0..2 {
        e.restore(&checkpoint).unwrap();
        assert_eq!(e.document.nodes.live_count(), live);
        *e.document.nodes.kind_mut(bad) = Kind::Character {
            font: 1,
            character: b'g',
        };
        let root = e
            .document
            .nodes
            .pop_front(&mut e.document.text.vertical)
            .unwrap();
        e.publish_page(root).unwrap();
        assert_eq!(e.document.nodes.live_count(), 0);
        assert!(e.page_lowering.as_ref().unwrap().frames.is_empty());
        let output = e.outcome();
        let page = output.pages.get(0).unwrap();
        let glyphs: Vec<_> = page
            .placements()
            .filter_map(|p| match p {
                Placement::Glyph { character, .. } => Some(character),
                _ => None,
            })
            .collect();
        assert_eq!(glyphs, b"Aagbagbagbyz");
        assert_eq!(page.boxes().len(), 8);
        assert_eq!(page.boxes().last().unwrap().parent, Some(0));
        assert_eq!(page.items().len(), 9); // The empty box splits y and z runs.
        if let Some(previous) = &old {
            assert_eq!(page.boxes(), previous.pages.get(0).unwrap().boxes());
            assert!(
                page.placements()
                    .eq(previous.pages.get(0).unwrap().placements())
            );
        }
        old = Some(output);
    }
}

#[test]
fn lowering_deep_terminal_children_keep_all_box_events_and_stay_iterative() {
    let mut e = Engine::new(b"");
    let mut list = List::default();
    append(
        &mut e,
        &mut list,
        Kind::Rule {
            width: Scaled(7),
            height: Scaled(9),
            depth: Scaled::ZERO,
        },
    );
    let depth = if cfg!(miri) { 32 } else { 4096 };
    for _ in 0..depth {
        let node = boxed(&mut e, list);
        list = List::default();
        e.document.nodes.append(&mut list, node);
    }
    let root = e.document.nodes.pop_front(&mut list).unwrap();
    e.publish_page(root).unwrap();
    assert_eq!(e.document.nodes.live_count(), 0);
    let output = e.outcome();
    let page = output.pages.get(0).unwrap();
    assert_eq!(page.boxes().len(), depth);
    for (i, b) in page.boxes().iter().enumerate() {
        assert_eq!(b.parent, if i == 0 { None } else { Some(i as u32 - 1) });
    }
    assert_eq!(
        page.placements().collect::<Vec<_>>(),
        [Placement::Rule {
            x: 0,
            y: -7,
            width: 7,
            height: 9
        }]
    );
}
