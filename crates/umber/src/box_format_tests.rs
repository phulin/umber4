use super::*;
use crate::{
    font_test_data,
    nodes::*,
    numeric::{Glue, GlueOrder},
};

fn engine() -> Engine<'static> {
    let mut e = Engine::new(br"\font\f=test\end");
    e.register_tfm(b"test", &font_test_data::basic()).unwrap();
    e.advance(usize::MAX).unwrap();
    e
}
fn encoded(e: &Engine<'_>) -> Vec<u8> {
    let mut w = Writer::default();
    w.bytes(&[0; 24]);
    write(e, &mut w).unwrap();
    w.finish().unwrap()[24..].to_vec()
}
fn append(e: &mut Engine<'_>, list: &mut List, kind: Kind) {
    let node = e.document.nodes.alloc(kind).unwrap();
    e.document.nodes.append(list, node);
}
fn rule(e: &mut Engine<'_>) -> List {
    let mut list = List::default();
    append(
        e,
        &mut list,
        Kind::Rule {
            width: Scaled(crate::page_output::RUNNING),
            height: Scaled(-17),
            depth: Scaled(5),
        },
    );
    list
}
fn install(e: &mut Engine<'_>, index: usize, node: DetachedNode) {
    e.document
        .nodes
        .append(&mut e.document.boxes.0[index].list, node);
}
fn forest(e: &mut Engine<'_>) {
    let mut list = List::default();
    append(
        e,
        &mut list,
        Kind::Language(crate::language::Language {
            number: 255,
            left: 1,
            right: 63,
        }),
    );
    append(
        e,
        &mut list,
        Kind::Character {
            font: 1,
            character: b'A',
        },
    );
    let mut components = List::default();
    append(
        e,
        &mut components,
        Kind::Character {
            font: 1,
            character: b'f',
        },
    );
    append(e, &mut list, Kind::ligature(1, 12, components, true, true));
    for kind in [KernKind::Normal, KernKind::Explicit, KernKind::Accent] {
        append(
            e,
            &mut list,
            Kind::Kern {
                width: Scaled(i32::MIN),
                kind,
            },
        );
    }
    for after in [false, true] {
        append(
            e,
            &mut list,
            Kind::MathBoundary {
                width: Scaled(-73),
                after,
            },
        );
    }
    let spec = Glue {
        width: Scaled(i32::MIN),
        stretch: Scaled(-3),
        shrink: Scaled(i32::MAX),
        stretch_order: GlueOrder::Filll,
        shrink_order: GlueOrder::Fil,
    };
    for subtype in [
        GlueKind::Normal,
        GlueKind::LeftSkip,
        GlueKind::RightSkip,
        GlueKind::ParFillSkip,
        GlueKind::BaselineSkip,
        GlueKind::LineSkip,
        GlueKind::ParSkip,
        GlueKind::TopSkip,
        GlueKind::SpaceSkip,
        GlueKind::XSpaceSkip,
        GlueKind::TabSkip,
        GlueKind::NonScript,
    ] {
        append(e, &mut list, Kind::Glue { spec, subtype });
    }
    append(e, &mut list, Kind::Penalty(i32::MIN));
    let children = rule(e);
    e.document.nodes.splice(&mut list, children);
    let nested = rule(e);
    let node = e
        .document
        .nodes
        .alloc_box(BoxData {
            list: nested,
            vertical: true,
            width: Scaled(i32::MAX),
            height: Scaled(-7),
            shift: Scaled(17),
            glue_set: GlueSet {
                sign: GlueSign::Shrinking,
                order: GlueOrder::Fill,
                ratio: -0.0,
            },
            ..BoxData::default()
        })
        .unwrap();
    e.document.nodes.append(&mut list, node);
    let children = rule(e);
    let node = e
        .document
        .nodes
        .alloc_insertion(InsertionData {
            list: children,
            class: 254,
            height: Scaled(i32::MIN),
            split_top: spec,
            max_depth: Scaled(-27),
            float_cost: i32::MIN,
        })
        .unwrap();
    e.document.nodes.append(&mut list, node);
    for subtype in [
        GlueKind::AlignedLeaders,
        GlueKind::CenteredLeaders,
        GlueKind::ExpandedLeaders,
        GlueKind::RightSkip,
    ] {
        let children = rule(e);
        let node = e
            .document
            .nodes
            .alloc_leader(LeaderData {
                list: children,
                spec,
                subtype,
            })
            .unwrap();
        e.document.nodes.append(&mut list, node);
    }
    let adjustment = rule(e);
    append(e, &mut list, Kind::Adjustment(adjustment));
    let pre = rule(e);
    let post = rule(e);
    append(e, &mut list, Kind::discretionary(pre, post, 1));
    append(
        e,
        &mut list,
        Kind::Character {
            font: 1,
            character: b'B',
        },
    );
    let node = e
        .document
        .nodes
        .alloc_box(BoxData {
            list,
            vertical: false,
            width: Scaled(i32::MIN),
            height: Scaled(17),
            depth: Scaled(-3),
            shift: Scaled(i32::MAX),
            glue_set: GlueSet {
                sign: GlueSign::Stretching,
                order: GlueOrder::Filll,
                ratio: -0.25,
            },
        })
        .unwrap();
    install(e, 0, node);
    for index in [7, 255] {
        let node = e.document.nodes.alloc_box(BoxData::default()).unwrap();
        install(e, index, node);
    }
}
#[test]
fn box_format_all_ordinary_kinds_roundtrip_without_payload_or_scalar_normalization() {
    let mut source = engine();
    forest(&mut source);
    let bytes = encoded(&source);
    let mut loaded = engine();
    read(&mut loaded, &mut Reader::new(&bytes).unwrap()).unwrap();
    assert_eq!(encoded(&loaded), bytes);
    assert_eq!(
        loaded.document.nodes.live_count(),
        source.document.nodes.live_count()
    );
    assert!(!loaded.document.boxes.head(255).is_null());
}
#[test]
fn box_format_partial_decode_allocation_rejection_releases_every_unconsumed_forest() {
    let mut source = engine();
    forest(&mut source);
    let bytes = encoded(&source);
    let count = source.document.nodes.live_count();
    for budget in 0..count {
        let mut store = NodeStore::default();
        store.reject_after_allocations(budget);
        assert!(matches!(
            crate::nodes::format::read(&source, &mut Reader::new(&bytes).unwrap(), &mut store),
            Err(Error::Capacity)
        ));
        assert_eq!(store.live_count(), 0, "budget {budget}");
    }
}
#[test]
fn box_format_public_font_free_image_restarts_boxes_and_checkpoint_ownership() {
    let boot = r"\catcode123=1\catcode125=2\setbox0=\hbox{\vrule width3pt height4pt depth1pt}\setbox255=\vbox{\kern-2pt\hrule width7pt height2pt}";
    let source = format!(r"{boot}\end");
    let mut init = Engine::new(source.as_bytes());
    init.advance(usize::MAX).unwrap();
    let bytes = init.encode_format().unwrap();
    let job = br"\shipout\copy0\shipout\box255\end";
    let mut loaded = Engine::from_format(&bytes, job).unwrap();
    let checkpoint = loaded.checkpoint().unwrap();
    loaded.advance(usize::MAX).unwrap();
    let outcome = loaded.outcome();
    loaded.restore(&checkpoint).unwrap();
    loaded.advance(usize::MAX).unwrap();
    let replay = loaded.outcome();
    let direct = format!("{boot}{}", std::str::from_utf8(job).unwrap());
    let expected = Engine::new(direct.as_bytes()).run().unwrap();
    assert_eq!(outcome.pages.len(), 2);
    for i in 0..2 {
        let page = outcome.pages.get(i).unwrap();
        assert_eq!(page.boxes(), expected.pages.get(i).unwrap().boxes());
        assert!(
            page.placements()
                .eq(expected.pages.get(i).unwrap().placements())
        );
        assert!(
            page.placements()
                .eq(replay.pages.get(i).unwrap().placements())
        );
    }
}
#[test]
fn box_format_deep_pooled_forests_are_iterative_and_store_versions_stay_independent() {
    let mut source = Engine::new(b"");
    let mut list = List::default();
    for _ in 0..if cfg!(miri) { 64 } else { 4096 } {
        let node = source.document.nodes.alloc_box(BoxData::new(list)).unwrap();
        list = List::default();
        source.document.nodes.append(&mut list, node);
    }
    source.document.boxes.0[255].list = list;
    let bytes = encoded(&source);
    let checkpoint = source.checkpoint().unwrap();
    read(&mut source, &mut Reader::new(&bytes).unwrap()).unwrap();
    source
        .document
        .nodes
        .box_mut(source.document.boxes.head(255))
        .width = Scaled(123);
    assert_ne!(encoded(&source), bytes);
    source.restore(&checkpoint).unwrap();
    assert_eq!(encoded(&source), bytes);
}

