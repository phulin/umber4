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
    message("\\message{X\tZ/X\t\nY}", br"X^^IZ/X^^I Y");
    message("\\message{X\rY}", b"X Y");
    message("\\catcode9=10\\relax\n\\message{X\t\nY}", b"X Y");
    message("\\catcode32=12\\relax\n\\message{X  \nY}", b"X Y");
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
    message(
        r"\def\twice#1{#1/#1}\message{\twice{X}/\twice{Y}}",
        b"X/X/Y/Y",
    );
    message(
        r"\def\id#1{#1}\def\tick{T}\message{\expandafter\id\noexpand\tick}",
        b"T",
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

#[test]
fn grouped_argument_windows() {
    let payload = format!("{}{{Y}}{}", "X".repeat(4097), "Z".repeat(4097));
    let program = format!(
        r"\def\id#1{{#1}}\def\pass#1{{\id{{A#1B}}}}\def\invoke{{\pass{{{payload}}}}}\message{{\invoke\invoke}}"
    );
    let expected = format!("A{payload}BA{payload}B");
    message(&program, expected.as_bytes());

    let source = format!(
        r"{BOOT}\def\id#1{{#1}}\def\invoke{{\id{{{}\par}}}}\invoke\end",
        "X".repeat(4097)
    );
    assert_eq!(
        Engine::new(source.as_bytes()).run().err(),
        Some(Error::Argument)
    );

    let result = Engine::new(include_bytes!("../../../fixtures/mvp/window-arguments.tex"))
        .run()
        .unwrap();
    let expected: &[&[u8]] = &[
        b"UMBER:A{B}C/L{X}R/L{}R:END",
        b"UMBER:{AB}:END",
        b"UMBER:AXB/AYB/AB:END",
        b"UMBER:LBR/A/LDR/C:END",
        b"UMBER:A{B}C:END",
        b"UMBER:D{E}F:END",
        br"UMBER:P\par Q:END",
        br"UMBER:A\L B\R C:END",
        b"UMBER:A[B]C:END",
    ];
    assert_eq!(result.messages, expected);
}

#[test]
fn single_group_words_cross_source_windows() {
    // Move the opening brace, sole word, and closing brace through a definition
    // chunk boundary. Also forward the word through an inline parameter and
    // finish an argument source whose capture backing can retire immediately.
    for padding in 4084..=4092 {
        let program = format!(
            r"\def\padding{{{}}}\def\id#1{{#1}}\def\stored{{\id{{X}}}}
\def\emit#1{{\id{{#1}}}}\def\forward#1{{#1}}
\message{{\stored/\stored/\emit{{Y}}/\forward{{\id{{Z}}}}}}",
            "P".repeat(padding)
        );
        message(&program, b"X/X/Y/Z");
    }
}

#[test]
fn shared_upstream_interactions() {
    let result = Engine::new(include_bytes!("../../../fixtures/mvp/interactions.tex"))
        .run()
        .unwrap();
    let expected: Vec<_> = include_str!("../../../fixtures/mvp/interactions.expected")
        .lines()
        .map(|line| line.as_bytes().to_vec())
        .collect();
    assert_eq!(result.messages, expected);
}

#[test]
fn inline_argument_values_and_promotion() {
    let result = Engine::new(include_bytes!("../../../fixtures/mvp/inline-arguments.tex"))
        .run()
        .unwrap();
    let expected: &[&[u8]] = &[
        b"UMBER:XX/YY/:END",
        b"UMBER:WW:END",
        b"UMBER:XX/YY:END",
        b"UMBER:iahbgcfde/XX/918273645:END",
        b"UMBER:XABX:END",
        b"UMBER:YAXXBY:END",
        b"UMBER:[AB]/[{C}]:END",
        b"UMBER:[Y]/[Z]/[]:END",
        b"UMBER:FROZEN:END",
        b"UMBER:NEW:END",
        b"UMBER:0/9/-7/123:END",
        b"UMBER:7:END",
        b"UMBER:A:END",
        b"UMBER:+:END",
    ];
    assert_eq!(result.messages, expected);
}

#[test]
fn stored_integer_windows_and_terminators() {
    let result = Engine::new(include_bytes!("../../../fixtures/mvp/numeric-windows.tex"))
        .run()
        .unwrap();
    let expected: &[&[u8]] = &[
        b"UMBER:12345/11259375/2054353:END",
        b"UMBER:15f:END",
        b"UMBER:123X:END",
        b"UMBER:12345Z:END",
        b"UMBER:9:END",
        b"UMBER:Y:END",
        b"UMBER:7:END",
        b"UMBER:123:END",
        b"UMBER:12T:END",
        b"UMBER:1234Z:END",
        b"UMBER:12T:END",
        b"UMBER:159:END",
    ];
    assert_eq!(result.messages, expected);

    let zeros = "0".repeat(8197);
    let source = format!(r"\def\digits{{{zeros}17}}\count0=\digits\relax");
    assert_eq!(run(&source).counts[0], 17);
    let source = format!(r"{BOOT}\def\digits{{{zeros}2147483648}}\count0=\digits\relax\end");
    assert_eq!(
        Engine::new(source.as_bytes()).run().err(),
        Some(Error::Overflow)
    );
}

#[test]
fn discarded_arguments_still_follow_scanning_rules() {
    let result = Engine::new(include_bytes!(
        "../../../fixtures/mvp/discarded-arguments.tex"
    ))
    .run()
    .unwrap();
    let expected: &[&[u8]] = &[
        b"UMBER:OK:END",
        b"UMBER:D/5:END",
        b"UMBER:##:END",
        b"UMBER:L:END",
        b"UMBER:G:END",
        b"UMBER:G:END",
        b"UMBER:B{Y}:END",
        b"UMBER:5/2:END",
        b"UMBER:P:END",
        b"UMBER:E:END",
        b"UMBER:X:END",
    ];
    assert_eq!(result.messages, expected);
}

#[test]
fn supported_failure_boundaries() {
    let cases: &[(&[u8], Error)] = &[
        (
            include_bytes!("../../../fixtures/mvp/failures/stored-integer-overflow.tex"),
            Error::Overflow,
        ),
        (
            include_bytes!("../../../fixtures/mvp/failures/letter-digit.tex"),
            Error::Number,
        ),
        (
            include_bytes!("../../../fixtures/mvp/failures/paragraph-argument.tex"),
            Error::Argument,
        ),
        (
            include_bytes!("../../../fixtures/mvp/failures/paragraph-stored-argument.tex"),
            Error::Argument,
        ),
        (
            include_bytes!("../../../fixtures/mvp/failures/paragraph-discarded-argument.tex"),
            Error::Argument,
        ),
        (
            include_bytes!("../../../fixtures/mvp/failures/group-kind.tex"),
            Error::Group,
        ),
        (
            include_bytes!("../../../fixtures/mvp/failures/divide-zero.tex"),
            Error::Overflow,
        ),
        (
            include_bytes!("../../../fixtures/mvp/failures/integer-overflow.tex"),
            Error::Overflow,
        ),
        (
            include_bytes!("../../../fixtures/mvp/failures/category-range.tex"),
            Error::Number,
        ),
        (
            include_bytes!("../../../fixtures/mvp/failures/extra-conditional.tex"),
            Error::Conditional,
        ),
    ];
    for &(source, expected) in cases {
        assert_eq!(Engine::new(source).run().err(), Some(expected));
    }
}
