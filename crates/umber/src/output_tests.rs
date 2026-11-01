use super::*;
use crate::{
    Engine, Progress,
    font_test_data::basic,
    nodes::{BoxData, GlueSet, GlueSign, Kind, List},
    numeric::{Glue, Scaled},
};

fn engine() -> Engine<'static> {
    let mut e = Engine::new(br"\font\f=test\f\end");
    e.register_tfm(b"test", &basic()).unwrap();
    assert_eq!(e.advance(usize::MAX).unwrap(), Progress::Complete);
    e
}
fn append(e: &mut Engine<'_>, list: &mut List, kind: Kind) {
    let node = e.document.nodes.alloc(kind).unwrap();
    e.document.nodes.append(list, node);
}
fn append_box(e: &mut Engine<'_>, list: &mut List, data: BoxData) {
    let node = e.document.nodes.alloc_box(data).unwrap();
    e.document.nodes.append(list, node);
}
fn character(c: u8) -> Kind {
    Kind::Character {
        font: 1,
        character: c,
    }
}

#[test]
fn run_iterator_wraps_and_borrows_retained_metrics_after_engine_drop() {
    let e = engine();
    let mut page = Page::new(&e.output_pools, [0; 256], 0);
    let resource = &e.font_catalog.scaled[0];
    let font = page.font(resource);
    assert_eq!(page.font(&resource.clone()), font);
    let mut runs = Runs::default();
    let start = i32::MAX - 10;
    runs.glyph(&mut page, font, b'a', start, -19);
    runs.glyph(&mut page, font, b'b', start.wrapping_add(491520), -19);
    runs.finish(&mut page);
    page.rule(-42, 17, 5, 9);
    drop(e);
    assert_eq!(page.items().len(), 2);
    assert_eq!(page.glyphs(), b"ab");
    let mut iter = page.placements();
    assert_eq!(iter.size_hint(), (3, Some(3)));
    assert_eq!(
        iter.next(),
        Some(Placement::Glyph {
            font,
            character: b'a',
            x: start,
            y: -19,
        })
    );
    assert_eq!(iter.len(), 2);
    assert_eq!(
        iter.next(),
        Some(Placement::Glyph {
            font,
            character: b'b',
            x: start.wrapping_add(491520),
            y: -19,
        })
    );
    assert_eq!(
        iter.next(),
        Some(Placement::Rule {
            x: -42,
            y: 17,
            width: 5,
            height: 9,
        })
    );
    assert_eq!(iter.size_hint(), (0, Some(0)));
    assert_eq!(iter.next(), None);
    assert_eq!(iter.next(), None);
}

#[test]
fn aggregate_placement_capacity_is_checked_before_glyph_or_rule_storage() {
    let pools = Pools::default();
    let mut page = Page::new(&pools, [0; 256], 0);
    let mut runs = Runs::default();
    // Simulate the preceding placements without constructing a giant fixture.
    page.placement_count = u32::MAX - 1;
    runs.glyph(&mut page, 0, b'a', 0, 0);
    assert_eq!(page.placement_count, u32::MAX);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runs.glyph(&mut page, 0, b'b', 0, 0);
    }));
    assert!(result.is_err());
    assert_eq!(page.glyphs(), b"a");
    assert!(page.items().is_empty());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        page.rule(0, 0, 1, 1);
    }));
    assert!(result.is_err());
    assert!(page.items().is_empty());
    runs.finish(&mut page);
    assert!(matches!(&page.items()[0], OutputItem::GlyphRun { glyphs, .. } if glyphs == &(0..1)));
}

#[test]
fn nested_empty_boxes_glue_kerns_and_unpainted_rules_end_runs_exactly() {
    let mut e = engine();
    let mut list = List::default();
    append(&mut e, &mut list, character(b'a'));
    append(
        &mut e,
        &mut list,
        Kind::Glue {
            spec: Glue::default(),
            subtype: Default::default(),
        },
    );
    append(&mut e, &mut list, character(b'b'));
    append(
        &mut e,
        &mut list,
        Kind::Kern {
            width: Scaled(-7),
            explicit: false,
        },
    );
    append_box(
        &mut e,
        &mut list,
        BoxData {
            width: Scaled(-11),
            ..BoxData::default()
        },
    );
    append(&mut e, &mut list, character(b'c'));
    let mut child = List::default();
    append(&mut e, &mut child, character(b'a'));
    append(&mut e, &mut child, character(b'b'));
    append_box(
        &mut e,
        &mut list,
        BoxData {
            list: child,
            width: Scaled(100),
            shift: Scaled(5),
            ..BoxData::default()
        },
    );
    append(&mut e, &mut list, character(b'a'));
    append(
        &mut e,
        &mut list,
        Kind::Rule {
            width: Scaled(7),
            height: Scaled(4),
            depth: Scaled(1),
        },
    );
    append(&mut e, &mut list, character(b'b'));
    append(
        &mut e,
        &mut list,
        Kind::Rule {
            width: Scaled(-13),
            height: Scaled(1),
            depth: Scaled(0),
        },
    );
    append(&mut e, &mut list, character(b'c'));
    for _ in 0..2 {
        append(
            &mut e,
            &mut list,
            Kind::Glue {
                spec: Glue {
                    width: Scaled(-5),
                    stretch: Scaled(1),
                    ..Glue::default()
                },
                subtype: Default::default(),
            },
        );
        append(&mut e, &mut list, character(b'a'));
    }
    let page = e
        .document
        .nodes
        .alloc_box(BoxData {
            list,
            height: Scaled(10),
            glue_set: GlueSet {
                sign: GlueSign::Stretching,
                ratio: 0.5,
                ..GlueSet::default()
            },
            ..BoxData::default()
        })
        .unwrap();
    e.publish_page(page, 0).unwrap();
    let out = e.outcome();
    drop(e);
    let page = out.pages.get(0).unwrap();
    assert_eq!(page.items().len(), 10);
    let actual: Vec<_> = page.placements().collect();
    let mut expected = Vec::new();
    for (c, x, y) in [
        (b'a', 0, 10),
        (b'b', 491520, 10),
        (b'c', 655342, 10),
        (b'a', 983022, 15),
        (b'b', 1474542, 15),
        (b'a', 983122, 10),
    ] {
        expected.push(Placement::Glyph {
            font: 0,
            character: c,
            x,
            y,
        });
    }
    expected.push(Placement::Rule {
        x: 1474642,
        y: 6,
        width: 7,
        height: 5,
    });
    for (c, x) in [
        (b'b', 1474649),
        (b'c', 1638476),
        (b'a', 1966152),
        (b'a', 2457667),
    ] {
        expected.push(Placement::Glyph {
            font: 0,
            character: c,
            x,
            y: 10,
        });
    }
    assert_eq!(actual, expected);
    assert_eq!(page.glyphs(), b"abcababcaa");
}
