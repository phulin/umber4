use super::*;
use crate::{token::cmd, values::Class};

const BOOT: &str = r"\catcode123=1\catcode125=2\catcode35=6\catcode38=4 ";
fn setup(source: &[u8]) -> Engine<'_> {
    let mut e = Engine::new(source);
    loop {
        let read = e.next_expanded().unwrap();
        if read.meaning.command == cmd::RELAX {
            break;
        }
        e.execute(read).unwrap();
    }
    e
}
fn templates(e: &mut Engine<'_>, u: &[Token], v: &[Token]) -> (Span, Span) {
    let mut builder = e.values.builder(Class::Durable).unwrap();
    for &t in u {
        builder.push(t).unwrap();
    }
    let split = builder.len();
    for &t in v {
        builder.push(t).unwrap();
    }
    builder.push(FROZEN_END_TEMPLATE).unwrap();
    let owner = builder.finish();
    let u = Span {
        store: owner.id,
        start: 0,
        end: split,
    };
    let v = Span {
        store: owner.id,
        start: split,
        end: owner.tokens.len,
    };
    e.install_alignment_templates(owner).unwrap();
    (u, v)
}
fn start(e: &mut Engine<'_>, u: &[Token], v: &[Token]) {
    e.begin_alignment_reader().unwrap();
    let ranges = templates(e, u, v);
    let first = e.peek_alignment_entry().unwrap();
    e.begin_alignment_entry(first, Some(ranges)).unwrap();
}
fn endv(e: &mut Engine<'_>, value: i32) {
    assert_eq!(e.next_expanded().unwrap().token, FROZEN_ENDV);
    assert_eq!(e.alignment_endv(), Ok(Terminator(value)));
}

#[test]
fn borrowed_templates_intercept_all_source_variants_and_retire_v_only_on_raw_overread() {
    for variant in 0..4 {
        let source = format!(r"{BOOT}\def\pass#1{{#1}}\relax a&b");
        let mut e = setup(source.as_bytes());
        let input = [
            Token::other(b'a'),
            Token::character(4, b'&'),
            Token::other(b'b'),
        ];
        match variant {
            1 => e.push_inline([input[0], input[1]], 2, false),
            2 => {
                let mut tokens = e.values.builder(Class::Durable).unwrap();
                for t in input {
                    tokens.push(t).unwrap();
                }
                e.push_tokens(tokens.finish(), 0).unwrap();
            }
            3 => {
                let pass = Token::cs(e.names.lookup(b"pass").unwrap());
                let mut tokens = e.values.builder(Class::Durable).unwrap();
                for t in [
                    pass,
                    Token::character(1, b'{'),
                    input[0],
                    Token::character(2, b'}'),
                    input[1],
                    input[2],
                ] {
                    tokens.push(t).unwrap();
                }
                e.push_tokens(tokens.finish(), 0).unwrap();
            }
            _ => {}
        }
        start(&mut e, &[Token::other(b'u')], &[Token::other(b'v')]);
        assert_eq!(e.next_raw().unwrap().token, Token::other(b'u'));
        assert!(e.alignment_reader.state > 500000);
        assert_eq!(e.next_raw().unwrap().token.byte(), b'a');
        assert_eq!(e.alignment_reader.state, 0);
        assert_eq!(e.next_raw().unwrap().token.byte(), b'v');
        endv(&mut e, 38);
        e.retire_exhausted().unwrap();
        assert!(matches!(
            e.document.sources.last().unwrap().role(),
            SourceRole::AlignmentV(1)
        ));
        e.back(FROZEN_ENDV).unwrap();
        assert_eq!(
            e.alignment_endv(),
            Err(Error::Syntax),
            "pending wrapper must remain"
        );
        assert_eq!(e.next_expanded().unwrap().token, FROZEN_ENDV);
        assert_eq!(e.alignment_endv(), Ok(Terminator(38)));
        e.retire_exhausted().unwrap();
        let _ = e.next_raw().unwrap();
        e.end_alignment_reader().unwrap();
        assert!(!e.alignment_reader.active());
    }
}

