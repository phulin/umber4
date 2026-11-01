//! Pending math units and immediate nonscript removal share the affine arena.
use super::*;
use crate::{
    Engine,
    nodes::{self, GlueKind},
    numeric::{Glue, GlueOrder, Scaled},
};

fn marker(e: &mut Engine<'_>) -> Kind {
    Kind::Material(
        e.document
            .nodes
            .alloc(nodes::Kind::Glue {
                spec: Glue::default(),
                subtype: GlueKind::NonScript,
            })
            .unwrap(),
    )
}
fn append(e: &mut Engine<'_>, m: &mut MathStore, l: &mut MathList, k: Kind) {
    m.append(l, k, &mut e.document.nodes).unwrap();
}

#[test]
fn mu_finite_scaling_uses_signed_native_rounding_and_preserves_infinite_components() {
    let mut e = tests::engine(&tests::metrics(&[], &[]));
    let mut m = MathStore::default();
    let mut scratch = Scratch::default();
    // A negative fractional mu unit distinguishes nx_plus_y from a single
    // truncated product. Vary finite and infinite components independently.
    let font = e.document.fonts.descriptors[1];
    e.document.fonts.parameters[font.params + 5] = Scaled(-589824);
    for order in [
        GlueOrder::Normal,
        GlueOrder::Fil,
        GlueOrder::Fill,
        GlueOrder::Filll,
    ] {
        let mut l = MathList::default();
        append(
            &mut e,
            &mut m,
            &mut l,
            Kind::MuGlue(Glue {
                width: Scaled(1),
                stretch: Scaled(-1),
                shrink: Scaled(3),
                stretch_order: order,
                shrink_order: order,
            }),
        );
        let output = e
            .math_to_hlist(&mut m, l, Style::TEXT, false, &mut scratch)
            .unwrap();
        let nodes::Kind::Glue { spec, subtype } = e.document.nodes.get(output.head()).kind else {
            panic!()
        };
        assert_eq!(subtype, GlueKind::Normal);
        assert_eq!(spec.width, Scaled(-1));
        assert_eq!(
            spec.stretch,
            Scaled(if order == GlueOrder::Normal { 1 } else { -1 })
        );
        assert_eq!(
            spec.shrink,
            Scaled(if order == GlueOrder::Normal { -2 } else { 3 })
        );
        assert_eq!(spec.stretch_order, order);
        assert_eq!(spec.shrink_order, order);
        e.document
            .nodes
            .release(output, &mut ReleaseScratch::default());
    }
    assert_eq!(m.live, 0);
    assert_eq!(e.document.nodes.live_count(), 0);
}

#[test]
fn nonscript_releases_leader_forest_and_skips_overflowing_pending_tail() {
    let mut e = tests::engine(&tests::metrics(&[], &[]));
    let mut m = MathStore::default();
    let mut scratch = Scratch::default();
    let font = e.document.fonts.descriptors[2];
    e.document.fonts.parameters[font.params + 5] = Scaled::MAX;
    for pending in 0..4 {
        let mut l = MathList::default();
        let k = marker(&mut e);
        append(&mut e, &mut m, &mut l, k);
        let k = match pending {
            0 => Kind::MuGlue(Glue {
                width: Scaled(i32::MAX),
                ..Glue::default()
            }),
            1 => Kind::MuKern(Scaled(i32::MAX)),
            2 => marker(&mut e),
            _ => {
                let b = e
                    .document
                    .nodes
                    .alloc_box(nodes::BoxData::default())
                    .unwrap();
                let mut list = List::default();
                e.document.nodes.append(&mut list, b);
                Kind::Material(
                    e.document
                        .nodes
                        .alloc_leader(nodes::LeaderData {
                            list,
                            spec: Glue::default(),
                            subtype: GlueKind::AlignedLeaders,
                        })
                        .unwrap(),
                )
            }
        };
        append(&mut e, &mut m, &mut l, k);
        let output = e
            .math_to_hlist(&mut m, l, Style::new(4).unwrap(), false, &mut scratch)
            .unwrap();
        assert_eq!(output.head(), output.tail());
        assert!(matches!(
            e.document.nodes.get(output.head()).kind,
            nodes::Kind::Glue {
                subtype: GlueKind::NonScript,
                ..
            }
        ));
        assert_eq!(m.live, 0);
        assert_eq!(e.document.nodes.live_count(), 1);
        e.document
            .nodes
            .release(output, &mut ReleaseScratch::default());
        assert_eq!(e.document.nodes.live_count(), 0);
    }
}

#[test]
fn nonscript_uses_current_size_in_all_eight_styles() {
    let mut e = tests::engine(&tests::metrics(&[], &[]));
    let mut m = MathStore::default();
    let mut scratch = Scratch::default();
    for value in 0..8 {
        let mut l = MathList::default();
        let k = marker(&mut e);
        append(&mut e, &mut m, &mut l, k);
        append(
            &mut e,
            &mut m,
            &mut l,
            Kind::MuGlue(Glue {
                width: Scaled::ONE,
                ..Glue::default()
            }),
        );
        let output = e
            .math_to_hlist(&mut m, l, Style::new(value).unwrap(), false, &mut scratch)
            .unwrap();
        assert_eq!(output.head() == output.tail(), value >= 4);
        e.document
            .nodes
            .release(output, &mut ReleaseScratch::default());
        assert_eq!(m.live, 0);
        assert_eq!(e.document.nodes.live_count(), 0);
    }
}

#[test]
fn failed_mu_conversion_reclaims_prefix_and_pending_boxes_preserving_outer_ids() {
    let mut e = tests::engine(&tests::metrics(&[], &[]));
    let mut m = MathStore::default();
    let mut scratch = Scratch::default();
    let mut outer = MathList::default();
    let b = e
        .document
        .nodes
        .alloc_box(nodes::BoxData::default())
        .unwrap();
    let id = m
        .append(
            &mut outer,
            Kind::Atom(Atom::new(Class::Ord, Field::Box(b))),
            &mut e.document.nodes,
        )
        .unwrap();
    let font = e.document.fonts.descriptors[1];
    e.document.fonts.parameters[font.params + 5] = Scaled::MAX;
    let mut high_water = 0;
    for _ in 0..3 {
        let mut inner = MathList::default();
        for width in [Scaled(1), Scaled(i32::MAX)] {
            append(
                &mut e,
                &mut m,
                &mut inner,
                Kind::MuGlue(Glue {
                    width,
                    ..Glue::default()
                }),
            );
        }
        let b = e
            .document
            .nodes
            .alloc_box(nodes::BoxData::default())
            .unwrap();
        append(
            &mut e,
            &mut m,
            &mut inner,
            Kind::Atom(Atom::new(Class::Ord, Field::Box(b))),
        );
        assert!(matches!(
            e.math_to_hlist(&mut m, inner, Style::TEXT, false, &mut scratch),
            Err(Error::Overflow)
        ));
        assert_eq!(m.live, 1);
        assert_eq!(outer.head, id);
        assert!(matches!(m.node(id).kind, Kind::Atom(_)));
        assert_eq!(e.document.nodes.live_count(), 1);
        assert!(scratch.is_empty());
        if high_water == 0 {
            high_water = m.cells.len();
        }
        assert_eq!(m.cells.len(), high_water);
    }
    m.release(outer, &mut e.document.nodes);
    assert_eq!(e.document.nodes.live_count(), 0);
}
