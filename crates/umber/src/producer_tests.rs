use super::*;
const BOOT: &str = "\\catcode123=1 \\catcode125=2 \\catcode35=6 \\catcode94=7\\relax\n";
fn compare(source: &str, expected: &[&[u8]]) -> Stats {
    let source = format!("{BOOT}{source}");
    let original = Engine::new(source.as_bytes()).run().unwrap();
    let (produced, stats) = Engine::new(source.as_bytes())
        .run_producer_diagnostic()
        .unwrap();
    assert_eq!(produced.counts, original.counts);
    assert_eq!(produced.messages, original.messages);
    assert_eq!(produced.messages, expected);
    stats
}
#[test]
fn scanner_completion_changes_consumer_inside_live_body() {
    compare(
        r"\def\value{123\relax\message{\the\count0}}\count0=\value\end",
        &[b"123"],
    );
    compare(
        r"\def\value{123\relax\message{later}\count1=7}\count0=\value\message{\the\count0/\the\count1}\end",
        &[b"later", b"123/7"],
    );
}
#[test]
fn body_exhaustion_keeps_the_consumer_for_parent_input() {
    compare(
        r"\def\set{\count0=}\set 123\message{\the\count0}\end",
        &[b"123"],
    );
    compare(
        r"\def\digits{12}\count0=\digits 34\relax\message{\the\count0}\end",
        &[b"1234"],
    );
    compare(
        r"\let\openbrace={\def\open{\message\openbrace hello }\open world}\end",
        &[b"hello world"],
    );
}
#[test]
fn replacement_and_group_restoration_retain_live_body() {
    compare(
        r"\def\old{\def\old{NEW}\message{OLD}}\old\message{\old}{\count0=9\def\old{LOCAL}\global\count1=7}\message{\old/\the\count0/\the\count1}\end",
        &[b"OLD", b"NEW", b"NEW/0/7"],
    );
}
#[test]
fn self_tail_alias_retains_body_across_redefinition() {
    let stats = compare(
        r"\def\step{\advance\count0by1\relax\ifnum\count0<3 \let\next\step\def\step{\message{REPLACED}}\else\let\next\relax\fi\next}\step\message{\the\count0}\end",
        &[b"REPLACED", b"2"],
    );
    assert_eq!(stats.max_producer_depth, 1);
    assert_eq!(stats.tail_calls, 2);
}
#[test]
fn nested_integers_and_split_keyword_resume_their_consumers() {
    compare(
        r"\count0=1\relax\count1=2\relax\count2=7\relax\count3=-\count\count\count0\relax\count4=\number\number123\relax\let\A\advance\let\C\count\def\by{b}\def\why{y}\A\C0\by\why\C2\relax\iffalse\iftrue\count0=999\else\count0=998\fi\else\ifnum\count3<0\relax\else\count0=997\fi\fi\message{\the\count0/\the\count3/\the\count4}\end",
        &[b"8/-7/123"],
    );
}
#[test]
fn scalar_windows_share_signed_internal_values_and_group_restoration() {
    compare(
        r"\count2=7\relax\let\C\count\let\A\advance\def\work{\C0=-\C2\relax\A\C0by-\C2\relax\multiply\C0by-2\relax\divide\C0by7\relax}\work\message{\the\count0}{\global\work}\message{\the\count0}\end",
        &[b"4", b"-7"],
    );
    compare(
        r"\def\setup{\catcode64=11\relax\endlinechar=-1\relax}\setup\def\foo@bar{OK}\message{\foo@bar}\end",
        &[b"OK"],
    );
}
#[test]
fn file_index_completion_preserves_expansion_and_character_scanning() {
    compare(
        r"\def\more{2}\count1\more=7\relax\count^^31=8\relax\count-0=9\relax\message{\the\count12/\the\count1/\the\count0}\end",
        &[b"7/8/9"],
    );
}
#[test]
fn internal_completion_reads_suspended_parents_in_order() {
    compare(
        r"\count0=1\relax\count1=2\relax\count2=7\relax\ifnum\count\count0<\count\count\count0 \message{LESS}\else\message{WRONG}\fi\message{\the\count\count0/\number-\count\count\count0/\number\number\count\count0}\end",
        &[b"LESS", b"2/-7/2"],
    );
}
#[test]
fn scalar_windows_consume_only_inert_operand_terminators() {
    compare(
        r"\let\R\relax\def\work{\count0=12\R\advance\count0by3\R\global\count1=\count0\R}\work\message{\the\count0/\the\count1}\end",
        &[b"15/15"],
    );
    let source = format!(r"{BOOT}\def\work{{\count0\relax=7\relax}}\work\end");
    assert_eq!(
        Engine::new(source.as_bytes()).run().err(),
        Some(Error::Number)
    );
    assert_eq!(
        Engine::new(source.as_bytes()).run_producer().err(),
        Some(Error::Number)
    );
}
#[test]
fn scalar_windows_defer_expansions_and_incomplete_windows() {
    for (source, expected) in [
        (
            r"\def\more{34}\def\work{\count0=12\more\relax}\work\message{\the\count0}\end",
            b"1234".as_slice(),
        ),
        (
            r"\def\index{1}\def\by{by}\def\value{7}\def\work{\advance\count\index\by\value\relax}\work\message{\the\count1}\end",
            b"7".as_slice(),
        ),
        (
            r"\def\work{\count0=12}\work34\relax\message{\the\count0}\end",
            b"1234".as_slice(),
        ),
    ] {
        compare(source, &[expected]);
    }
    // Straddle the retained page end in the operand, index, and command.
    for padding in [4088, 4093, 4094] {
        let source = format!(
            r"\def\work{{{}\count0=1234567\relax}}\work\message{{\the\count0}}\end",
            r"\relax".repeat(padding)
        );
        compare(&source, &[b"1234567"]);
    }
}
#[test]
fn scalar_window_rejection_preserves_supported_errors() {
    for (body, error) in [
        (r"\count0=2147483648\relax", Error::Overflow),
        (r"\count256=1\relax", Error::Number),
        (r"\catcode64=16\relax", Error::Number),
        (
            r"\count0=2147483647\relax\multiply\count0by2\relax",
            Error::Overflow,
        ),
        (r"\count0=7\relax\divide\count0by0\relax", Error::Overflow),
        (r"\long\count0=1\relax", Error::Syntax),
        (r"\advance\count0by\missing\relax", Error::Undefined),
    ] {
        let source = format!(r"{BOOT}\def\work{{{body}}}\work\end");
        assert_eq!(Engine::new(source.as_bytes()).run().err(), Some(error));
        assert_eq!(
            Engine::new(source.as_bytes()).run_producer().err(),
            Some(error)
        );
    }
}
#[test]
fn integer_consumers_and_nested_conditionals_preserve_order() {
    compare(
        r"\count0=\iftrue12\else99\fi 3\relax\count1=-\count0\relax\advance\count0by7\multiply\count0by2\divide\count0by5\message{\the\count0/\number\count1/\ifnum\count0>0 YES\else NO\fi}\end",
        &[b"52/-123/YES"],
    );
}
#[test]
fn long_tail_loop_uses_one_native_producer_frame() {
    let stats = compare(
        r"\def\step{\advance\count0by1\relax\ifnum\count0<50000 \let\next\step\else\let\next\relax\fi\next}\step\message{\the\count0}\end",
        &[b"50000"],
    );
    assert_eq!(stats.max_producer_depth, 1);
    assert_eq!(stats.macro_calls, 50000);
    assert_eq!(stats.tail_calls, 49999);
    assert!(stats.max_consumer_depth <= 3, "{stats:?}");
}
#[test]
fn windows_cross_chunks_and_native_depth_is_checked() {
    let text = "AbCd ".repeat(1800);
    let source = format!(r"\def\text{{{text}}}\message{{\text}}\end");
    compare(&source, &[text.as_bytes()]);
    let mut source = String::from(BOOT);
    for index in 0..140 {
        // Alphabetic control words, with a trailing token making each call
        // genuinely nested rather than eligible for the tail loop.
        let name = |n: usize| {
            format!(
                "m{}{}",
                (b'a' + (n / 26) as u8) as char,
                (b'a' + (n % 26) as u8) as char
            )
        };
        source.push_str(&format!(
            "\\def\\{}{{\\{}\\relax}}",
            name(index),
            name(index + 1)
        ));
    }
    source.push_str("\\def\\mfk{\\relax}\\maa\\end");
    assert!(matches!(
        Engine::new(source.as_bytes()).run_producer(),
        Err(Error::Capacity)
    ));
}
#[test]
fn unsupported_facilities_are_rejected() {
    for source in [r"\def\f#1{#1}\end", r"\toks0={X}\end", r"\edef\f{X}\end"] {
        let source = format!("{BOOT}{source}");
        assert!(matches!(
            Engine::new(source.as_bytes()).run_producer(),
            Err(Error::Unsupported)
        ));
    }
}