#[path = "box_format_invalid_tests.rs"]
mod invalid;

#[test]
fn box_format_boundary_ligature_originals_need_not_have_rendered_metrics() {
    for (first, original, op) in [(64, 32, 1), (0, 200, 1), (0, 0, 1), (0, 0, 0)] {
        let bytes = font_test_data::tfm_range(
            first,
            127,
            &[
                [255, original, 0, 1],
                [128, original, op, b'c'],
                [255, 0, 0, 1],
            ],
            &[],
        );
        let source = format!(
            r"\catcode123=1\catcode125=2\font\f=test\f\setbox0=\hbox{{\char{original}}}\end"
        );
        let mut e = Engine::new(source.as_bytes());
        e.register_tfm(b"test", &bytes).unwrap();
        e.advance(usize::MAX).unwrap();
        assert!(e.character_metrics(1, original).is_none());
        let Kind::Box(id) = &e.document.nodes.get(e.document.boxes.head(0)).kind else {
            panic!("stored hbox")
        };
        let head = e.document.nodes.box_data(id).list.head();
        let Kind::Ligature { components, .. } = &e.document.nodes.get(head).kind else {
            panic!("boundary ligature")
        };
        assert_eq!(components.is_empty(), op == 1);
        if op == 0 {
            assert!(matches!(
                e.document.nodes.get(components.head()).kind,
                Kind::Character {
                    font: 1,
                    character: 0
                }
            ));
        }
        let image = encoded(&e);
        let mut loaded = Engine::new(br"\font\f=test\end");
        loaded.register_tfm(b"test", &bytes).unwrap();
        loaded.advance(usize::MAX).unwrap();
        read(&mut loaded, &mut Reader::new(&image).unwrap()).unwrap();
        assert_eq!(encoded(&loaded), image);
    }
}
