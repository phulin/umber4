use crate::{Engine, Error};

#[test]
fn serialized_spelling_survives_name_table_growth() {
    use crate::token::Token;
    for spelling in [
        b"a".to_vec(),
        b"retainedspelling".to_vec(),
        vec![b'x'; 4097],
    ] {
        let mut engine = Engine::new(b"");
        let id = engine.intern(&spelling).unwrap();
        let mut output = Vec::new();
        engine
            .token_text(Token::cs(id), true, |engine, byte| {
                output.push(byte);
                if output.len() == 2 {
                    for index in 0..1500 {
                        engine.intern(format!("addedname{index}").as_bytes())?;
                    }
                }
                Ok(())
            })
            .unwrap();
        let mut expected = vec![b'\\'];
        expected.extend_from_slice(&spelling);
        expected.push(b' ');
        assert_eq!(output, expected);
    }
}

#[test]
fn decimal_sources_retain_signs_digits_and_nested_backing() {
    use crate::token::Token;
    let mut values = vec![0, 7, 42, -9, -10, 99, 100, i32::MIN, i32::MAX];
    for power in 1..=9 {
        let boundary = 10_i32.pow(power);
        values.extend([
            boundary - 1,
            boundary,
            boundary + 1,
            -boundary + 1,
            -boundary,
            -boundary - 1,
        ]);
    }
    let mut engine = Engine::new(b"");
    for _ in 0..3 {
        // Pending sources retain their original buffers while later numbers
        // are built. The following passes then exercise reuse after retirement.
        for &value in &values {
            engine.insert_number(value).unwrap();
        }
        for &value in values.iter().rev() {
            for byte in value.to_string().bytes() {
                assert_eq!(engine.next_raw().unwrap().token, Token::other(byte));
            }
        }
        engine.retire_exhausted();
        assert!(engine.sources.is_empty());
    }
}

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
        (
            include_bytes!("../../../fixtures/mvp/name-scanning.tex"),
            include_str!("../../../fixtures/mvp/name-scanning.expected"),
        ),
        (
            include_bytes!("../../../fixtures/mvp/control-word-names.tex"),
            include_str!("../../../fixtures/mvp/control-word-names.expected"),
        ),
        (
            include_bytes!("../../../fixtures/mvp/list-windows.tex"),
            include_str!("../../../fixtures/mvp/list-windows.expected"),
        ),
        (
            include_bytes!("../../../fixtures/mvp/number-emission.tex"),
            include_str!("../../../fixtures/mvp/number-emission.expected"),
        ),
        (
            include_bytes!("../../../fixtures/mvp/symbol-names.tex"),
            include_str!("../../../fixtures/mvp/symbol-names.expected"),
        ),
        (
            include_bytes!("../../../fixtures/mvp/macro-equality.tex"),
            include_str!("../../../fixtures/mvp/macro-equality.expected"),
        ),
        (
            include_bytes!("../../../fixtures/mvp/file-arguments.tex"),
            include_str!("../../../fixtures/mvp/file-arguments.expected"),
        ),
        (
            include_bytes!("../../../fixtures/mvp/delimited-runs.tex"),
            include_str!("../../../fixtures/mvp/delimited-runs.expected"),
        ),
        (
            include_bytes!("../../../fixtures/mvp/delimited-windows.tex"),
            include_str!("../../../fixtures/mvp/delimited-windows.expected"),
        ),
        (
            include_bytes!("../../../fixtures/mvp/arithmetic-keywords.tex"),
            include_str!("../../../fixtures/mvp/arithmetic-keywords.expected"),
        ),
        (
            include_bytes!("../../../fixtures/mvp/short-numbers.tex"),
            include_str!("../../../fixtures/mvp/short-numbers.expected"),
        ),
        (
            include_bytes!("../../../fixtures/mvp/source-space-runs.tex"),
            include_str!("../../../fixtures/mvp/source-space-runs.expected"),
        ),
    ];
    for &(source, expected) in fixtures {
        let result = Engine::new(source).run().unwrap();
        let expected: Vec<_> = expected.lines().map(|line| line.as_bytes()).collect();
        assert_eq!(result.messages, expected);
    }
}

