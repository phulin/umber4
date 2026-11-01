use super::*;

const BOOT: &str = "\\catcode123=1 \\catcode125=2 \\catcode35=6\\relax\n";

fn write(destination: DiagnosticDestination, bytes: &[u8]) -> Diagnostic {
    Diagnostic::Write {
        destination,
        bytes: bytes.to_vec(),
    }
}

#[test]
fn native_immediate_fixture_preserves_all_records_and_destinations() {
    let expected: Vec<_> = include_bytes!("../../../fixtures/mvp/immediate-write.expected")
        .split(|&byte| byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| match line[0] {
            b'M' => Diagnostic::Message(line[2..].to_vec()),
            b'L' => write(DiagnosticDestination::LogOnly, &line[2..]),
            b'T' => write(DiagnosticDestination::TerminalAndLog, &line[2..]),
            _ => panic!("unknown native diagnostic kind"),
        })
        .collect();
    let outcome = Engine::new(include_bytes!("../../../fixtures/mvp/immediate-write.tex"))
        .run()
        .unwrap();
    assert_eq!(outcome.diagnostics, expected);
}

#[test]
fn immediate_backs_exactly_one_nonextension_token_after_expansion() {
    for token in [Token::SPACE, FROZEN_RELAX] {
        let mut engine = Engine::new(br"\end");
        engine.back(token).unwrap();
        engine.extension(IMMEDIATE).unwrap();
        assert_eq!(engine.next_token().unwrap().token, token);
        assert_eq!(engine.next_token().unwrap().meaning.command, cmd::END);
    }
    let mut engine = Engine::new(br"\special");
    engine.extension(IMMEDIATE).unwrap();
    assert_eq!(
        engine.next_token().unwrap().meaning,
        Meaning {
            command: cmd::EXTENSION,
            value: 3
        }
    );
    let source = format!(
        r"{BOOT}\def\release{{\message{{\the\count0}}}}\afterassignment\release\immediate\write-1{{LOG}}\count0=7\relax\end"
    );
    let outcome = Engine::new(source.as_bytes()).run().unwrap();
    assert_eq!(
        outcome.diagnostics,
        [
            write(DiagnosticDestination::LogOnly, b"LOG"),
            Diagnostic::Message(b"7".to_vec())
        ]
    );
}

#[test]
fn completed_write_prefixes_replay_without_retaining_token_values() {
    let source = format!(
        r"{BOOT}\def\value{{PAYLOAD}}\message{{BEFORE}}\immediate\write-1{{\value}}\immediate\write16{{LAST}}\end"
    );
    let mut engine = Engine::new(source.as_bytes());
    while engine.outcome().diagnostics.is_empty() {
        engine.advance(1).unwrap();
    }
    let saved = engine.checkpoint().unwrap();
    let baseline_values = engine.values.retained().0;
    let prefix = engine.outcome();
    let expected = [
        Diagnostic::Message(b"BEFORE".to_vec()),
        write(DiagnosticDestination::LogOnly, b"PAYLOAD"),
        write(DiagnosticDestination::TerminalAndLog, b"LAST"),
    ];
    for _ in 0..3 {
        engine.run_input().unwrap();
        assert_eq!(engine.outcome().diagnostics, expected);
        assert_eq!(engine.values.retained().0, baseline_values);
        assert!(engine.document.sources.is_empty());
        assert_eq!(engine.document.text.mode, Mode::Vertical);
        engine.restore(&saved).unwrap();
        assert_eq!(engine.outcome().diagnostics, prefix.diagnostics);
    }
    engine.run_input().unwrap();
    let published = engine.outcome();
    drop(engine);
    drop(saved);
    assert_eq!(published.diagnostics, expected);
    assert_eq!(
        prefix.diagnostics,
        [Diagnostic::Message(b"BEFORE".to_vec())]
    );
}

#[test]
fn write_conditionals_remain_document_state_across_handoff() {
    let source = format!(r"{BOOT}\immediate\write-1{{\iftrue YES}}\fi\message{{AFTER}}\end");
    let mut engine = Engine::new(source.as_bytes());
    engine.advance(1).unwrap();
    while engine.outcome().diagnostics.is_empty() {
        engine.advance(1).unwrap();
    }
    assert_eq!(engine.document.conditions.len(), 1);
    let saved = engine.checkpoint().unwrap();
    for _ in 0..2 {
        engine.run_input().unwrap();
        assert!(engine.document.conditions.is_empty());
        assert_eq!(
            engine.outcome().diagnostics,
            [
                write(DiagnosticDestination::LogOnly, b"YES"),
                Diagnostic::Message(b"AFTER".to_vec())
            ]
        );
        engine.restore(&saved).unwrap();
    }
}