#[test]
fn empty_u_is_installed_and_u_failure_precedes_backing_compensation() {
    let source = format!(r"{BOOT}\relax a&b");
    let mut e = setup(source.as_bytes());
    start(&mut e, &[], &[]);
    assert!(matches!(
        e.document.sources.last().unwrap().role(),
        SourceRole::AlignmentU(1)
    ));
    assert!(e.current_source_exhausted());
    e.alignment_reader.state = 500000;
    let count = e.document.sources.len();
    assert_eq!(e.back(Token::character(1, b'{')), Err(Error::Syntax));
    assert_eq!(e.alignment_reader.state, 500000);
    assert_eq!(e.document.sources.len(), count);
    e.alignment_reader.state = 500001;
    e.back(Token::character(1, b'{')).unwrap();
    assert_eq!(e.alignment_reader.state, -1, "U resets before undo");
    assert!(e.next_raw().unwrap().token.is_begin());
    assert_eq!(e.alignment_reader.state, 0);
}

#[test]
fn lexical_braces_and_effective_terminator_aliases_follow_different_rules() {
    let source = format!(
        r"{BOOT}\let\open={{\let\close=}}\let\amp=&\let\row\cr\relax {{&}}\open\close\noexpand\amp b"
    );
    let mut e = setup(source.as_bytes());
    start(&mut e, &[], &[Token::other(b'v')]);
    let open = e.next_raw().unwrap();
    assert!(open.token.is_begin());
    assert_eq!(e.alignment_reader.state, 1);
    assert_eq!(e.next_raw().unwrap().token.cat(), 4);
    assert_eq!(e.alignment_reader.state, 1);
    assert!(e.next_raw().unwrap().token.is_end());
    assert_eq!(e.alignment_reader.state, 0);
    assert_eq!(e.next_token().unwrap().meaning.command, cmd::BEGIN);
    assert_eq!(e.alignment_reader.state, 0);
    assert_eq!(e.next_token().unwrap().meaning.command, cmd::END_GROUP_CHAR);
    assert_eq!(e.alignment_reader.state, 0);
    assert_eq!(e.next_expanded().unwrap().token.byte(), b'v');
    endv(&mut e, 38); // noexpand still caused V insertion for the amp alias.
}

#[test]
fn frozen_markers_are_outer_before_expansion_and_provenance_is_not_a_stack_truncation() {
    for scanner in [
        Scanner::Matching,
        Scanner::Defining,
        Scanner::Absorbing,
        Scanner::Skipping,
        Scanner::Aligning,
    ] {
        let source = format!(r"{BOOT}\relax a&b");
        let mut e = setup(source.as_bytes());
        start(&mut e, &[], &[Token::other(b'v')]);
        e.next_raw().unwrap();
        e.next_raw().unwrap();
        assert_eq!(
            e.with_scanner(scanner, |e| e.next_token()).map(|r| r.token),
            Err(Error::Outer)
        );
        assert_eq!(e.scanner, Scanner::Normal);
    }
    let source = format!(r"{BOOT}\relax a&b");
    let mut e = setup(source.as_bytes());
    start(&mut e, &[], &[]);
    e.next_raw().unwrap();
    endv(&mut e, 38);
    e.push_inline([Token::other(b'x'), FROZEN_ENDV], 2, false);
    assert_eq!(e.alignment_endv(), Err(Error::Syntax));
    assert_eq!(e.next_raw().unwrap().token, Token::other(b'x'));
    assert_eq!(e.next_raw().unwrap().token, FROZEN_ENDV);
    assert_eq!(e.alignment_endv(), Ok(Terminator(38)));
    e.push_inline([FROZEN_ENDV, Token::other(b'y')], 2, false);
    e.next_raw().unwrap();
    assert_eq!(e.alignment_endv(), Err(Error::Syntax));
    assert_eq!(e.next_raw().unwrap().token, Token::other(b'y'));
}

