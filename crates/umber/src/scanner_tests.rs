use super::*;

const BOOT: &str = "\\catcode123=1 \\catcode125=2 \\catcode35=6 \\catcode94=7\\relax\n";

fn complete(engine: &mut Engine<'_>) -> crate::Outcome {
    engine.run_input().unwrap();
    engine.outcome()
}

#[test]
fn outer_prefixes_and_normal_context_exceptions_match_native() {
    let outcome = Engine::new(include_bytes!("../../../fixtures/mvp/outer.tex"))
        .run()
        .unwrap();
    let expected: Vec<_> = include_bytes!("../../../fixtures/mvp/outer.expected")
        .split(|&byte| byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(<[u8]>::to_vec)
        .collect();
    assert_eq!(outcome.messages, expected);
    assert_eq!(outcome.counts[..5], [2, 1, 2, 2, 2]);
}

#[test]
fn outer_flags_aliases_and_pending_sources_replay_at_each_boundary() {
    let source = include_bytes!("../../../fixtures/mvp/outer.tex");
    let expected = Engine::new(source).run().unwrap();
    let mut engine = Engine::new(source);
    loop {
        let saved = engine.checkpoint().unwrap();
        let prefix = engine.outcome();
        let finished = complete(&mut engine);
        assert_eq!(finished.messages, expected.messages);
        assert_eq!(finished.counts, expected.counts);
        engine.restore(&saved).unwrap();
        assert_eq!(engine.scanner, Scanner::Normal);
        assert_eq!(engine.outcome().messages, prefix.messages);
        if engine.advance(1).unwrap() == crate::Progress::Complete {
            break;
        }
    }
}

#[test]
fn forbidden_outer_reads_reject_and_release_failed_scanner_roots_on_restore() {
    let sources: &[&[u8]] = &[
        include_bytes!("../../../fixtures/mvp/failures/outer-definition.tex"),
        include_bytes!("../../../fixtures/mvp/failures/outer-argument.tex"),
        include_bytes!("../../../fixtures/mvp/failures/outer-prefix.tex"),
        include_bytes!("../../../fixtures/mvp/failures/outer-skipping.tex"),
        include_bytes!("../../../fixtures/mvp/failures/outer-balanced.tex"),
        include_bytes!("../../../fixtures/mvp/failures/outer-protected.tex"),
        include_bytes!("../../../fixtures/mvp/failures/outer-raw-noexpand.tex"),
        include_bytes!("../../../fixtures/mvp/failures/outer-detokenize.tex"),
        include_bytes!("../../../fixtures/mvp/failures/outer-unexpanded.tex"),
        include_bytes!("../../../fixtures/mvp/failures/outer-message.tex"),
        include_bytes!("../../../fixtures/mvp/failures/outer-stored-argument.tex"),
        include_bytes!("../../../fixtures/mvp/failures/outer-parameter.tex"),
        include_bytes!("../../../fixtures/mvp/failures/outer-case-change.tex"),
        include_bytes!("../../../fixtures/mvp/failures/outer-paragraph.tex"),
    ];
    for source in sources {
        let mut engine = Engine::new(source);
        let initial = engine.checkpoint().unwrap();
        let mut slots = None;
        for _ in 0..3 {
            assert_eq!(
                engine.run_input(),
                Err(Error::Outer),
                "{}",
                String::from_utf8_lossy(source)
            );
            assert_eq!(engine.scanner, Scanner::Normal);
            assert!(!engine.can_checkpoint());
            assert_eq!(engine.advance(0), Err(Error::Checkpoint));
            engine.restore(&initial).unwrap();
            assert!(engine.can_checkpoint());
            assert_eq!(engine.values.retained().0, 0);
            assert!(engine.packets.iter().all(|packet| packet.owners.is_empty()));
            let allocated = engine.values.retained().1;
            if let Some(previous) = slots {
                assert_eq!(allocated, previous);
            }
            slots = Some(allocated);
        }
    }
}

#[test]
fn nested_scanners_restore_enclosing_validity_and_error_priority() {
    for (body, error) in [
        (r"\def\bad#0{\out}", Error::Syntax),
        (r"\edef\bad{\undefined\out}", Error::Undefined),
        (r"\edef\bad{\string\out\out}", Error::Outer),
        (r"\edef\bad{\meaning\out\out}", Error::Outer),
        (r"\edef\bad{\ifdefined\out\fi\out}", Error::Outer),
        (r"\edef\bad{\ifx\out\out\fi\out}", Error::Outer),
        (r"\edef\bad{\ifcsname\out\endcsname\fi}", Error::Outer),
        (r"\edef\bad{\noexpand\out\undefined}", Error::Undefined),
        (r"\def\id#1{#1}\edef\bad{\id{A}\out}", Error::Outer),
        (r"\edef\bad{\unexpanded{ABC}\out}", Error::Outer),
        (r"\edef\bad{\detokenize{ABC}\out}", Error::Outer),
        (r"\message{\iffalse ignored\fi\out}", Error::Outer),
        (r"\def\take#1{}\take\par\out", Error::Argument),
        (r"\let\par=\out\def\take#1{}\take\par", Error::Outer),
        (
            r"\def\carrier{\take{\par}}\def\take#1{}\outer\def\par{}\carrier",
            Error::Outer,
        ),
        (
            r"\def\carrier{\take{\par}}\def\take#1{#1}\outer\def\par{}\carrier",
            Error::Outer,
        ),
        (r"\def\bad{", Error::Eof),
        (r"\outer\count0=7", Error::Syntax),
    ] {
        let source = format!(r"{BOOT}\outer\def\out{{}}{body}");
        let mut engine = Engine::new(source.as_bytes());
        assert_eq!(engine.run_input(), Err(error), "{body}");
        assert_eq!(engine.scanner, Scanner::Normal);
        assert!(!engine.can_checkpoint());
    }
}

#[test]
fn failed_outer_name_expansion_returns_spill_scratch() {
    let prefix = "longname".repeat(20);
    let source = format!(
        r"{BOOT}\def\prefix{{{prefix}}}\outer\def\out{{}}\edef\bad{{\csname\prefix\out\endcsname}}"
    );
    let mut engine = Engine::new(source.as_bytes());
    let initial = engine.checkpoint().unwrap();
    for _ in 0..3 {
        assert_eq!(engine.run_input(), Err(Error::Outer));
        assert_eq!(engine.scanner, Scanner::Normal);
        assert_eq!(engine.name_spills.len(), 1);
        assert!(engine.name_spills[0].is_empty());
        assert_eq!(engine.names.lookup(prefix.as_bytes()), None);
        engine.restore(&initial).unwrap();
        assert_eq!(engine.values.retained().0, 0);
    }
}

#[test]
fn saved_outer_definition_keeps_flags_and_backing_after_registry_drop() {
    let source = format!(r"{BOOT}\outer\long\def\word#1{{#1}}{{\def\word{{LOCAL}}\relax}}\end");
    let mut engine = Engine::new(source.as_bytes());
    while !engine
        .document
        .save
        .iter()
        .any(|save| matches!(save, crate::execute::Save::Meaning(_, _, Some(_))))
    {
        engine.advance(1).unwrap();
    }
    let (binding, owner) = engine
        .document
        .save
        .iter()
        .find_map(|save| {
            if let crate::execute::Save::Meaning(_, binding, Some(owner)) = save {
                Some((*binding, owner.clone()))
            } else {
                None
            }
        })
        .unwrap();
    let saved = engine.checkpoint().unwrap();
    complete(&mut engine);
    assert!(
        engine
            .resolve(Token::cs(engine.names.lookup(b"word").unwrap()))
            .is_outer_macro()
    );
    engine.restore(&saved).unwrap();
    complete(&mut engine);
    drop(engine);
    assert!(binding.meaning().is_outer_macro());
    assert!(owner.definition().long);
    assert!(
        owner
            .tokens
            .get(owner.definition().body.start)
            .is_parameter()
    );
    drop(saved);
}

#[test]
fn nested_contexts_restore_on_errors_and_do_not_form_checkpoint_roots() {
    let mut engine = Engine::new(br"\end");
    assert!(engine.can_checkpoint());
    engine
        .with_scanner(Scanner::Defining, |engine| {
            assert!(!engine.can_checkpoint());
            assert!(matches!(engine.checkpoint(), Err(Error::Checkpoint)));
            assert_eq!(
                engine.with_scanner(Scanner::Normal, |_| Err::<(), _>(Error::Syntax)),
                Err(Error::Syntax)
            );
            assert_eq!(engine.scanner, Scanner::Defining);
            Ok(())
        })
        .unwrap();
    assert_eq!(engine.scanner, Scanner::Normal);
    assert!(engine.can_checkpoint());
}