#[test]
fn macro_equality_crosses_shifted_chunk_boundaries() {
    for length in [1, 2, 4093, 4096, 8193, 12289] {
        let text = "A".repeat(length);
        let padding = "P".repeat(17 + length % 53);
        let mut source = format!(
            r"\catcode123=1 \catcode125=2 \catcode35=6\relax
\def\first Q#1aaba#2{{{text}#2/#1\relax}}
\def\padding{{{padding}}}
\def\second Q#1aaba#2{{{text}#2/#1\relax}}
\message{{\ifx\first\second Y\else N\fi}}"
        );
        let mut expected = vec![b"Y".as_slice()];
        let mut positions = vec![0, length / 2, length - 1];
        positions.extend([4095, 8191].into_iter().filter(|&p| p < length));
        positions.sort_unstable();
        positions.dedup();
        for position in positions {
            let mut changed = text.clone();
            changed.replace_range(position..position + 1, "B");
            source.push_str(&format!(
                r"\def\second Q#1aaba#2{{{changed}#2/#1\relax}}
\message{{\ifx\first\second Y\else N\fi}}"
            ));
            expected.push(b"N");
        }
        // Prefix identity and total length remain part of the complete macro
        // signature, alongside body characters, parameter references and names.
        for (prefix, body) in [('R', text.as_str()), ('Q', &text[..length - 1])] {
            source.push_str(&format!(
                r"\def\second {prefix}#1aaba#2{{{body}#2/#1\relax}}
\message{{\ifx\first\second Y\else N\fi}}"
            ));
            expected.push(b"N");
        }
        source.push_str(r"\end");
        let actual = Engine::new(source.as_bytes()).run().unwrap().messages;
        assert_eq!(actual, expected, "body length {length}");
    }
}