#[test]
fn numeric_scanning_crosses_u_and_v_before_the_assignment_executes() {
    let source = format!(r"{BOOT}\relax 34&b");
    let mut e = setup(source.as_bytes());
    let count = Token::cs(e.names.lookup(b"count").unwrap());
    start(
        &mut e,
        &[
            count,
            Token::other(b'0'),
            Token::other(b'='),
            Token::other(b'1'),
            Token::other(b'2'),
        ],
        &[Token::other(b'5'), FROZEN_RELAX],
    );
    let read = e.next_expanded().unwrap();
    e.execute(read).unwrap();
    assert_eq!(e.outcome().counts[0], 12345);
    assert_eq!(e.next_expanded().unwrap().meaning.command, cmd::RELAX);
    endv(&mut e, 38);
}

#[test]
fn protected_peek_stops_then_ordinary_execution_expands_and_special_brace_reads_compensate() {
    let source = format!(r"{BOOT}\protected\def\probe{{a}}\def\grab#1{{#1}}\relax\probe&b");
    let mut e = setup(source.as_bytes());
    e.begin_alignment_reader().unwrap();
    let spans = templates(&mut e, &[], &[]);
    let first = e.peek_alignment_entry().unwrap();
    assert!(first.meaning.is_protected_macro());
    e.begin_alignment_entry(first, Some(spans)).unwrap();
    assert_eq!(e.next_expanded().unwrap().token.byte(), b'a');
    endv(&mut e, 38);
    let source = format!(r"{BOOT}\def\delimited#1#{{a}}\relax\count0=`{{\delimited z{{x}}&b");
    let mut e = setup(source.as_bytes());
    start(&mut e, &[], &[]);
    let read = e.next_expanded().unwrap();
    e.execute(read).unwrap();
    assert_eq!(e.outcome().counts[0], 123);
    assert_eq!(e.alignment_reader.state, 0);
    assert_eq!(e.next_expanded().unwrap().token.byte(), b'a');
    assert!(e.next_raw().unwrap().token.is_begin());
    assert_eq!(e.alignment_reader.state, 1);
}

#[test]
fn write_expansion_isolates_counter_and_does_not_intercept_its_tabs() {
    let source = format!(r"{BOOT}\def\amp{{&}}\relax\immediate\write16{{\amp}}a&b");
    let mut e = setup(source.as_bytes());
    start(&mut e, &[], &[]);
    let read = e.next_expanded().unwrap();
    e.execute(read).unwrap();
    assert_eq!(e.alignment_reader.state, 0);
    assert_eq!(e.outcome().diagnostics.len(), 1);
    assert_eq!(e.next_raw().unwrap().token.byte(), b'a');
    endv(&mut e, 38);
}

#[test]
fn escaped_capture_outlives_borrowed_template_and_restore_clears_old_sources_first() {
    let source = format!(r"{BOOT}\relax a&b c&d");
    let mut e = setup(source.as_bytes());
    let checkpoint = e.checkpoint().unwrap();
    start(&mut e, &[Token::other(b'u')], &[]);
    let raw = e.next_raw().unwrap();
    let store = raw.origin.unwrap().store;
    let packet = e.take_packet(Arity::One).unwrap();
    let mut builder = None;
    e.capture_raw(packet, 0, raw, &mut builder).unwrap();
    e.finish_capture(packet, builder);
    assert_eq!(e.packets[packet as usize].owners.len(), 1);
    e.next_raw().unwrap();
    endv(&mut e, 38);
    e.next_raw().unwrap();
    e.end_alignment_reader().unwrap();
    assert_eq!(e.values.token(store, 0).byte(), b'u');
    e.push_argument(packet, 0).unwrap();
    assert_eq!(e.next_raw().unwrap().token.byte(), b'u');
    e.retire_exhausted().unwrap();
    e.release_packet(packet);
    start(&mut e, &[], &[]);
    assert!(!e.can_checkpoint());
    let foreign = Engine::new(b"").checkpoint().unwrap();
    assert_eq!(e.restore(&foreign), Err(Error::Checkpoint));
    assert!(e.alignment_reader.active());
    e.restore(&checkpoint).unwrap();
    assert!(!e.alignment_reader.active());
    assert!(e.document.sources.is_empty());
}

