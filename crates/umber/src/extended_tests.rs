use crate::{Engine, Error};

// Expected messages were captured from pdfTeX 1.40.25 INITEX, rather than from
// this interpreter. scripts/oracle.py also runs these exact sources in both.
#[test]
fn extended_upstream_fixtures() {
    let fixtures: &[(&[u8], &str)] = &[
        (
            include_bytes!("../../../fixtures/mvp/token-registers.tex"),
            include_str!("../../../fixtures/mvp/token-registers.expected"),
        ),
        (
            include_bytes!("../../../fixtures/mvp/extended-conditionals.tex"),
            include_str!("../../../fixtures/mvp/extended-conditionals.expected"),
        ),
        (
            include_bytes!("../../../fixtures/mvp/deferred-tokens.tex"),
            include_str!("../../../fixtures/mvp/deferred-tokens.expected"),
        ),
        (
            include_bytes!("../../../fixtures/mvp/conversions.tex"),
            include_str!("../../../fixtures/mvp/conversions.expected"),
        ),
        (
            include_bytes!("../../../fixtures/mvp/expressions.tex"),
            include_str!("../../../fixtures/mvp/expressions.expected"),
        ),
    ];
    for &(source, expected) in fixtures {
        let result = Engine::new(source).run().unwrap();
        let expected: Vec<_> = expected.lines().map(|line| line.as_bytes()).collect();
        assert_eq!(result.messages, expected);
    }
}

#[test]
fn register_sources_survive_overwrite_and_arena_growth() {
    let old = "A".repeat(8197);
    let new = "B".repeat(10003);
    let source = format!(
        r"\catcode123=1 \catcode125=2 \catcode35=6\relax
\def\replace{{\toks0={{{new}}}}}
\toks0={{\replace\message{{{old}}}}}\toks1=\toks0
\def\identity#1{{#1}}\def\copy{{\identity{{\the\toks0}}}}
\copy\message{{\the\toks0}}\toks0=\toks1\relax\the\toks0
\end"
    );
    let result = Engine::new(source.as_bytes()).run().unwrap();
    assert_eq!(
        result.messages,
        [old.as_bytes(), new.as_bytes(), old.as_bytes()]
    );
}

#[test]
fn calendar_is_host_initialized_and_assignable() {
    let mut engine = Engine::new(
        br"\catcode123=1 \catcode125=2\relax
\message{\the\time/\the\day/\the\month/\the\year}
{\day=31\relax}\message{\the\day}\end",
    );
    engine.set_calendar(754, 7, 10, 2026);
    assert_eq!(
        engine.run().unwrap().messages,
        [b"754/7/10/2026".as_slice(), b"7"]
    );
}

#[test]
fn generated_buffers_survive_nested_reads_and_reuse() {
    let text = "X".repeat(10003);
    let source = format!(
        r"\catcode123=1 \catcode125=2 \catcode35=6\relax
\def\word{{{text}}}\edef\saved{{\meaning\word}}\def\word{{Y}}
\message{{\saved}}\message{{\detokenize{{\word # {{{text}}}}}}}
\edef\saved{{\string\word\romannumeral1994\meaning\word}}
\message{{\saved}}\end"
    );
    let result = Engine::new(source.as_bytes()).run().unwrap();
    assert_eq!(
        result.messages,
        [
            format!("macro:->{text}"),
            format!(r"\word ## {{{text}}}"),
            r"\wordmcmxcivmacro:->Y".into()
        ]
        .map(String::into_bytes)
    );
}

#[test]
fn extended_failure_boundaries() {
    let cases: &[(&[u8], Error)] = &[
        (
            include_bytes!("../../../fixtures/mvp/failures/expression-late-undefined.tex"),
            Error::Undefined,
        ),
        (
            include_bytes!("../../../fixtures/mvp/failures/expression-overflow-undefined.tex"),
            Error::Undefined,
        ),
        (
            include_bytes!("../../../fixtures/mvp/failures/expression-range-undefined.tex"),
            Error::Undefined,
        ),
        (
            include_bytes!("../../../fixtures/mvp/failures/expression-overflow.tex"),
            Error::Overflow,
        ),
        (
            include_bytes!("../../../fixtures/mvp/failures/expression-divide-zero.tex"),
            Error::Overflow,
        ),
        (
            include_bytes!("../../../fixtures/mvp/failures/expression-parenthesis.tex"),
            Error::Syntax,
        ),
        (
            include_bytes!("../../../fixtures/mvp/failures/expression-unary-parenthesis.tex"),
            Error::Number,
        ),
        (
            include_bytes!("../../../fixtures/mvp/failures/unless-case.tex"),
            Error::Conditional,
        ),
        (
            include_bytes!("../../../fixtures/mvp/failures/unless-macro.tex"),
            Error::Conditional,
        ),
        (
            include_bytes!("../../../fixtures/mvp/failures/boolean-or.tex"),
            Error::Conditional,
        ),
        (
            include_bytes!("../../../fixtures/mvp/failures/token-register-value.tex"),
            Error::Syntax,
        ),
        (
            include_bytes!("../../../fixtures/mvp/failures/character-range.tex"),
            Error::Number,
        ),
        (
            include_bytes!("../../../fixtures/mvp/failures/case-range.tex"),
            Error::Number,
        ),
    ];
    for &(source, expected) in cases {
        assert_eq!(Engine::new(source).run().err(), Some(expected));
    }
}
