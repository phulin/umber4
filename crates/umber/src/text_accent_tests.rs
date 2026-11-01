use super::*;
use crate::{Progress, font_test_data, nodes::NodeId, scalars::Target};

fn engine(source: &[u8]) -> Engine<'_> {
    let mut e = Engine::new(source);
    e.register_tfm(b"test", &font_test_data::basic()).unwrap();
    e
}
fn build(e: &mut Engine<'_>) -> Result<()> {
    loop {
        let read = e.next_expanded()?;
        if read.meaning.command == cmd::END {
            return Ok(());
        }
        e.execute(read)?;
    }
}
fn shape(nodes: &crate::nodes::NodeStore, mut p: NodeId) -> Vec<(NodeId, NodeId, String)> {
    let mut result = Vec::new();
    while !p.is_null() {
        let node = nodes.get(p);
        result.push((p, node.next, format!("{:?}", node.kind)));
        p = node.next;
    }
    result
}
#[test]
fn text_accent_missing_glyphs_readback_and_space_factor() {
    let mut e = engine(br"\catcode123=1\catcode125=2\font\f=test\f\noindent\spacefactor=2345\accent0\message{\the\spacefactor}\accent97\char0\message{\the\spacefactor}\spacefactor=1777\accent97\message{\the\spacefactor}\end");
    build(&mut e).unwrap();
    assert_eq!(
        e.outcome().message_bytes(),
        [b"2345".to_vec(), b"1000".to_vec(), b"1000".to_vec()]
    );
    let mut p = e.document.text.horizontal.head();
    let mut glyphs = Vec::new();
    while !p.is_null() {
        let node = e.document.nodes.get(p);
        if let Kind::Character { character, .. } = node.kind {
            glyphs.push(character);
        } else {
            panic!("lone accents remain raw characters: {:?}", node.kind);
        }
        p = node.next;
    }
    assert_eq!(glyphs, b"aa");
    // A missing accent leaves setbox outside the forbidden assignment window.
    let mut e =
        engine(br"\catcode123=1\catcode125=2\font\f=test\f\noindent\accent0\setbox0=\hbox{b}\end");
    build(&mut e).unwrap();
    assert!(!e.document.boxes.head(0).is_null());
    assert!(e.document.text.horizontal.is_empty());
}
#[test]
fn text_accent_direct_base_has_no_ligatures_sfcode_hyphen_disc_or_language_fix() {
    let mut e =
        engine(br"\font\f=test\f\hyphenchar\f=45\noindent\accent97\language=12 fi\accent97-\end");
    build(&mut e).unwrap();
    let mut p = e.document.text.horizontal.head();
    let mut glyphs = Vec::new();
    let mut language_at = None;
    let mut kerns = 0;
    while !p.is_null() {
        let node = e.document.nodes.get(p);
        match node.kind {
            Kind::Character { character, .. } => glyphs.push(character),
            Kind::Language(language) => {
                assert_eq!(language.number, 12);
                language_at = Some(glyphs.len());
            }
            Kind::Kern { kind, .. } => {
                assert_eq!(kind, KernKind::Accent);
                kerns += 1;
            }
            Kind::Box(_) => {}
            _ => panic!("unexpected accent component: {:?}", node.kind),
        }
        p = node.next;
    }
    assert_eq!(glyphs, b"fi-");
    assert_eq!(
        language_at,
        Some(1),
        "ordinary i, after the directly appended base f, fixes language"
    );
    assert_eq!(kerns, 4);
    assert_eq!(e.document.text.space_factor, 1000);
}
#[test]
fn text_accent_everypar_precedes_operand_and_assignments_supply_the_base() {
    let mut e = engine(br"\catcode123=1\catcode125=2\font\f=test\everypar{\f\count0=7}\accent97\global\def\b{b}\relax\b\end");
    build(&mut e).unwrap();
    assert_eq!(e.document.scalars[Target::count(0)].value, 7);
    assert_eq!(e.document.text.space_factor, 1000);
    assert!(
        matches!(
            e.document.nodes.get(e.document.text.horizontal.head()).kind,
            Kind::Box(_)
        ),
        "implicit paragraph starts with indentation"
    );
    let tail = e.document.text.horizontal.tail();
    assert!(matches!(
        e.document.nodes.get(tail).kind,
        Kind::Character {
            character: b'b',
            font: 1
        }
    ));
    let mut e = engine(br"\font\f=test\f\afterassignment b\noindent\accent97\count0=3 a\end");
    build(&mut e).unwrap();
    let tail = e.document.text.horizontal.tail();
    assert!(matches!(
        e.document.nodes.get(tail).kind,
        Kind::Character {
            character: b'a',
            ..
        }
    ));
    assert_eq!(e.document.scalars[Target::count(0)].value, 3);
}
#[test]
fn text_accent_every_allocation_handoff_preserves_original_cow_version() {
    for equal in [false, true] {
        let source = format!(
            r"\font\f=test\f{}\noindent A\accent97 b\end",
            if equal {
                r"\fontdimen5\f=\fontcharht\f98 "
            } else {
                ""
            }
        );
        let mut succeeded = false;
        for budget in 0..8 {
            let mut e = engine(source.as_bytes());
            loop {
                let read = e.next_expanded().unwrap();
                if read.meaning.command == cmd::ACCENT {
                    break;
                }
                e.execute(read).unwrap();
            }
            let old_nodes = e.document.nodes.clone();
            let old_list = e.document.text.horizontal.snapshot();
            let original = shape(&old_nodes, old_list.head());
            let live = e.document.nodes.live_count();
            let mut work = Work {
                fail_after: Some(budget),
                ..Work::default()
            };
            let result = e.text_accent_owned(&mut work);
            assert!(work.accent.is_empty() && work.base.is_empty() && work.output.is_empty());
            assert_eq!(shape(&old_nodes, old_list.head()), original);
            if result.is_err() {
                assert_eq!(result, Err(Error::Capacity));
                assert_eq!(
                    shape(&e.document.nodes, e.document.text.horizontal.head()),
                    original
                );
                assert_eq!(e.document.nodes.live_count(), live);
            } else {
                succeeded = true;
            }
            let list = std::mem::take(&mut e.document.text.horizontal);
            e.document.nodes.release(list, &mut e.page_release);
            assert_eq!(e.document.nodes.live_count(), 0);
            if succeeded {
                break;
            }
        }
        assert!(succeeded);
    }
}
#[test]
fn text_accent_scanner_failures_release_detached_glyph_and_restore() {
    for (operand, error) in [
        (r"999 b", Error::Number),
        (r"97\char999", Error::Number),
        (r"97\global\setbox0=\hbox{b}", Error::Syntax),
        (r"97\setbox999=\hbox{b}", Error::Number),
        (r"97\setbox0\undefined", Error::Undefined),
        (r"97\count0=\undefined b", Error::Undefined),
    ] {
        let source = format!(
            r"\catcode123=1\catcode125=2\font\f=test\f\count0=1\relax\noindent A\accent{operand}\end"
        );
        let mut e = engine(source.as_bytes());
        while e.outcome().counts[0] != 1 {
            e.advance(1).unwrap();
        }
        let before = e.checkpoint().unwrap();
        for _ in 0..3 {
            assert_eq!(e.advance(usize::MAX), Err(error), "{operand}");
            assert_eq!(
                e.document.nodes.live_count(),
                1,
                "only the original A survives: {operand}"
            );
            assert!(e.document.box_frames.is_empty());
            e.restore(&before).unwrap();
        }
    }
    let mut e = engine(br"\catcode36=3\noindent$\accent\undefined$\end");
    assert_eq!(
        e.advance(usize::MAX),
        Err(Error::Syntax),
        "math rejection precedes operand expansion"
    );
}
#[test]
fn text_accent_checkpoints_replay_detached_output_and_reuse_existing_storage() {
    let mut e = engine(br"\catcode123=1\catcode125=2\font\f=test\f\hsize=100pt\vsize=1000pt\count0=1\relax\noindent\accent97 b \accent97 f \accent97 i\par\end");
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
