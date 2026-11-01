use super::*;
use super::{
    forms_tests::{chars, kind, vertical},
    tests::{character, engine, metrics},
};
use crate::{
    Engine,
    nodes::{self, BoxData},
};
pub(super) fn accent(fields: Fields, family: u8, character: u8) -> Kind {
    Kind::Accent {
        accent: MathChar { family, character },
        fields,
    }
}
fn complex(e: &mut Engine<'_>, m: &mut MathStore) -> MathList {
    let mut inside = Fields::new(character(b'b'));
    inside.sub = Field::SubMath(MathList::default());
    inside.sup = character(b'c');
    let inner = kind(e, m, accent(inside, 0, b'a'));
    let mut outer = Fields::new(Field::SubMath(inner));
    outer.sub = vertical(e, 65536, 0);
    outer.sup = Field::SubMath(chars(e, m, b"de"));
    kind(e, m, accent(outer, 0, b'a'))
}
#[test]
fn math_accents_every_handoff_reclaims_fields_boxes_and_retains_checkpoint_roots() {
    let bytes = metrics(&[[128, b's', 128, 0]], &[(b'b', 1, 0)]);
    let mut e = engine(&bytes);
    let mut m = MathStore::default();
    let mut scratch = Scratch::default();
    let unrelated = e.document.nodes.alloc_box(BoxData::default()).unwrap();
    let id = unrelated.id();
    e.install_output_box(unrelated);
    let checkpoint = e.checkpoint().unwrap();
    let old = e.document.nodes.clone();
    let mut success = false;
    for budget in 0..256 {
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
        assert!(e.document.nodes.box_slot_count() < 32);
        assert!(matches!(old.get(id).kind, nodes::Kind::Box(_)));
        if success {
            assert!(budget > 20);
            break;
        }
    }
    assert!(success);
    scratch.clear_interrupt();
    e.restore(&checkpoint).unwrap();
    assert_eq!(e.document.boxes.head(255), id);
    let b = e.take_output_box();
    e.document
        .nodes
        .release_node(b, &mut ReleaseScratch::default());
    drop(e);
    assert!(matches!(old.get(id).kind, nodes::Kind::Box(_)));
}
#[test]
fn math_accents_capacity_and_family_failures_release_owned_subfields() {
    let bytes = metrics(&[], &[]);
    let mut e = engine(&bytes);
    let mut m = MathStore {
        limit: 0,
        ..MathStore::default()
    };
    let mut fields = Fields::new(vertical(&mut e, 65536, 0));
    fields.sup = vertical(&mut e, 65536, 0);
    assert!(matches!(
        m.append(
            &mut MathList::default(),
            accent(fields, 0, b'a'),
            &mut e.document.nodes
        ),
        Err(Error::Capacity)
    ));
    assert_eq!(e.document.nodes.live_count(), 0);
    m.limit = u32::MAX;
    for (accent_family, accent_char, nucleus_family) in [(15, b'a', 0), (0, 0, 15), (0, b'a', 15)] {
        let mut fields = Fields::new(Field::Character {
            family: nucleus_family,
            character: b'b',
        });
        fields.sub = vertical(&mut e, 65536, 0);
        let l = kind(&mut e, &mut m, accent(fields, accent_family, accent_char));
        let mut scratch = Scratch::default();
        assert!(matches!(
            e.math_to_hlist(&mut m, l, Style::TEXT, false, &mut scratch),
            Err(Error::Font)
        ));
        assert_eq!(m.live, 0);
        assert!(scratch.is_empty());
        assert_eq!(e.document.nodes.live_count(), 0);
    }
}
#[test]
fn math_accents_deep_conversion_release_and_arena_reuse_are_iterative() {
    let bytes = metrics(&[], &[]);
    let mut e = engine(&bytes);
    let mut m = MathStore::default();
    let mut scratch = Scratch::default();
    let depth = if cfg!(miri) { 32 } else { 2048 };
    for release in [false, true, false] {
        let mut l = chars(&mut e, &mut m, b"b");
        for _ in 0..depth {
            l = kind(
                &mut e,
                &mut m,
                accent(Fields::new(Field::SubMath(l)), 0, b'a'),
            );
        }
        if release {
            m.release(l, &mut e.document.nodes);
        } else {
            let output = e
                .math_to_hlist(&mut m, l, Style::TEXT, false, &mut scratch)
                .unwrap();
            e.document
                .nodes
                .release(output, &mut ReleaseScratch::default());
        }
        assert_eq!(m.live, 0);
        assert_eq!(m.cells.len(), depth + 1);
        assert_eq!(e.document.nodes.live_count(), 0);
        assert!(scratch.is_empty());
    }
    assert!(size_of::<Node>() <= 48);
}