#[path = "alignment_reader_oracle_tests.rs"]
mod oracle;

#[test]
fn fixed_brace_prefix_and_nested_reader_owners_preserve_outer_counter() {
    let source = format!(r"{BOOT}\def\fixed#{{X}}\relax a\fixed{{Y}}&b");
    let mut e = setup(source.as_bytes());
    start(&mut e, &[], &[]);
    assert_eq!(e.next_raw().unwrap().token.byte(), b'a');
    assert_eq!(e.next_expanded().unwrap().token.byte(), b'X');
    assert!(e.next_raw().unwrap().token.is_begin());
    assert_eq!(e.alignment_reader.state, 1);
    assert_eq!(e.next_raw().unwrap().token.byte(), b'Y');
    assert!(e.next_raw().unwrap().token.is_end());
    assert_eq!(e.alignment_reader.state, 0);
    endv(&mut e, 38);

    let source = format!(r"{BOOT}\relax a&b");
    let mut e = setup(source.as_bytes());
    let checkpoint = e.checkpoint().unwrap();
    let mut capacity = None;
    for _ in 0..3 {
        start(&mut e, &[], &[Token::other(b'v')]);
        e.next_raw().unwrap();
        e.push_inline([Token::other(b'i'), Token::character(4, b'&')], 2, false);
        start(&mut e, &[], &[]);
        assert_eq!(e.next_raw().unwrap().token.byte(), b'i');
        endv(&mut e, 38);
        e.advance_source().unwrap(); // ordinary raw retirement of the exhausted V.
        e.end_alignment_reader().unwrap();
        assert_eq!(e.alignment_reader.state, 0);
        assert_eq!(e.next_raw().unwrap().token.byte(), b'v');
        endv(&mut e, 38);
        let current = e.alignment_reader.frames.capacity();
        if let Some(capacity) = capacity {
            assert_eq!(current, capacity);
        } else {
            capacity = Some(current);
        }
        e.restore(&checkpoint).unwrap();
        assert!(!e.alignment_reader.active());
        assert_eq!(e.alignment_reader.frames.capacity(), current);
    }
}

#[test]
fn preamble_interception_rejects_before_installing_v_and_frozen_meanings_are_distinct() {
    let source = format!(r"{BOOT}\relax a&b");
    let mut e = setup(source.as_bytes());
    start(&mut e, &[], &[]);
    e.next_raw().unwrap();
    let count = e.document.sources.len();
    assert_eq!(
        e.with_scanner(Scanner::Aligning, |e| e.next_raw()).err(),
        Some(Error::Syntax)
    );
    assert_eq!(e.document.sources.len(), count - 1); // exhausted backed first-token source retired.
    for (token, expected) in [
        (FROZEN_END_TEMPLATE, b"\\outer endtemplate".as_slice()),
        (FROZEN_ENDV, b"end of alignment template"),
    ] {
        let mut text = Vec::new();
        e.push_inline([token, Token(0)], 1, false);
        let command = Token::cs(e.names.lookup(b"meaning").unwrap());
        let read = Read {
            token: command,
            meaning: e.resolve(command),
        };
        e.conversion::<false>(read, |_, byte| {
            text.push(byte);
            Ok(())
        })
        .unwrap();
        assert_eq!(text, expected);
    }
}
