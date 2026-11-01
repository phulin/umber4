use super::*;
use crate::nodes::KernKind;
use crate::{
    Engine,
    nodes::{self, BoxData},
    numeric::{Glue, Scaled},
    scalars::Target,
};
pub(super) const BOOT: &str = r"\catcode123=1\catcode125=2\catcode36=3\catcode94=7\catcode95=8\font\t=math\font\s=math at7pt\font\ss=math at5pt\textfont0=\t\textfont1=\t\textfont2=\t\textfont3=\t\scriptfont0=\s\scriptfont1=\s\scriptfont2=\s\scriptfont3=\s\scriptscriptfont0=\ss\scriptscriptfont1=\ss\scriptscriptfont2=\ss\scriptscriptfont3=\ss\thinmuskip=3mu\medmuskip=4mu plus2mu minus4mu\thickmuskip=5mu plus5mu\scriptspace=.5pt \end";
pub(super) fn metrics(program: &[[u8; 4]], tags: &[(u8, u8, u8)]) -> Vec<u8> {
    let mut bytes = crate::font_test_data::tfm(program, tags);
    let words = u16::from_be_bytes([bytes[0], bytes[1]]) + 15;
    bytes[..2].copy_from_slice(&words.to_be_bytes());
    bytes[22..24].copy_from_slice(&22u16.to_be_bytes());
    // Parameters 8--22, independent signed 12.20 TFM quantities.
    for value in [
        700000i32, 400000, 300000, 680000, 350000, 420000, 370000, 290000, 160000, 260000, 100000,
        60000, 40000, 60000, 250000,
    ] {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    bytes
}
pub(super) fn engine(tfm: &[u8]) -> Engine<'static> {
    let mut e = Engine::new(BOOT.as_bytes());
    e.register_tfm(b"math", tfm).unwrap();
    e.advance(usize::MAX).unwrap();
    e
}
pub(super) fn character(character: u8) -> Field {
    Field::Character {
        family: 0,
        character,
    }
}
pub(super) fn add(e: &mut Engine<'_>, m: &mut MathStore, list: &mut MathList, atom: Atom) {
    m.append(list, Kind::Atom(atom), &mut e.document.nodes)
        .unwrap();
}
fn glyphs(nodes: &nodes::NodeStore, list: &List) -> Vec<u8> {
    let mut result = Vec::new();
    let mut pending = vec![list.head()];
    while let Some(mut id) = pending.pop() {
        while !id.is_null() {
            let n = nodes.get(id);
            match &n.kind {
                nodes::Kind::Character { character, .. } => result.push(*character),
                nodes::Kind::Box(b) => pending.push(nodes.box_data(b).list.head()),
                _ => {}
            }
            id = n.next;
        }
    }
    result
}
fn reclaim(e: &mut Engine<'_>, list: List) {
    e.document
        .nodes
        .release(list, &mut ReleaseScratch::default());
    assert_eq!(e.document.nodes.live_count(), 0);
}
#[test]
fn math_arena_is_one_compact_reusable_typed_store() {
    assert!(size_of::<Node>() <= 48);
    let mut m = MathStore::default();
    let mut nodes = nodes::NodeStore::default();
    for _ in 0..8 {
        let mut l = MathList::default();
        for _ in 0..1000 {
            m.append(
                &mut l,
                Kind::Atom(Atom::new(Class::Ord, character(b'a'))),
                &mut nodes,
            )
            .unwrap();
        }
        m.release(l, &mut nodes);
        assert_eq!(m.live, 0);
        assert!(m.pending.is_empty());
        assert_eq!(m.cells.len(), 1000);
    }
}
#[test]
fn math_empty_script_is_present_and_suppresses_italic() {
    let bytes = metrics(&[], &[]);
    let mut e = engine(&bytes);
    let mut m = MathStore::default();
    let mut scratch = Scratch::default();
    for present in [false, true] {
        let mut l = MathList::default();
        let mut a = Atom::new(Class::Ord, character(b'a'));
        if present {
            a.fields.sub = Field::SubMath(MathList::default());
        }
        add(&mut e, &mut m, &mut l, a);
        let l = e
            .math_to_hlist(&mut m, l, Style::TEXT, false, &mut scratch)
            .unwrap();
        let next = e.document.nodes.get(l.head()).next;
        if present {
            assert!(matches!(
                e.document.nodes.get(next).kind,
                nodes::Kind::Box(_)
            ));
        } else {
            assert!(matches!(
                e.document.nodes.get(next).kind,
                nodes::Kind::Kern { .. }
            ));
        }
        reclaim(&mut e, l);
        assert_eq!(m.live, 0);
    }
}
#[test]
fn math_ord_no_match_still_suppresses_text_italic_and_has_no_boundaries() {
    let bytes = metrics(
        &[[255, b'b', 0, 1], [128, b'a', 0, b'c'], [255, 0, 0, 1]],
        &[],
    );
    let mut e = engine(&bytes);
    let mut m = MathStore::default();
    let mut scratch = Scratch::default();
    let mut l = MathList::default();
    for c in b"ab" {
        add(&mut e, &mut m, &mut l, Atom::new(Class::Ord, character(*c)));
    }
    let l = e
        .math_to_hlist(&mut m, l, Style::TEXT, false, &mut scratch)
        .unwrap();
    assert_eq!(glyphs(&e.document.nodes, &l), b"ab");
    assert!(matches!(
        e.document
            .nodes
            .get(e.document.nodes.get(l.head()).next)
            .kind,
        nodes::Kind::Character {
            character: b'b',
            ..
        }
    ));
    reclaim(&mut e, l);
}
#[test]
fn math_ligature_transfers_following_scripts_and_reuses_freed_slots() {
    let bytes = metrics(&[[128, b'b', 0, b'c']], &[(b'a', 1, 0)]);
    let mut e = engine(&bytes);
    let mut m = MathStore::default();
    let mut scratch = Scratch::default();
    for _ in 0..12 {
        let mut l = MathList::default();
        add(
            &mut e,
            &mut m,
            &mut l,
            Atom::new(Class::Ord, character(b'a')),
        );
        let mut a = Atom::new(Class::Rel, character(b'b'));
        a.fields.sup = character(b'd');
        add(&mut e, &mut m, &mut l, a);
        let l = e
            .math_to_hlist(&mut m, l, Style::TEXT, false, &mut scratch)
            .unwrap();
        assert_eq!(glyphs(&e.document.nodes, &l), b"cd");
        reclaim(&mut e, l);
        assert_eq!(m.live, 0);
        assert_eq!(m.cells.len(), 2);
    }
}
#[test]
fn math_binary_cancellation_crosses_choice_and_material_and_penalty_is_immediate() {
    let bytes = metrics(&[], &[]);
    let mut e = engine(&bytes);
    let mut m = MathStore::default();
    let mut scratch = Scratch::default();
    e.document.scalars[Target::parameter(26)].value = 123;
    let mut l = MathList::default();
    add(
        &mut e,
        &mut m,
        &mut l,
        Atom::new(Class::Ord, character(b'a')),
    );
    add(
        &mut e,
        &mut m,
        &mut l,
        Atom::new(Class::Bin, character(b'b')),
    );
    let mut branch = MathList::default();
    add(
        &mut e,
        &mut m,
        &mut branch,
        Atom::new(Class::Ord, character(b'c')),
    );
    m.append(
        &mut l,
        Kind::Choice([
            MathList::default(),
            branch,
            MathList::default(),
            MathList::default(),
        ]),
        &mut e.document.nodes,
    )
    .unwrap();
    let l = e
        .math_to_hlist(&mut m, l, Style::TEXT, true, &mut scratch)
        .unwrap();
    let mut id = l.head();
    let mut penalties = Vec::new();
    while !id.is_null() {
        let n = e.document.nodes.get(id);
        if let nodes::Kind::Penalty(p) = n.kind {
            penalties.push(p);
        }
        id = n.next;
    }
    assert_eq!(penalties, [123]);
    reclaim(&mut e, l);
    for material in [false, true] {
        let mut l = MathList::default();
        add(
            &mut e,
            &mut m,
            &mut l,
            Atom::new(Class::Ord, character(b'a')),
        );
        add(
            &mut e,
            &mut m,
            &mut l,
            Atom::new(Class::Bin, character(b'b')),
        );
        if material {
            let n = e
                .document
                .nodes
                .alloc(nodes::Kind::Kern {
                    width: Scaled(1),
                    kind: KernKind::Explicit,
                })
                .unwrap();
            m.append(&mut l, Kind::Material(n), &mut e.document.nodes)
                .unwrap();
        }
        add(
            &mut e,
            &mut m,
            &mut l,
            Atom::new(Class::Rel, character(b'c')),
        );
        let l = e
            .math_to_hlist(&mut m, l, Style::TEXT, true, &mut scratch)
            .unwrap();
        let mut id = l.head();
        let mut penalties = 0;
        while !id.is_null() {
            let n = e.document.nodes.get(id);
            penalties += usize::from(matches!(n.kind, nodes::Kind::Penalty(_)));
            id = n.next;
        }
        assert_eq!(penalties, 0);
        reclaim(&mut e, l);
    }
}
#[test]
fn math_nested_conversion_release_and_failure_use_iterative_disjoint_forests() {
    let depth = if cfg!(miri) { 64 } else { 4096 };
    let bytes = metrics(&[], &[]);
    let mut e = engine(&bytes);
    let mut m = MathStore::default();
    let mut scratch = Scratch::default();
    for fail in [false, true] {
        let mut l = MathList::default();
        let b = e.document.nodes.alloc_box(BoxData::default()).unwrap();
        add(&mut e, &mut m, &mut l, Atom::new(Class::Ord, Field::Box(b)));
        if fail {
            add(
                &mut e,
                &mut m,
                &mut l,
                Atom::new(
                    Class::Ord,
                    Field::Character {
                        family: 15,
                        character: b'a',
                    },
                ),
            );
        }
        for _ in 0..depth {
            let mut outer = MathList::default();
            add(
                &mut e,
                &mut m,
                &mut outer,
                Atom::new(Class::Ord, Field::SubMath(l)),
            );
            l = outer;
        }
        let result = e.math_to_hlist(&mut m, l, Style::TEXT, false, &mut scratch);
        if fail {
            assert!(matches!(result, Err(Error::Font)));
            assert_eq!(e.document.nodes.live_count(), 0);
        } else {
            reclaim(&mut e, result.unwrap());
        }
        assert_eq!(m.live, 0);
        assert!(m.pending.is_empty());
    }
}
#[test]
fn math_capacity_rejection_and_spacing_overflow_release_children_and_outputs() {
    let bytes = metrics(&[], &[]);
    let mut e = engine(&bytes);
    let mut m = MathStore {
        limit: 0,
        ..MathStore::default()
    };
    let b = e.document.nodes.alloc_box(BoxData::default()).unwrap();
    assert!(matches!(
        m.append(
            &mut MathList::default(),
            Kind::Atom(Atom::new(Class::Ord, Field::Box(b))),
            &mut e.document.nodes
        ),
        Err(Error::Capacity)
    ));
    assert_eq!(e.document.nodes.live_count(), 0);
    m.limit = u32::MAX;
    // Ligature insertion fails after a partial converted prefix, with an owned
    // box/script still pending on the same input tree.
    let bytes = metrics(&[[128, b'b', 3, b'c']], &[(b'a', 1, 0)]);
    let mut e = engine(&bytes);
    let mut l = MathList::default();
    for c in b"dab" {
        add(&mut e, &mut m, &mut l, Atom::new(Class::Ord, character(*c)));
    }
    m.limit = m.cells.len() as u32;
    let mut scratch = Scratch::default();
    assert!(matches!(
        e.math_to_hlist(&mut m, l, Style::TEXT, false, &mut scratch),
        Err(Error::Capacity)
    ));
    assert_eq!(m.live, 0);
    assert_eq!(e.document.nodes.live_count(), 0);
    m.limit = u32::MAX;
    let mut l = MathList::default();
    for class in [Class::Ord, Class::Rel, Class::Ord] {
        add(&mut e, &mut m, &mut l, Atom::new(class, character(b'a')));
    }
    e.document.layout_values.mu_glues[crate::layout_values::REGISTERS + 2].value = Glue {
        width: Scaled(i32::MAX),
        ..Glue::default()
    };
    let font = e.document.fonts.descriptors[1];
    e.document.fonts.parameters[font.params + 5] = Scaled::MAX;
    assert!(matches!(
        e.math_to_hlist(&mut m, l, Style::TEXT, false, &mut scratch),
        Err(Error::Overflow)
    ));
    assert_eq!(m.live, 0);
    assert_eq!(e.document.nodes.live_count(), 0);
    let l = e
        .math_to_hlist(
            &mut m,
            MathList::default(),
            Style::DISPLAY,
            false,
            &mut scratch,
        )
        .unwrap();
    reclaim(&mut e, l);
}
#[test]
fn math_every_suspended_handoff_cleans_only_owned_forests_and_reuses_box_slots() {
    let bytes = metrics(&[], &[]);
    let mut e = engine(&bytes);
    let mut m = MathStore::default();
    let mut scratch = Scratch::default();
    // This live ordinary box and its retained version are outside conversion.
    let mut unrelated = List::default();
    let n = e.document.nodes.alloc(nodes::Kind::Penalty(42)).unwrap();
    e.document.nodes.append(&mut unrelated, n);
    let preserved = e.document.nodes.alloc_box(BoxData::new(unrelated)).unwrap();
    let preserved_id = preserved.id();
    let old = e.document.nodes.clone();
    let mut succeeded = false;
    for budget in 0..256 {
        let mut l = MathList::default();
        let mut field = MathList::default();
        add(
            &mut e,
            &mut m,
            &mut field,
            Atom::new(Class::Ord, character(b'x')),
        );
        let mut op = Atom::new(Class::Op, Field::SubMath(field));
        op.limits = Limits::AboveBelow;
        op.fields.sup = Field::SubMath(MathList::default());
        let boxed = e.document.nodes.alloc_box(BoxData::default()).unwrap();
        op.fields.sub = Field::Box(boxed);
        add(&mut e, &mut m, &mut l, op);
        let mut choices = std::array::from_fn(|_| MathList::default());
        for list in &mut choices {
            let boxed = e.document.nodes.alloc_box(BoxData::default()).unwrap();
            let mut a = Atom::new(Class::Ord, Field::Box(boxed));
            a.fields.sup = character(b'b');
            a.fields.sub = character(b'c');
            add(&mut e, &mut m, list, a);
        }
        m.append(&mut l, Kind::Choice(choices), &mut e.document.nodes)
            .unwrap();
        let mut outer = MathList::default();
        let mut a = Atom::new(Class::Ord, Field::SubMath(l));
        a.fields.sup = character(b'e');
        a.fields.sub = Field::SubMath(MathList::default());
        add(&mut e, &mut m, &mut outer, a);
        scratch.interrupt_after(budget);
        match e.math_to_hlist(&mut m, outer, Style::TEXT, false, &mut scratch) {
            Ok(output) => {
                e.document
                    .nodes
                    .release(output, &mut ReleaseScratch::default());
                succeeded = true;
            }
            Err(error) => assert_eq!(error, Error::Capacity),
        }
        assert_eq!(m.live, 0);
        assert!(scratch.is_empty());
        assert_eq!(e.document.nodes.live_count(), 2);
        assert!(
            e.document.nodes.box_slot_count() <= 16,
            "payload slots must be reused, budget {budget}"
        );
        assert!(matches!(
            e.document.nodes.get(preserved_id).kind,
            nodes::Kind::Box(_)
        ));
        assert!(matches!(old.get(preserved_id).kind, nodes::Kind::Box(_)));
        if succeeded {
            assert!(budget > 20);
            break;
        }
    }
    assert!(succeeded);
    scratch.clear_interrupt();
    drop(old);
    e.document
        .nodes
        .release_node(preserved, &mut ReleaseScratch::default());
    assert_eq!(e.document.nodes.live_count(), 0);
}
#[test]
fn math_font_validation_precedes_conversion_and_missing_characters_are_discarded() {
    let bytes = metrics(&[], &[]);
    let mut e = engine(&bytes);
    let mut m = MathStore::default();
    let mut scratch = Scratch::default();
    for size in 0..3 {
        for family in [2, 3] {
            let target = Target::family(size, family);
            let saved = e.document.scalars[target].value;
            e.document.scalars[target].value = 0;
            let b = e.document.nodes.alloc_box(BoxData::default()).unwrap();
            let mut l = MathList::default();
            add(&mut e, &mut m, &mut l, Atom::new(Class::Ord, Field::Box(b)));
            assert!(matches!(
                e.math_to_hlist(&mut m, l, Style::TEXT, false, &mut scratch),
                Err(Error::Font)
            ));
            assert_eq!(m.live, 0);
            assert_eq!(e.document.nodes.live_count(), 0);
            assert!(scratch.is_empty());
            e.document.scalars[target].value = saved;
        }
    }
    let mut l = MathList::default();
    add(&mut e, &mut m, &mut l, Atom::new(Class::Ord, character(0)));
    let output = e
        .math_to_hlist(&mut m, l, Style::TEXT, false, &mut scratch)
        .unwrap();
    assert!(output.is_empty());
    reclaim(&mut e, output);
}
