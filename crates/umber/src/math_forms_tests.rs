use super::tests::{add, character, engine, metrics};
use super::*;
use crate::{
    Engine,
    nodes::{self, BoxData},
    numeric::Scaled,
    packing::PackSpec,
};
pub(super) fn delimiter(c: u8) -> Delimiter {
    Delimiter {
        small_char: c,
        ..Delimiter::default()
    }
}
pub(super) fn chars(e: &mut Engine<'_>, m: &mut MathStore, bytes: &[u8]) -> MathList {
    let mut l = MathList::default();
    for c in bytes {
        add(e, m, &mut l, Atom::new(Class::Ord, character(*c)));
    }
    l
}
pub(super) fn kind(e: &mut Engine<'_>, m: &mut MathStore, k: Kind) -> MathList {
    let mut l = MathList::default();
    m.append(&mut l, k, &mut e.document.nodes).unwrap();
    l
}
pub(super) fn surround(
    e: &mut Engine<'_>,
    m: &mut MathStore,
    body: MathList,
    left: Delimiter,
    right: Delimiter,
) -> MathList {
    let mut l = MathList::default();
    let first = m
        .append(
            &mut l,
            Kind::Boundary {
                side: Side::Left,
                delimiter: left,
            },
            &mut e.document.nodes,
        )
        .unwrap();
    m.insert_after(&mut l, first, body);
    m.append(
        &mut l,
        Kind::Boundary {
            side: Side::Right,
            delimiter: right,
        },
        &mut e.document.nodes,
    )
    .unwrap();
    kind(e, m, Kind::Atom(Atom::new(Class::Inner, Field::SubMath(l))))
}
pub(super) fn vertical(e: &mut Engine<'_>, height: i32, depth: i32) -> Field {
    Field::Box(
        e.document
            .nodes
            .alloc_box(BoxData {
                vertical: true,
                width: Scaled(3 * 65536),
                height: Scaled(height),
                depth: Scaled(depth),
                ..BoxData::default()
            })
            .unwrap(),
    )
}
pub(super) fn recipe_font(recipe: [u8; 4]) -> Vec<u8> {
    let mut bytes = metrics(&[], &[(b'b', 3, 0)]);
    let words = u16::from_be_bytes([bytes[0], bytes[1]]) + 1;
    bytes[..2].copy_from_slice(&words.to_be_bytes());
    bytes[20..22].copy_from_slice(&1u16.to_be_bytes());
    let at = bytes.len() - 22 * 4;
    bytes.splice(at..at, recipe);
    bytes
}
pub(super) fn metric(bytes: &mut [u8], table: usize, value: i32) {
    let h = |n: usize| usize::from(u16::from_be_bytes([bytes[n * 2], bytes[n * 2 + 1]]));
    let start = 6
        + h(1)
        + (h(3) + 1 - h(2))
        + h(4)
        + if table >= 1 { h(5) } else { 0 }
        + if table >= 2 { h(6) } else { 0 };
    let at = (start + 1) * 4;
    bytes[at..at + 4].copy_from_slice(&value.to_be_bytes());
}
fn complex(e: &mut Engine<'_>, m: &mut MathStore) -> MathList {
    let numerator = chars(e, m, b"ab");
    let denominator = chars(e, m, b"c");
    let f = kind(
        e,
        m,
        Kind::Fraction(Fraction {
            numerator,
            denominator,
            left: delimiter(b'b'),
            right: delimiter(b'b'),
            ..Fraction::default()
        }),
    );
    let mut fields = Fields::new(Field::SubMath(f));
    fields.sup = character(b'd');
    fields.sub = Field::SubMath(MathList::default());
    let body = kind(
        e,
        m,
        Kind::Radical {
            fields,
            delimiter: delimiter(b'b'),
        },
    );
    let body = kind(e, m, Kind::Over(Fields::new(Field::SubMath(body))));
    let body = kind(e, m, Kind::Under(Fields::new(Field::SubMath(body))));
    let mut body = surround(e, m, body, delimiter(b'b'), Delimiter::default());
    let v = vertical(e, 7 * 65536, 2 * 65536);
    m.append(
        &mut body,
        Kind::VCenter(Fields::new(v)),
        &mut e.document.nodes,
    )
    .unwrap();
    body
}
#[test]
fn math_forms_every_handoff_reclaims_disjoint_trees_and_reuses_storage() {
    let bytes = recipe_font(*b"acde");
    let mut e = engine(&bytes);
    let mut m = MathStore::default();
    let mut scratch = Scratch::default();
    let unrelated = e.document.nodes.alloc_box(BoxData::default()).unwrap();
    let id = unrelated.id();
    e.install_output_box(unrelated);
    let checkpoint = e.checkpoint().unwrap();
    let old = e.document.nodes.clone();
    let mut success = false;
    for budget in 0..512 {
        let l = complex(&mut e, &mut m);
        scratch.interrupt_after(budget);
        match e.math_to_hlist(&mut m, l, Style::TEXT, false, &mut scratch) {
            Ok(l) => {
                e.document.nodes.release(l, &mut ReleaseScratch::default());
                success = true;
            }
            Err(e) => assert_eq!(e, Error::Capacity),
        }
        assert_eq!(m.live, 0);
        assert!(scratch.is_empty());
        assert_eq!(e.document.nodes.live_count(), 1);
        assert!(e.document.nodes.box_slot_count() < 128);
        assert!(matches!(old.get(id).kind, nodes::Kind::Box(_)));
        if success {
            assert!(budget > 30);
            break;
        }
    }
    assert!(success);
    scratch.clear_interrupt();
    e.restore(&checkpoint).unwrap();
    assert_eq!(e.document.boxes.head(255), id);
    let unrelated = e.take_output_box();
    e.document
        .nodes
        .release_node(unrelated, &mut ReleaseScratch::default());
    assert_eq!(e.document.nodes.live_count(), 0);
    drop(e);
    assert!(matches!(old.get(id).kind, nodes::Kind::Box(_)));
}
#[test]
fn math_forms_invalid_boundaries_and_vcenters_release_owned_fields() {
    let bytes = metrics(&[], &[]);
    let mut e = engine(&bytes);
    let mut m = MathStore::default();
    let mut scratch = Scratch::default();
    for right in [false, true] {
        let body = chars(&mut e, &mut m, b"a");
        let mut l = MathList::default();
        if right {
            m.append(
                &mut l,
                Kind::Boundary {
                    side: Side::Right,
                    delimiter: Delimiter::default(),
                },
                &mut e.document.nodes,
            )
            .unwrap();
        } else {
            m.append(
                &mut l,
                Kind::Boundary {
                    side: Side::Left,
                    delimiter: Delimiter::default(),
                },
                &mut e.document.nodes,
            )
            .unwrap();
        }
        let at = l.head;
        m.insert_after(&mut l, at, body);
        assert!(matches!(
            e.math_to_hlist(&mut m, l, Style::TEXT, false, &mut scratch),
            Err(Error::Syntax)
        ));
        assert_eq!(m.live, 0);
        assert_eq!(e.document.nodes.live_count(), 0);
    }
    for boxed in [false, true] {
        let nucleus = if boxed {
            Field::Box(e.document.nodes.alloc_box(BoxData::default()).unwrap())
        } else {
            character(b'a')
        };
        let mut f = Fields::new(nucleus);
        f.sup = vertical(&mut e, 65536, 0);
        let l = kind(&mut e, &mut m, Kind::VCenter(f));
        assert!(matches!(
            e.math_to_hlist(&mut m, l, Style::TEXT, false, &mut scratch),
            Err(Error::Syntax)
        ));
        assert_eq!(m.live, 0);
        assert_eq!(e.document.nodes.live_count(), 0);
    }
}
#[test]
fn math_forms_deep_construction_and_bulk_release_are_iterative() {
    let bytes = metrics(&[], &[]);
    let mut e = engine(&bytes);
    let mut m = MathStore::default();
    let mut scratch = Scratch::default();
    for font in 1..e.document.fonts.len {
        let f = e.document.fonts.descriptors[font];
        e.document.fonts.parameters[f.params + 7] = Scaled(1);
    }
    let depth = if cfg!(miri) { 32 } else { 2048 };
    for release in [false, true] {
        let mut l = chars(&mut e, &mut m, b"a");
        for i in 0..depth {
            l = kind(
                &mut e,
                &mut m,
                if i % 2 == 0 {
                    Kind::Over(Fields::new(Field::SubMath(l)))
                } else {
                    Kind::Under(Fields::new(Field::SubMath(l)))
                },
            );
        }
        if release {
            m.release(l, &mut e.document.nodes);
        } else {
            let output = e
                .math_to_hlist(&mut m, l, Style::TEXT, false, &mut scratch)
                .unwrap();
            let packed = e.hpack(output, PackSpec::Natural).unwrap();
            e.document
                .nodes
                .release_node(packed, &mut ReleaseScratch::default());
        }
        assert_eq!(m.live, 0);
        assert_eq!(e.document.nodes.live_count(), 0);
    }
    assert!(size_of::<Node>() <= 48, "arena cell {}", size_of::<Node>());
}
