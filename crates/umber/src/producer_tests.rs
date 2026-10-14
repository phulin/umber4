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
