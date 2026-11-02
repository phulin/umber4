//! Live cursor suffix visibility, affine failures and retained node versions.
use super::*;
use crate::{Error, font_test_data, scalars::Target};
const SETUP: &str = r"\font\f=test\f\noindent\hskip3pt\relax ";
fn prepared<'a>(source: &'a [u8], bytes: &[u8]) -> (Engine<'a>, Read) {
    let mut e = Engine::new(source);
    e.register_tfm(b"test", bytes).unwrap();
    for (c, cat) in [(123, 1), (125, 2), (35, 6)] {
        e.document.scalars[Target::catcode(c)].value = cat;
    }
    loop {
        let read = e.next_expanded().unwrap();
        if matches!(read.meaning.command, 11 | 12) {
            return (e, read);
        }
        e.execute(read).unwrap();
    }
}
fn finish(e: &mut Engine<'_>) -> Result<()> {
    loop {
        let read = e.next_expanded()?;
        if read.meaning.command == cmd::END {
            return Ok(());
        }
        e.execute(read)?;
    }
}
fn glyphs(e: &Engine<'_>) -> Vec<(u8, bool)> {
    let mut p = e.document.text.horizontal.head();
    let mut result = Vec::new();
    while !p.is_null() {
        let n = e.document.nodes.get(p);
        match &n.kind {
            Kind::Character { character, .. } => result.push((*character, false)),
            Kind::Ligature { character, .. } => result.push((*character, true)),
            _ => {}
        }
        p = n.next;
    }
    result
}
fn owned(nodes: &crate::nodes::NodeStore, list: &List, seen: &mut Vec<NodeId>) {
    let mut p = list.head();
    while !p.is_null() {
        assert!(!seen.contains(&p), "duplicate owner {p:?}");
        seen.push(p);
        let node = nodes.get(p);
        match &node.kind {
            Kind::Ligature { components, .. } => owned(nodes, components, seen),
            Kind::Discretionary { pre, post, .. } => {
                owned(nodes, pre, seen);
                owned(nodes, post, seen);
            }
            Kind::Character { .. } | Kind::Kern { .. } | Kind::Glue { .. } | Kind::Language(_) => {}
            other => panic!("unexpected test forest {other:?}"),
        }
        p = node.next;
    }
}
#[test]
fn lookahead_quantities_see_accepted_originals_before_wrapping() {
    for (prefix, query) in [
        (r"\hskip3pt", r"\the\lastskip"),
        (r"\kern3pt", r"\the\lastkern"),
        (r"\penalty7", r"\the\lastpenalty"),
    ] {
        let source = format!(r"{SETUP}{prefix} a{query}\relax\end");
        let (mut e, first) = prepared(source.as_bytes(), &font_test_data::basic());
        e.execute(first).unwrap();
        finish(&mut e).unwrap();
        let actual: Vec<_> = glyphs(&e).into_iter().map(|(c, _)| c).collect();
        assert_eq!(
            actual,
            if query.ends_with("penalty") {
                b"a0".as_slice()
            } else {
                b"a0.0pt".as_slice()
            }
        );
    }
    let source = format!(r"{SETUP}f\romannumeral0 i\the\lastskip\relax\end");
    let (mut e, first) = prepared(source.as_bytes(), &font_test_data::basic());
    e.execute(first).unwrap();
    finish(&mut e).unwrap();
    assert_eq!(
        glyphs(&e),
        [
            (12, true),
            (b'0', false),
            (b'.', false),
            (b'0', false),
            (b'p', false),
            (b't', false)
        ]
    );
}
#[test]
fn lookahead_errors_leave_accepted_suffix_and_release_unconsumed_candidates() {
    for (body, error, expected) in [
        (r"fi\undefined", Error::Undefined, b"fi".as_slice()),
        (r"f\char999", Error::Number, b"f".as_slice()),
    ] {
        let source = format!(r"{SETUP}{body}\end");
        let (mut e, first) = prepared(source.as_bytes(), &font_test_data::basic());
        assert_eq!(e.execute(first), Err(error));
        assert_eq!(
            glyphs(&e),
            expected.iter().map(|&c| (c, false)).collect::<Vec<_>>()
        );
        assert!(e.ligatures.stack.is_empty());
        let mut seen = Vec::new();
        owned(&e.document.nodes, &e.document.text.horizontal, &mut seen);
        owned(&e.document.nodes, &e.document.text.vertical, &mut seen);
        assert_eq!(seen.len(), e.document.nodes.live_count());
    }
    let source = format!(r"{SETUP}fi\noexpand\undefined\relax\end");
    let (mut e, first) = prepared(source.as_bytes(), &font_test_data::basic());
    e.execute(first).unwrap();
    finish(&mut e).unwrap();
    assert_eq!(glyphs(&e), [(12, true)]);
}
#[test]
fn restoring_after_a_lookahead_error_abandons_live_originals_and_reuses_the_cursor() {
    let mut e = Engine::new(br"\font\f=test\f\noindent fi\undefined");
    e.register_tfm(b"test", &font_test_data::basic()).unwrap();
    let initial = e.checkpoint().unwrap();
    for _ in 0..2 {
        assert_eq!(e.advance(usize::MAX), Err(Error::Undefined));
        assert_eq!(glyphs(&e), [(b'f', false), (b'i', false)]);
        let old = e.document.nodes.clone();
        let old_list = e.document.text.horizontal.snapshot();
        e.restore(&initial).unwrap();
        assert_eq!(e.document.nodes.live_count(), 0);
        let mut seen = Vec::new();
        owned(&old, &old_list, &mut seen);
        assert_eq!(seen.len(), 2);
    }
}
#[test]
fn rejected_ligature_header_keeps_the_original_suffix_rooted() {
    for budget in 0..=3 {
        let source = format!(r"{SETUP}fi\relax\end");
        let (mut e, first) = prepared(source.as_bytes(), &font_test_data::basic());
        e.document.nodes.reject_after_allocations(budget);
        assert_eq!(
            e.execute(first),
            if budget < 3 {
                Err(Error::Capacity)
            } else {
                Ok(())
            }
        );
        assert_eq!(
            glyphs(&e),
            match budget {
                0 => vec![],
                1 => vec![(b'f', false)],
                2 => vec![(b'f', false), (b'i', false)],
                _ => vec![(12, true)],
            }
        );
        let mut seen = Vec::new();
        owned(&e.document.nodes, &e.document.text.horizontal, &mut seen);
        owned(&e.document.nodes, &e.document.text.vertical, &mut seen);
        assert_eq!(seen.len(), e.document.nodes.live_count());
    }
}
#[test]
fn every_cursor_allocation_cutpoint_has_disjoint_owners_and_retained_versions() {
    let mut cases = vec![(font_test_data::basic(), b"ffi AV-f".as_slice())];
    for op in [2, 3, 7, 11] {
        cases.push((
            font_test_data::tfm(&[[128, b'b', op, b'c']], &[(b'a', 1, 0)]),
            b"ab",
        ));
    }
    for (bytes, body) in cases {
        let source = format!(r"{SETUP}{}\relax\end", std::str::from_utf8(body).unwrap());
        let mut success = false;
        for budget in 0..40 {
            let (mut e, first) = prepared(source.as_bytes(), &bytes);
            // Include explicit-hyphen discretionary failure as well as kern/header failures.
            e.document.fonts.descriptors[1].hyphen = body[0] as i32;
            let old = e.document.nodes.clone();
            let old_list = e.document.text.horizontal.snapshot();
            let old_tail = old.get(old_list.tail()).next;
            e.document.nodes.reject_after_allocations(budget);
            let result = e.execute(first);
            assert!(result == Ok(()) || result == Err(Error::Capacity));
            let mut seen = Vec::new();
            owned(&e.document.nodes, &e.document.text.horizontal, &mut seen);
            owned(&e.document.nodes, &e.document.text.vertical, &mut seen);
            assert_eq!(seen.len(), e.document.nodes.live_count(), "budget {budget}");
            assert!(e.ligatures.stack.is_empty());
            assert_eq!(old.get(old_list.tail()).next, old_tail);
            assert!(matches!(old.get(old_list.tail()).kind, Kind::Glue { .. }));
            let mut release = ReleaseScratch::default();
            let h = std::mem::take(&mut e.document.text.horizontal);
            e.document.nodes.release(h, &mut release);
            let v = std::mem::take(&mut e.document.text.vertical);
            e.document.nodes.release(v, &mut release);
            assert_eq!(e.document.nodes.live_count(), 0);
            if result.is_ok() {
                success = true;
                break;
            }
        }
        assert!(success);
    }
}
#[test]
fn retaining_ligature_reset_excludes_the_just_published_hyphen_discretionary() {
    for op in [7, 11] {
        let bytes = font_test_data::tfm(&[[128, b'b', op, b'c']], &[(b'a', 1, 0)]);
        let source = format!(r"{SETUP}ab\relax\end");
        let (mut e, first) = prepared(source.as_bytes(), &bytes);
        e.document.fonts.descriptors[1].hyphen = b'a' as i32;
        e.execute(first).unwrap();
        let mut p = e.document.text.horizontal.head();
        let mut discs = 0;
        let mut ligatures = 0;
        while !p.is_null() {
            let node = e.document.nodes.get(p);
            match &node.kind {
                Kind::Discretionary { .. } => discs += 1,
                Kind::Ligature {
                    components,
                    character,
                    ..
                } => {
                    assert_eq!(*character, b'c');
                    assert!(components.is_empty());
                    ligatures += 1;
                }
                _ => {}
            }
            p = node.next;
        }
        assert_eq!((discs, ligatures), (1, 1));
    }
}
#[test]
fn language_marker_stays_before_ligature_components_and_checkpoint_replays() {
    let source = br"\font\f=test\f\language=2\noindent fi\relax\par\end";
    let mut e = Engine::new(source);
    e.register_tfm(b"test", &font_test_data::basic()).unwrap();
    let saved = e.checkpoint().unwrap();
    for _ in 0..2 {
        // Stop manually before paragraph completion so components remain inspectable.
        loop {
            let r = e.next_expanded().unwrap();
            if r.meaning.command == cmd::PAR {
                break;
            }
            e.execute(r).unwrap();
        }
        let mut p = e.document.text.horizontal.head();
        let mut found = false;
        while !p.is_null() {
            let n = e.document.nodes.get(p);
            if let Kind::Ligature { components, .. } = &n.kind {
                let mut chars = Vec::new();
                let mut q = components.head();
                while !q.is_null() {
                    let c = e.document.nodes.get(q);
                    let Kind::Character { character, .. } = c.kind else {
                        panic!("non-character component")
                    };
                    chars.push(character);
                    q = c.next;
                }
                assert_eq!(chars, b"fi");
                found = true;
            }
            p = n.next;
        }
        assert!(found);
        e.restore(&saved).unwrap();
    }
}

