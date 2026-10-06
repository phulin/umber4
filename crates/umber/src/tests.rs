use crate::{Engine, Error};

const BOOT: &str = "\\catcode123=1 \\catcode125=2 \\catcode35=6 \\catcode94=7\\relax\n";
fn run(program: &str) -> crate::Outcome {
    let source = format!("{BOOT}{program}\\end\n");
    Engine::new(source.as_bytes())
        .run()
        .unwrap_or_else(|e| panic!("{e}: {program}"))
}
fn message(program: &str, expected: &[u8]) {
    let result = run(program);
    assert_eq!(result.messages, vec![expected.to_vec()], "{program}");
}
#[test]
fn meaning_and_body_lifetimes() {
    message(
        r"\def\a{old}\let\b=\a\def\a{new}\message{\b/\a}",
        b"old/new",
    );
    message(
        r"\def\a{\def\a{new}\count0=5\relax}\a\message{\a/\the\count0}",
        b"new/5",
    );
}
#[test]
fn arguments_and_delimiters() {
    message(r"\def\id#1{#1}\def\pass#1{\id#1}\message{\pass{X}}", b"X");
    message(r"\def\twice#1{#1/#1}\message{\twice{ab}}", b"ab/ab");
    message(r"\def\grab#1aba{[#1]}\message{\grab aaba}", b"[a]");
    message(r"\def\grab#1x{[#1]}\message{\grab{a{x}b}x}", b"[a{x}b]");
    message(r"\def\a#{X}\message{\a{Y}}", b"X{Y}");
}
#[test]
fn grouping_and_arithmetic() {
    let result =
        run(r"\count0=1{\count0=2\global\count0=3\count0=4}\count1=-7\divide\count1 BY2\relax");
    assert_eq!(result.counts[0], 3);
    assert_eq!(result.counts[1], -3);
    message(r"\def\a{A}{\def\a{B}\gdef\a{C}\def\a{D}}\message{\a}", b"C");
    message(
        r#"\countdef\a=4\a='17\advance\a BY"A\relax\message{\the\a}"#,
        b"25",
    );
}
#[test]
fn expansion_order_and_suppression() {
    message(r"\def\a{A}\edef\b{\noexpand\a}\def\a{B}\message{\b}", b"B");
    let result = run(r"\noexpand\count0=2\relax");
    assert_eq!(result.counts[0], 2);
    message(
        r"\expandafter\def\csname made\endcsname{YES}\message{\made}",
        b"YES",
    );
    message(r"\def\a{A}\def\b{B}\message{\expandafter\a\b}", b"AB");
}
#[test]
fn conditional_semantics() {
    message(
        r"\def\a#1{#1}\def\b#1{#1}\message{\ifx\a\b Y\else N\fi}",
        b"Y",
    );
    message(r"\message{\ifx\notknown\otherunknown Y\else N\fi}", b"Y");
    message(r"\message{\ifnum1=1\else N\fi Y}", br"\relax Y");
    message(
        r"\let\test=\iftrue\message{\iffalse BAD\test INNER\else BAD\fi\else OK\fi}",
        b"OK",
    );
}
#[test]
fn lazy_character_input() {
    message(r"\catcode64=11\def\a@b{OK}\message{\a@b}", b"OK");
    message(r"\def\a{\catcode94=12\relax}\a\message{^^2b}", b"^^2b");
    message(r"\def\a{A}\message{\^^61}", b"A");
    message("\\endlinechar=33\\relax\n\\message{X\nY}", b"X!Y");
}
#[test]
fn failure_and_tail_calls() {
    let source = format!("{BOOT}{{\\endgroup\\end");
    assert_eq!(
        Engine::new(source.as_bytes()).run().err(),
        Some(Error::Group)
    );
    let result =
        run(r"\def\again{\advance\count0by1\ifnum\count0<10000 \expandafter\again\fi}\again");
    assert_eq!(result.counts[0], 10001);
    let result =
        run(r"\def\again{\advance\count0by1\relax\ifnum\count0<10000 \expandafter\again\fi}\again");
    assert_eq!(result.counts[0], 10000);
}

#[test]
fn scanner_edges() {
    message(
        r"\message{\csname\endcsname/#/^^01/^^80}",
        br"\csname\endcsname /##/^^A/^^80",
    );
    message(
        r"\advance\endlinechar by1\relax\message{\the\endlinechar}",
        b"14",
    );
    message(r"\let\B={\def\a\B{X}\message{\a\B}", b"X");
    message(r"\def\n{1}\edef\a#1{#\n}\message{\a{X}}", b"X");
    message(
        r"\def\a{A}\message{\expandafter\ifx\noexpand\a\relax Y\else N\fi}",
        b"N",
    );
    message(r"\global\relax\count0=7\relax\message{\the\count0}", b"7");
    message(
        r"\def\make#1#2{\let\s=#1#2}\make{ }{ }\def\s{X}\message{\s}",
        b"X",
    );
    message(
        r"\def\a#1\par!{[#1]}\message{\a X\par Y\par!}",
        br"[X\par Y]",
    );
}

#[test]
fn argument_backing_survives_tail_retirement_and_repetition() {
    let nested = format!("{}X{}", r"\id{".repeat(5000), "}".repeat(5000));
    message(&format!(r"\def\id#1{{#1}}\message{{{nested}}}"), b"X");
    message(
        r"\def\id#1{#1}\def\twice#1{\id{#1}\id{#1}}\message{\twice{AB}\twice{}\twice{CD}}",
        b"ABABCDCD",
    );
    message(
        r"\def\id#1{#1}\def\pass#1{\id{A#1B}}\message{\pass{X}\pass{Y}}",
        b"AXBAYB",
    );
}

#[test]
fn active_definition_cursors_survive_chunk_and_header_growth() {
    let replacement = "R".repeat(10000);
    let program = format!(
        r"\def\a{{\def\a{{{replacement}}}\count0=9\relax}}\a\message{{\the\count0}}\message{{\a}}"
    );
    let result = run(&program);
    assert_eq!(
        result.messages,
        vec![b"9".to_vec(), replacement.into_bytes()]
    );
    let huge = format!(r"\def\a{{{}\count0=7\relax}}\a", r"\relax".repeat(10000));
    assert_eq!(run(&huge).counts[0], 7);
}