#[test]
fn arena_windows_retain_whole_chunk_access() {
    use crate::token::Token;
    let mut values = crate::values::Values::default();
    let mut builder = values.builder();
    for index in 0..4100 {
        builder.push(Token::other(index as u8)).unwrap();
    }
    let value = values.publish(builder, None).unwrap();
    let (first, end, next) = value.tokens.window(4094, 4100);
    assert_eq!(next, 4096);
    let mut neighbor = values.builder();
    neighbor.push(Token::other(99)).unwrap();
    // SAFETY: value retains the page, and neighbor writes a disjoint suffix.
    unsafe {
        assert_eq!(first.read(), Token::other(254));
        assert_eq!(first.add(1).read(), Token::other(255));
        assert_eq!(first.add(2), end);
    }
    assert!(value.tokens.equal(1, &value.tokens, 257, 3843));
    assert!(!value.tokens.equal(1, &value.tokens, 258, 3842));
    assert!(value.tokens.equal(value.tokens.len, &value.tokens, 0, 0));
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
fn integer_overflow_consumes_its_digit_across_arena_windows() {
    use crate::token::{Token, cmd};
    for number in ["2147483648", "'20000000000", "\"80000000"] {
        for padding in [0, 4079, 4087, 4090] {
            let source = format!(
                r"\catcode123=1 \catcode125=2\relax
\def\padding{{{}}}\def\digits{{{number}X}}\end\digits",
                "P".repeat(padding)
            );
            let mut engine = Engine::new(source.as_bytes());
            loop {
                let read = engine.next_expanded().unwrap();
                if read.meaning.command == cmd::END {
                    break;
                }
                engine.execute(read).unwrap();
            }
            assert_eq!(engine.scan_int(), Err(Error::Overflow));
            assert_eq!(engine.next_raw().unwrap().token, Token::character(11, b'X'));
        }
    }
}

#[test]
fn nested_number_limits_match_in_scanners_and_expanded_lists() {
    for depth in [255, 256, 257] {
        let number = format!("{}7", r"\number".repeat(depth));
        for statement in [
            format!(r"\count0={number}\relax\message{{\the\count0}}"),
            format!(r"\message{{{number}}}"),
            format!(r"\edef\saved{{{number}}}\message{{\saved}}"),
        ] {
            let source = format!(r"\catcode123=1 \catcode125=2\relax{statement}\end");
            let result = Engine::new(source.as_bytes()).run();
            if depth > 256 {
                assert_eq!(result.err(), Some(Error::Capacity), "depth {depth}");
            } else {
                assert_eq!(result.unwrap().messages, [b"7"], "depth {depth}");
            }
        }
    }
}

#[test]
fn conversion_operand_errors_precede_destination_capacity() {
    for (operand, expected) in [
        (br"{\detokenize{ABCDE}}".as_slice(), Error::Capacity),
        (br"{\detokenize{ABCDE".as_slice(), Error::Eof),
        (br"{\detokenize ABCDE}".as_slice(), Error::Syntax),
    ] {
        let mut source = b"\\catcode123=1 \\catcode125=2\\relax\\end ".to_vec();
        source.extend_from_slice(operand);
        let mut engine = Engine::new(&source);
        // Stop the ordinary driver before the list, retaining its live state.
        loop {
            let read = engine.next_expanded().unwrap();
            if read.meaning.command == crate::token::cmd::END {
                break;
            }
            engine.execute(read).unwrap();
        }
        engine.opening_brace().unwrap();
        let result = engine.scan_body::<true>(Some(0), |_, _| Err(Error::Capacity));
        assert_eq!(result, Err(expected));
    }
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
fn stored_list_windows_preserve_nested_and_parameter_boundaries() {
    let left = "a".repeat(4103);
    let right = "b".repeat(8231);
    // Native messages print parameter tokens doubled and preserve protected
    // macros. The small list-windows fixture records the same oracle behavior.
    let text = format!("{left}{{{right}}}##");
    let raw = format!("{left}{{{right}}}####");
    let source = format!(
        r"\catcode123=1 \catcode125=2 \catcode35=6\relax
\def\stored{{{left}{{{right}}}####}}\def\pass#1{{[#1/#1]}}
\protected\def\keep{{K}}
\edef\actual{{\pass{{\stored}}\noexpand\keep\unexpanded{{#}}}}
\message{{\actual}}
\toks0=\expandafter{{\stored}}\edef\actual{{\the\toks0}}
\message{{\actual}}
\edef\actual{{\unexpanded\expandafter{{\stored}}}}
\message{{\actual}}\end"
    );
    let actual = Engine::new(source.as_bytes()).run().unwrap().messages;
    for (index, (actual, expected)) in actual
        .iter()
        .zip([format!(r"[{text}/{text}]\keep ##"), raw.clone(), raw])
        .enumerate()
    {
        assert_eq!(actual.len(), expected.len(), "message {index} length");
        assert!(actual == expected.as_bytes(), "message {index} bytes");
    }
    assert_eq!(actual.len(), 3);
}

#[test]
fn retained_argument_windows_survive_generated_buffer_growth() {
    let inputs = [
        format!(
            "{} {{IJ KL}} {}",
            "AbCdEfGh".repeat(1601),
            "MN".repeat(2053)
        ),
        format!("{} {{OP QR}}", "STuV".repeat(2051)),
    ];
    let mut source = String::from(
        r"\catcode123=1 \catcode125=2 \catcode35=6\relax
\def\inspect#1{\edef\saved{\detokenize{#1}}\message{\saved}
\lowercase{\def\lowered{#1}}\message{\lowered}
\message{\detokenize{#1}}}",
    );
    for input in &inputs {
        source.push_str(&format!(r"\inspect{{{input}}}"));
    }
    source.push_str(r"\end");
    let actual = Engine::new(source.as_bytes()).run().unwrap().messages;
    let expected: Vec<_> = inputs
        .iter()
        .flat_map(|input| [input.clone(), input.to_ascii_lowercase(), input.clone()])
        .collect();
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(actual.len(), expected.len(), "message {index} length");
        assert!(actual == expected.as_bytes(), "message {index} bytes");
    }
}

#[test]
fn stored_parameter_substitution_crosses_window_edges() {
    for length in [4093, 4094, 8190, 8191] {
        let prefix = "A".repeat(length);
        let suffix = "B".repeat(4097);
        let source = format!(
            r"\catcode123=1 \catcode125=2 \catcode35=6\relax
\def\pass#1{{{prefix}#1X#1{suffix}}}
\edef\actual{{\pass{{{{Y}}}}}}\message{{\actual}}\end"
        );
        let actual = Engine::new(source.as_bytes()).run().unwrap().messages;
        let expected = format!("{prefix}{{Y}}X{{Y}}{suffix}");
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].len(), expected.len(), "prefix length {length}");
        assert!(actual[0] == expected.as_bytes(), "prefix length {length}");
    }
}

#[test]
fn delimited_windows_cross_chunks_and_argument_sources() {
    for length in [1, 4093, 4094, 4095, 4096, 4097, 8191, 8192, 8193] {
        let text = "X".repeat(length);
        // The padding also places aaba across the delimiter arena's chunk end
        // in two alignments, exercising matching through a delimiter chunk end.
        let padding = "P".repeat(4092 + length % 4);
        let source = format!(
            r"\catcode123=1 \catcode125=2 \catcode35=6\relax
\def\padding{{{padding}}}
\def\grab#1aaba{{\edef\held{{#1/#1}}\message{{\held}}}}
\def\stored{{\grab {text}aaaba}}\stored
\def\forward#1{{\grab#1}}\forward{{{text}aaaba}}
\def\across#1#2{{\grab#1#2}}\across{{{text}aa}}{{aba}}
\def\stored{{\grab{{{text}}}aaba}}\stored\end"
        );
        let actual = Engine::new(source.as_bytes()).run().unwrap().messages;
        let overlapping = format!("{text}a/{text}a").into_bytes();
        let grouped = format!("{text}/{text}").into_bytes();
        assert_eq!(actual.len(), 4);
        for (index, message) in actual.iter().enumerate() {
            let expected = if index < 3 { &overlapping } else { &grouped };
            assert_eq!(
                message.len(),
                expected.len(),
                "length {length}, message {index}"
            );
            assert!(message == expected, "length {length}, message {index}");
        }
    }
}

#[test]
fn delimited_window_errors_consume_through_the_failing_token() {
    for source in [
        include_bytes!("../../../fixtures/mvp/failures/paragraph-delimited-window.tex").as_slice(),
        include_bytes!("../../../fixtures/mvp/failures/group-paragraph-delimited-window.tex")
            .as_slice(),
        include_bytes!("../../../fixtures/mvp/failures/end-group-delimited-window.tex").as_slice(),
    ] {
        let mut engine = Engine::new(source);
        let error = loop {
            let read = match engine.next_expanded() {
                Ok(read) => read,
                Err(error) => break error,
            };
            engine.execute(read).unwrap();
        };
        assert_eq!(error, Error::Argument);
        assert_eq!(
            engine.next_raw().unwrap().token,
            crate::token::Token::character(11, b'C')
        );
    }
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
#[test]
fn grouped_character_runs_preserve_following_tokens() {
    for padding in 0..4 {
        for length in (0..=12).chain([255, 256, 257]) {
            let payload = "A".repeat(length);
            let source = format!(
                r"\catcode123=1 \catcode125=2 \catcode35=6\relax
\def\padding{{{}}}\def\id#1{{#1}}\def\ignore#1{{I}}
\def\forward#1{{\id{{#1}}/\ignore{{#1}}}}
\def\invoke{{\id{{{payload}}}B/\ignore{{{payload}}}D/\forward{{{payload}}}E}}
\message{{\invoke/\id{{{payload}}}F/\ignore{{{payload}}}G}}\end",
                "P".repeat(padding)
            );
            let expected = format!("{payload}B/ID/{payload}/IE/{payload}F/IG");
            let outcome = Engine::new(source.as_bytes()).run().unwrap();
            assert_eq!(
                outcome.messages,
                [expected.as_bytes()],
                "{padding}/{length}"
            );
        }
    }
}

#[test]
fn skipped_runs_preserve_aliases_parameters_and_line_state() {
    for padding in 0..4 {
        for length in [0, 1, 4093, 4095, 4096, 8193] {
            let text = "A".repeat(length);
            let source = format!(
                r"\catcode123=1 \catcode125=2 \catcode35=6 \catcode94=7\relax
\def\padding{{{}}}\let\finish=\fi\let\otherwise=\else
\catcode64=13\let@=\else
\def\skip#1{{\iffalse{{{text}}}#1{text}\finish STORED}}
\message{{UMBER:\skip{{\iftrue {text}\fi}}:END}}
\message{{UMBER:\iffalse {text}^^40FILE\finish:END}}
\let\par=\otherwise
\message{{UMBER:\iffalse{{{text}}}

PAR\finish:END}}\end",
                "P".repeat(padding)
            );
            let result = Engine::new(source.as_bytes()).run().unwrap();
            assert_eq!(
                result.messages,
                [
                    b"UMBER:STORED:END".to_vec(),
                    b"UMBER:FILE:END".to_vec(),
                    b"UMBER:PAR:END".to_vec()
                ],
                "padding {padding}, length {length}"
            );
        }
    }
}