#[test]
fn hyphen_binding_survives_expanded_fontdimen_reads_and_refreshes_after_assignment() {
    let source = br"\font\f=test\f\hyphenchar\f=97\def\A{a}\noindent a\the\fontdimen255\f\A\hyphenchar\f=353 a\hyphenchar\f=-159 a\relax\end";
    let mut e = Engine::new(source);
    e.register_tfm(b"test", &font_test_data::basic()).unwrap();
    for (c, cat) in [(123, 1), (125, 2), (35, 6)] {
        e.document.scalars[Target::catcode(c)].value = cat;
    }
    let initial = e.checkpoint().unwrap();
    for _ in 0..2 {
        finish(&mut e).unwrap();
        assert_eq!(
            glyphs(&e).into_iter().map(|(c, _)| c).collect::<Vec<_>>(),
            b"a0.0ptaaa"
        );
        let mut p = e.document.text.horizontal.head();
        let mut discs = 0;
        while !p.is_null() {
            let node = e.document.nodes.get(p);
            discs += usize::from(matches!(node.kind, Kind::Discretionary { .. }));
            p = node.next;
        }
        assert_eq!(discs, 2);
        assert_eq!(e.document.fonts.descriptors[1].count, 255);
        assert_eq!(e.document.fonts.descriptors[1].hyphen, -159);
        e.restore(&initial).unwrap();
    }
}

#[test]
#[ignore = "native run-local hyphen bindings: set UMBER_PDFTEX"]
fn upstream_hyphen_assignments_end_runs_and_preserve_signed_values() {
    let oracle = crate::page_oracle_tests::Oracle::new();
    let bytes = font_test_data::basic();
    oracle.compare(
        &bytes,
        r"\hsize=8pt\pretolerance=10000",
        r"\noindent\hyphenchar\f=97 aaa\hyphenchar\f=98 bbbb\par",
    );
    for value in [-159, 353] {
        oracle.compare(
            &bytes,
            &format!(r"\hsize=8pt\pretolerance=10000\hyphenchar\f={value}"),
            r"\noindent aaaa\par",
        );
    }
}