#[test]
fn malformed_write_sources_cannot_read_caller_input_and_restore_scratch() {
    for (body, error) in [
        (r"\undefined", Error::Undefined),
        (r"\iffalse", Error::Outer),
        (r"\ifx", Error::Syntax),
        (r"\string", Error::Outer),
        (r"\expandafter\string\ifdefined", Error::Syntax),
        (r"\expandafter\meaning\ifdefined", Error::Syntax),
        (r"\expandafter\noexpand\ifdefined", Error::Syntax),
        (r"\the\prevdepth", Error::Syntax),
        (
            r"\number\numexpr2147483647+1+\undefined\relax",
            Error::Undefined,
        ),
    ] {
        let source =
            format!(r"{BOOT}\message{{BEFORE}}\immediate\write-1{{{body}}}\count0=7\relax\end");
        let mut engine = Engine::new(source.as_bytes());
        while engine.outcome().diagnostics.is_empty() {
            engine.advance(1).unwrap();
        }
        let saved = engine.checkpoint().unwrap();
        for _ in 0..2 {
            assert_eq!(engine.run_input(), Err(error), "{body}");
            assert_eq!(engine.scanner, Scanner::Normal);
            assert_eq!(engine.document.text.mode, Mode::Vertical);
            assert_eq!(engine.expansion_depth, 0);
            assert!(!engine.can_checkpoint());
            assert!(
                !engine
                    .document
                    .sources
                    .iter()
                    .any(|source| matches!(source, Source::WriteBoundary { .. }))
            );
            assert_eq!(
                engine.outcome().diagnostics,
                [Diagnostic::Message(b"BEFORE".to_vec())]
            );
            assert_eq!(
                engine.next_token().unwrap().meaning.command,
                cmd::COUNT,
                "{body}"
            );
            engine.restore(&saved).unwrap();
            assert_eq!(engine.values.retained().0, 0);
        }
    }
}

#[test]
fn write_mode_rejects_space_factor_and_restores_surrounding_horizontal_mode() {
    let mut engine = Engine::new(br"-1{\the\spacefactor}");
    engine.document.scalars[crate::scalars::Target::catcode(b'{' as usize)].value = 1;
    engine.document.scalars[crate::scalars::Target::catcode(b'}' as usize)].value = 2;
    engine.document.text.mode = Mode::Horizontal;
    engine.document.text.space_factor = 4321;
    assert_eq!(engine.immediate_write(), Err(Error::Syntax));
    assert_eq!(engine.document.text.mode, Mode::Horizontal);
    assert_eq!(engine.document.text.space_factor, 4321);
    assert!(engine.document.sources.is_empty());
    assert!(engine.document.diagnostics.copy().is_empty());
}

#[test]
fn raw_write_capture_checks_outer_before_expansion_exceptions() {
    for body in [r"\string\out", r"\noexpand\out", r"\ifdefined\out YES\fi"] {
        let source = format!(r"{BOOT}\outer\def\out{{}}\immediate\write-1{{{body}}}\end");
        let mut engine = Engine::new(source.as_bytes());
        assert_eq!(engine.run_input(), Err(Error::Outer), "{body}");
        assert_eq!(engine.scanner, Scanner::Normal);
        assert!(engine.outcome().diagnostics.is_empty());
    }
    for body in [
        r"\write-1{NO}",
        r"\immediate\write18{NO}",
        r"\immediate\openout0=file",
        r"\immediate\closeout0",
    ] {
        let source = format!(r"{BOOT}{body}\end");
        let mut engine = Engine::new(source.as_bytes());
        assert_eq!(engine.run_input(), Err(Error::Unsupported), "{body}");
        assert!(engine.outcome().diagnostics.is_empty());
    }
}

#[test]
fn private_stopper_is_distinct_safe_and_supports_suppression() {
    let mut engine = Engine::new(br"\endwrite");
    let user = engine.next_token().unwrap().token;
    assert_ne!(user, FROZEN_END_WRITE);
    assert_eq!(engine.names.spelling(FROZEN_END_WRITE.id()), b"endwrite");
    assert_eq!(engine.resolve(FROZEN_END_WRITE).command, cmd::END_WRITE);
    assert!(!engine.resolve(FROZEN_END_WRITE).is_macro());
    assert!(engine.resolve(FROZEN_END_WRITE).expandable());
    engine.push_inline([FROZEN_END_WRITE, Token(0)], 1, true);
    assert_eq!(
        engine
            .with_scanner(Scanner::Absorbing, |engine| engine.next_token())
            .unwrap()
            .meaning,
        Meaning::NOEXPAND_RELAX
    );
}

#[test]
fn multi_window_write_ends_temporary_sources_and_keeps_only_owned_output() {
    let text = "abcdefgh".repeat(700);
    let source = format!(r"{BOOT}\immediate\write-1{{{text}}}\count0=9\relax\end");
    let mut engine = Engine::new(source.as_bytes());
    let initial = engine.checkpoint().unwrap();
    let mut slots = None;
    for _ in 0..3 {
        engine.run_input().unwrap();
        assert_eq!(
            engine.outcome().diagnostics,
            [write(DiagnosticDestination::LogOnly, text.as_bytes())]
        );
        assert_eq!(engine.outcome().counts[0], 9);
        assert_eq!(engine.values.retained().0, 0);
        let high_water = engine.values.retained().1;
        if let Some(previous) = slots {
            assert_eq!(high_water, previous);
        }
        slots = Some(high_water);
        engine.restore(&initial).unwrap();
    }
}

#[test]
fn immediate_write_preserves_channel_and_chronology() {
    let source = br"\catcode123=1\catcode125=2\relax\message{A}\immediate\write-1{B}\immediate\write16{C}\end";
    let outcome = Engine::new(source).run().unwrap();
    assert_eq!(
        outcome.diagnostics,
        [
            Diagnostic::Message(b"A".to_vec()),
            Diagnostic::Write {
                destination: DiagnosticDestination::LogOnly,
                bytes: b"B".to_vec()
            },
            Diagnostic::Write {
                destination: DiagnosticDestination::TerminalAndLog,
                bytes: b"C".to_vec()
            },
        ]
    );
}
