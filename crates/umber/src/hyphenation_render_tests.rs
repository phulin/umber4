use super::*;
use crate::{Engine, Error, Progress, font_test_data, token::cmd};

fn source(settings: &str, body: &str) -> String {
    let mut s = String::from(
        r"\catcode123=1\catcode125=2\font\f=test\f\hyphenchar\f=45\hsize=200pt\vsize=1000pt\parfillskip=0pt plus1fil\pretolerance=-1\lefthyphenmin=1\righthyphenmin=1 ",
    );
    for c in b'a'..=b'z' {
        s.push_str(&format!("\\lccode{c}={c}"));
    }
    s.push_str(settings);
    s.push_str(body);
    s.push_str(r"\par\end");
    s
}
fn prepared<'a>(source: &'a str, tfm: &[u8]) -> Engine<'a> {
    let mut e = Engine::new(source.as_bytes());
    e.register_tfm(b"test", tfm).unwrap();
    loop {
        let read = e.next_expanded().unwrap();
        if read.meaning.command == cmd::PAR {
            return e;
        }
        e.execute(read).unwrap();
    }
}
fn apply(e: &mut Engine<'_>, scratch: &mut Scratch) -> Result<()> {
    let glue = e.document.text.horizontal.head();
    assert!(matches!(e.document.nodes.get(glue).kind, Kind::Glue { .. }));
    let mut language = e.document.text.initial_language;
    after_glue(
        glue,
        &mut e.document.nodes,
        &mut e.document.text.horizontal,
        &Inputs {
            scalars: &e.document.scalars,
            fonts: &e.document.fonts,
            catalog: &e.font_catalog,
            tables: &e.document.hyphenation,
        },
        &mut language,
        scratch,
    )
}
fn fingerprint(nodes: &NodeStore, mut p: NodeId) -> Vec<(NodeId, NodeId, String)> {
    let mut result = Vec::new();
    while !p.is_null() {
        let node = nodes.get(p);
        result.push((p, node.next, format!("{:?}", node.kind)));
        p = node.next;
    }
    result
}
fn glyphs(nodes: &NodeStore, mut p: NodeId) -> Vec<u8> {
    let mut result = Vec::new();
    while !p.is_null() {
        let node = nodes.get(p);
        if let Kind::Character { character, .. } | Kind::Ligature { character, .. } = node.kind {
            result.push(character);
        }
        p = node.next;
    }
    result
}
#[test]
fn automatic_kern_discretionary_replaces_nodes_and_preserves_original_version() {
    let text = source(r"\hyphenation{ab-cdef}", r"\noindent\ abcdef ");
    let mut e = prepared(
        &text,
        &font_test_data::tfm(&[[128, b'c', 128, 0]], &[(b'b', 1, 0)]),
    );
    let old_nodes = e.document.nodes.clone();
    let old_list = e.document.text.horizontal.snapshot();
    let old_shape = fingerprint(&old_nodes, old_list.head());
    let mut scratch = Scratch::default();
    apply(&mut e, &mut scratch).unwrap();
    let mut p = e.document.text.horizontal.head();
    let mut found = 0;
    while !p.is_null() {
        let node = e.document.nodes.get(p);
        if let Kind::Discretionary {
            pre,
            post,
            replace_count,
        } = &node.kind
        {
            found += 1;
            assert_eq!(glyphs(&e.document.nodes, pre.head()), b"b-");
            assert!(post.is_empty());
            assert_eq!(*replace_count, 2);
            assert!(matches!(
                e.document.nodes.get(node.next).kind,
                Kind::Character {
                    character: b'b',
                    ..
                }
            ));
            assert!(matches!(
                e.document
                    .nodes
                    .get(e.document.nodes.get(node.next).next)
                    .kind,
                Kind::Kern {
                    explicit: false,
                    ..
                }
            ));
        }
        p = node.next;
    }
    assert_eq!(found, 1);
    assert_eq!(fingerprint(&old_nodes, old_list.head()), old_shape);
    let shape = fingerprint(&e.document.nodes, e.document.text.horizontal.head());
    apply(&mut e, &mut scratch).unwrap();
    assert_eq!(
        fingerprint(&e.document.nodes, e.document.text.horizontal.head()),
        shape,
        "existing discretionaries prevent repeated reconstruction"
    );
    let list = std::mem::take(&mut e.document.text.horizontal);
    e.document.nodes.release(list, &mut scratch.release);
    assert_eq!(e.document.nodes.live_count(), 0);
    assert_eq!(fingerprint(&old_nodes, old_list.head()), old_shape);
}
#[test]
fn every_reconstruction_allocation_failure_keeps_original_owned_and_reusable() {
    for bytes in [
        font_test_data::basic(),
        font_test_data::tfm(&[[128, b'a', 1, b'X']], &[(b'.', 1, 0)]),
    ] {
        let text = source(r"\hyphenation{af-fi-ne}", r"\noindent\ .affine ");
        let mut e = prepared(&text, &bytes);
        let original = fingerprint(&e.document.nodes, e.document.text.horizontal.head());
        let live = e.document.nodes.live_count();
        let old_nodes = e.document.nodes.clone();
        let old_list = e.document.text.horizontal.snapshot();
        let mut scratch = Scratch::default();
        let mut reached_success = false;
        for budget in 0..250 {
            scratch.reconstitution.fail_after(Some(budget));
            match apply(&mut e, &mut scratch) {
                Err(error) => {
                    assert_eq!(error, Error::Capacity);
                    assert_eq!(e.document.nodes.live_count(), live, "budget {budget}");
                    assert_eq!(
                        fingerprint(&e.document.nodes, e.document.text.horizontal.head()),
                        original
                    );
                }
                Ok(()) => {
                    reached_success = true;
                    break;
                }
            }
            assert_eq!(fingerprint(&old_nodes, old_list.head()), original);
        }
        assert!(reached_success);
        scratch.reconstitution.fail_after(None);
        let list = std::mem::take(&mut e.document.text.horizontal);
        e.document.nodes.release(list, &mut scratch.release);
        assert_eq!(e.document.nodes.live_count(), 0);
        assert_eq!(fingerprint(&old_nodes, old_list.head()), original);
    }
}
#[test]
fn ligature_stack_and_node_storage_reuse_survive_repeated_version_branches() {
    let bytes = font_test_data::tfm(&[[128, b'c', 3, b'X']], &[(b'b', 1, 0)]);
    let text = source(r"\hyphenation{a-b-c-d-e-f}", r"\noindent\ abcdef ");
    let mut e = prepared(&text, &bytes);
    let old_nodes = e.document.nodes.clone();
    let old_list = e.document.text.horizontal.snapshot();
    let mut scratch = Scratch::default();
    apply(&mut e, &mut scratch).unwrap();
    let expected = glyphs(&e.document.nodes, e.document.text.horizontal.head());
    let capacity = scratch.reconstitution.capacity();
    assert!(capacity > 0);
    for _ in 0..30 {
        let list = std::mem::take(&mut e.document.text.horizontal);
        e.document.nodes.release(list, &mut scratch.release);
        assert_eq!(e.document.nodes.live_count(), 0);
        e.document.nodes = old_nodes.clone();
        e.document.text.horizontal = old_list.snapshot();
        apply(&mut e, &mut scratch).unwrap();
        assert_eq!(
            glyphs(&e.document.nodes, e.document.text.horizontal.head()),
            expected
        );
        assert_eq!(scratch.reconstitution.capacity(), capacity);
    }
}
#[test]
fn paragraph_replay_preserves_automatic_breaks_and_detached_output() {
    let text = source(
        r"\hyphenation{af-fi-ne dif-fi-cult}\hsize=25pt\tolerance=10000\count0=1\relax",
        r"\noindent\ affine difficult affine difficult affine difficult",
    );
    let mut e = Engine::new(text.as_bytes());
    e.register_tfm(b"test", &font_test_data::basic()).unwrap();
    while e.outcome().counts[0] != 1 {
        e.advance(1).unwrap();
    }
    let before = e.checkpoint().unwrap();
    let mut expected = None;
    let mut retained = None;
    for _ in 0..4 {
        assert_eq!(e.advance(usize::MAX), Ok(Progress::Complete));
        let pages = e.document.page.output.clone();
        let observed: Vec<_> = pages
            .iter()
            .flat_map(|p| p.placements().collect::<Vec<_>>())
            .map(|p| format!("{p:?}"))
            .collect();
        if let Some(expected) = &expected {
            assert_eq!(&observed, expected);
        } else {
            expected = Some(observed);
            retained = Some(pages);
        }
        assert!(!e.document.page.output.is_empty());
        assert_eq!(e.document.nodes.live_count(), 0);
        e.restore(&before).unwrap();
    }
    let observed: Vec<_> = retained
        .unwrap()
        .iter()
        .flat_map(|p| p.placements().collect::<Vec<_>>())
        .map(|p| format!("{p:?}"))
        .collect();
    assert_eq!(Some(observed), expected);
}
#[test]
fn growing_ligatures_keep_127_replacements_and_forget_only_larger_alternatives() {
    let tfm = font_test_data::tfm(
        &[
            [0, b'a', 3, b'b'],
            [128, b'b', 7, b'c'],
            [128, b'b', 7, b'd'],
            [128, b'b', 7, b'e'],
            [128, b'b', 0, b'x'],
            [128, b'a', 0, b'a'],
        ],
        &[
            (b'a', 1, 0),
            (b'c', 1, 2),
            (b'd', 1, 3),
            (b'e', 1, 4),
            (b'x', 1, 5),
        ],
    );
    let mut scratch = Scratch::default();
    for length in [42, 43, 44, 63] {
        let text = source(
            r"\patterns{a1a}",
            &format!(r"\noindent\ {} ", "a".repeat(length)),
        );
        let mut e = prepared(&text, &tfm);
        let original_glyphs = glyphs(&e.document.nodes, e.document.text.horizontal.head());
        apply(&mut e, &mut scratch).unwrap();
        assert_eq!(
            glyphs(&e.document.nodes, e.document.text.horizontal.head()),
            original_glyphs
        );
        let mut replacements = Vec::new();
        let mut p = e.document.text.horizontal.head();
        while !p.is_null() {
            let node = e.document.nodes.get(p);
            if let Kind::Discretionary { replace_count, .. } = node.kind {
                replacements.push(replace_count);
            }
            p = node.next;
        }
        assert_eq!(
            replacements,
            if length <= 43 {
                vec![(3 * (length - 1) + 1) as u8]
            } else {
                vec![]
            }
        );
        let list = std::mem::take(&mut e.document.text.horizontal);
        e.document.nodes.release(list, &mut scratch.release);
        assert_eq!(e.document.nodes.live_count(), 0);
    }
}
