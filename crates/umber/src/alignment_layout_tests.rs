use super::*;
use crate::page_output::RUNNING;
use crate::{
    layout_values::{DimenParam, GlueParam},
    nodes::KernKind,
};
#[derive(Clone)]
enum Item {
    Rule(i32, i32, i32),
    Kern(i32),
    Glue(Glue),
    Glyph(u8),
    Adjustment(Vec<Item>),
}
#[derive(Clone)]
struct Entry {
    span: usize,
    items: Vec<Item>,
}
#[derive(Clone)]
struct Row {
    entries: Vec<Entry>,
    material: Vec<Item>,
}
#[derive(Clone)]
struct Fixture {
    tabs: Vec<Glue>,
    rows: Vec<Row>,
    spec: PackSpec,
}
fn g(width: i32, stretch: i32, shrink: i32, order: GlueOrder) -> Glue {
    Glue {
        width: Scaled(width),
        stretch: Scaled(stretch),
        shrink: Scaled(shrink),
        stretch_order: order,
        shrink_order: order,
    }
}
fn entry(span: usize, width: i32) -> Entry {
    Entry {
        span,
        items: vec![Item::Rule(width, 3, 2)],
    }
}
fn row(entries: Vec<Entry>) -> Row {
    Row {
        entries,
        material: vec![],
    }
}
fn fixtures() -> Vec<Fixture> {
    let mut out = Vec::new();
    let rows = vec![
        row(vec![entry(1, 10), entry(1, 20), entry(1, 8)]),
        row(vec![entry(2, 45), entry(1, 2)]),
        row(vec![entry(1, 3), entry(2, 35)]),
        row(vec![entry(3, 80)]),
        row(vec![entry(2, 50)]),
        row(vec![entry(3, 70)]),
        row(vec![Entry {
            span: 1,
            items: vec![],
        }]),
    ];
    for spec in [
        PackSpec::Natural,
        PackSpec::To(Scaled(140)),
        PackSpec::Spread(Scaled(31)),
        PackSpec::To(Scaled(20)),
    ] {
        for order in [
            GlueOrder::Normal,
            GlueOrder::Fil,
            GlueOrder::Fill,
            GlueOrder::Filll,
        ] {
            out.push(Fixture {
                tabs: vec![
                    g(2, 3, 4, order),
                    g(5, 2, 3, order),
                    g(-2, 4, 2, order),
                    g(3, 1, 5, order),
                ],
                rows: rows.clone(),
                spec,
            });
        }
    }
    out.push(Fixture {
        tabs: vec![g(4, 1, 2, GlueOrder::Fil); 4],
        rows: vec![row(vec![entry(3, 73)]), row(vec![entry(3, 80)])],
        spec: PackSpec::To(Scaled(101)),
    });
    out.push(Fixture {
        tabs: vec![g(0, 1, 0, GlueOrder::Normal); 3],
        rows: vec![
            row(vec![entry(1, 3), entry(1, 5)]),
            row(vec![Entry {
                span: 2,
                items: vec![
                    Item::Rule(3, 3, 2),
                    Item::Glue(g(0, 1, 0, GlueOrder::Normal)),
                    Item::Rule(5, 3, 2),
                ],
            }]),
        ],
        spec: PackSpec::To(Scaled(9)),
    });
    for shrink_order in [GlueOrder::Normal, GlueOrder::Fil] {
        out.push(Fixture {
            tabs: vec![g(10, 0, 50, GlueOrder::Normal); 3],
            rows: vec![
                row(vec![entry(1, 20), entry(1, 30)]),
                row(vec![Entry {
                    span: 2,
                    items: vec![Item::Rule(50, 7, 0), Item::Glue(g(0, 0, 3, shrink_order))],
                }]),
            ],
            spec: PackSpec::To(Scaled(20)),
        });
    }
    out.push(Fixture {
        tabs: vec![Glue::default(); 3],
        rows: vec![
            row(vec![
                Entry {
                    span: 1,
                    items: vec![Item::Kern(-8)],
                },
                Entry {
                    span: 1,
                    items: vec![Item::Kern(-13)],
                },
            ]),
            row(vec![Entry {
                span: 2,
                items: vec![Item::Kern(-30)],
            }]),
        ],
        spec: PackSpec::Spread(Scaled(-7)),
    });
    out.push(Fixture {
        tabs: vec![g(3, 2, 1, GlueOrder::Normal); 4],
        rows: vec![],
        spec: PackSpec::To(Scaled(93)),
    });
    out.push(Fixture {
        tabs: vec![Glue::default(); 3],
        rows: vec![row(vec![Entry {
            span: 1,
            items: vec![],
        }])],
        spec: PackSpec::Natural,
    });
    let mut rule = row(vec![entry(1, 7), entry(1, 11)]);
    rule.material.push(Item::Rule(RUNNING, 4, 0));
    let mut second = row(vec![entry(1, 3)]);
    second
        .material
        .push(Item::Glue(g(7, 0, 0, GlueOrder::Normal)));
    out.push(Fixture {
        tabs: vec![g(2, 1, 0, GlueOrder::Fil); 3],
        rows: vec![rule, second, row(vec![entry(1, 2), entry(1, 4)])],
        spec: PackSpec::To(Scaled(40)),
    });
    out.push(Fixture {
        tabs: vec![Glue::default(); 3],
        rows: vec![
            row(vec![
                Entry {
                    span: 1,
                    items: vec![
                        Item::Glyph(b'a'),
                        Item::Glue(g(0, 65536, 0, GlueOrder::Fil)),
                    ],
                },
                Entry {
                    span: 1,
                    items: vec![Item::Glyph(b'b'), Item::Glyph(b'c')],
                },
            ]),
            row(vec![Entry {
                span: 2,
                items: vec![
                    Item::Glyph(b'd'),
                    Item::Adjustment(vec![Item::Rule(19, 6, 0)]),
                ],
            }]),
        ],
        spec: PackSpec::Spread(Scaled(17)),
    });
    out.push(Fixture {
        tabs: vec![g(1, 1, 1, GlueOrder::Normal); 7],
        rows: vec![
            row([3, 5, 7, 11, 13, 17]
                .into_iter()
                .map(|w| entry(1, w))
                .collect()),
            row(vec![entry(3, 30)]),
            row(vec![entry(4, 47)]),
            row(vec![entry(6, 88)]),
            row(vec![entry(1, 2), entry(2, 33)]),
            row(vec![entry(1, 1), entry(4, 69)]),
            row(vec![entry(1, 2), entry(5, 105)]),
        ],
        spec: PackSpec::To(Scaled(150)),
    });
    out
}
fn engine() -> Engine<'static> {
    let mut e = Engine::new(br"\font\f=test\f\relax");
    e.register_tfm(b"test", &crate::font_test_data::basic())
        .unwrap();
    e.advance(2).unwrap();
    e.document.layout_values.glues[GlueParam::Baseline.index()]
        .value
        .width = Scaled(12);
    e.document.layout_values.glues[GlueParam::Line.index()]
        .value
        .width = Scaled(1);
    e.document.layout_values.dimens[DimenParam::LineSkipLimit.index()].value = Scaled::ZERO;
    e
}
fn items(e: &mut Engine<'_>, input: &[Item]) -> List {
    let mut list = List::default();
    for item in input {
        let kind = match item {
            Item::Rule(w, h, d) => Kind::Rule {
                width: Scaled(*w),
                height: Scaled(*h),
                depth: Scaled(*d),
            },
            Item::Kern(width) => Kind::Kern {
                width: Scaled(*width),
                kind: KernKind::Explicit,
            },
            Item::Glue(spec) => Kind::Glue {
                spec: *spec,
                subtype: GlueKind::Normal,
            },
            Item::Glyph(character) => Kind::Character {
                font: e.document.fonts.current.value as u32,
                character: *character,
            },
            Item::Adjustment(children) => Kind::Adjustment(items(e, children)),
        };
        let node = e.document.nodes.alloc(kind).unwrap();
        e.document.nodes.append(&mut list, node);
    }
    list
}
fn build(e: &mut Engine<'_>, fixture: &Fixture) -> List {
    e.begin_alignment_layout(&fixture.tabs).unwrap();
    for row in &fixture.rows {
        e.begin_alignment_row().unwrap();
        for entry in &row.entries {
            let list = items(e, &entry.items);
            e.alignment_cell(list, entry.span).unwrap();
        }
        e.end_alignment_row().unwrap();
        if !row.material.is_empty() {
            let list = items(e, &row.material);
            e.alignment_material(list).unwrap();
            if row.material.iter().any(|i| matches!(i, Item::Rule(..))) {
                e.document.text.prev_depth = Scaled(-65_536_000);
            }
        }
    }
    e.finish_alignment_layout(fixture.spec).unwrap()
}
#[test]
fn alignment_layout_constructed_inputs_complete_and_publish() {
    for fixture in fixtures() {
        let mut e = engine();
        let list = build(&mut e, &fixture);
        assert!(e.can_checkpoint());
        let b = e.vpackage(list, PackSpec::Natural, Scaled::MAX).unwrap();
        e.publish_page(b).unwrap();
        assert_eq!(e.document.nodes.live_count(), 0);
    }
}
#[test]
fn alignment_layout_zero_stretch_sign_and_span_rounding_are_exact() {
    let mut e = engine();
    let fixture = Fixture {
        tabs: vec![Glue::default(); 2],
        rows: vec![row(vec![entry(1, 1)]), row(vec![entry(1, 9)])],
        spec: PackSpec::Natural,
    };
    let list = build(&mut e, &fixture);
    let Kind::Box(row) = &e.document.nodes.get(list.head()).kind else {
        panic!()
    };
    let cell = e
        .document
        .nodes
        .get(e.document.nodes.box_data(row).list.head())
        .next;
    let Kind::Box(cell) = &e.document.nodes.get(cell).kind else {
        panic!()
    };
    let data = e.document.nodes.box_data(cell);
    assert_eq!(
        (data.glue_set.sign, data.glue_set.order, data.glue_set.ratio),
        (crate::nodes::GlueSign::Stretching, GlueOrder::Normal, 0.0)
    );
    e.document.nodes.release(list, &mut e.page_release);
}
#[test]
fn alignment_layout_body_handoff_exposes_unset_rows_and_actual_previous_depth() {
    let mut e = engine();
    e.begin_alignment_layout(&[Glue::default(); 2]).unwrap();
    e.begin_alignment_row().unwrap();
    let list = items(&mut e, &[Item::Rule(3, 5, 2)]);
    e.alignment_cell(list, 1).unwrap();
    e.end_alignment_row().unwrap();
    assert_eq!(e.document.text.prev_depth, Scaled(2));
    let body = e.take_alignment_body().unwrap();
    assert!(!e.can_checkpoint());
    assert!(e.begin_alignment_row().is_err());
    assert!(e.finish_alignment_layout(PackSpec::Natural).is_err());
    e.document.text.mode = crate::text::Mode::InternalVertical;
    e.document.text.vertical = body;
    e.make_box(
        5,
        crate::boxes::Destination::Register {
            index: 0,
            global: true,
        },
    )
    .unwrap();
    assert!(e.document.boxes.head(0).is_null());
    let skip = e
        .document
        .nodes
        .alloc(Kind::Glue {
            spec: g(7, 0, 0, GlueOrder::Normal),
            subtype: GlueKind::Normal,
        })
        .unwrap();
    e.document.nodes.append(&mut e.document.text.vertical, skip);
    let body = std::mem::take(&mut e.document.text.vertical);
    e.put_alignment_body(body).unwrap();
    let result = e.finish_alignment_layout(PackSpec::Natural).unwrap();
    assert!(
        matches!(e.document.nodes.get(result.tail()).kind, Kind::Glue { spec, .. } if spec.width == Scaled(7))
    );
    e.document.nodes.release(result, &mut e.page_release);
}
#[cfg(test)]
#[path = "alignment_layout_oracle_tests.rs"]
mod oracle;
#[cfg(test)]
#[path = "alignment_layout_ownership_tests.rs"]
mod ownership;
