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
fn scanner_completion_leaves_unread_body_for_execution() {
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
fn body_exhaustion_scanners_read_parent_input() {
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
    assert_eq!(stats.max_native_depth, 1);
    assert_eq!(stats.tail_calls, 2);
}
#[test]
fn nested_integers_and_split_keyword_scan_across_macros() {
    compare(
        r"\count0=1\relax\count1=2\relax\count2=7\relax\count3=-\count\count\count0\relax\count4=\number\number123\relax\let\A\advance\let\C\count\def\by{b}\def\why{y}\A\C0\by\why\C2\relax\iffalse\iftrue\count0=999\else\count0=998\fi\else\ifnum\count3<0\relax\else\count0=997\fi\fi\message{\the\count0/\the\count3/\the\count4}\end",
        &[b"8/-7/123"],
    );
}
#[test]
fn native_assignments_share_values_and_group_restoration() {
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
fn source_integer_scanning_preserves_progress_and_categories() {
    let letter = |byte| Token::character(11, byte);
    for (source, expected, next) in [
        ("123X", Ok(123), letter(b'X')),
        ("-123X", Ok(-123), letter(b'X')),
        ("0000000007X", Ok(7), letter(b'X')),
        ("2147483647X", Ok(i32::MAX), letter(b'X')),
        ("2147483648X", Err(Error::Overflow), letter(b'X')),
        ("21474836470X", Err(Error::Overflow), letter(b'X')),
        ("7  X", Ok(7), Token::other(b'X')),
        ("7% ignored\n8X", Ok(78), letter(b'X')),
        ("7\n8X", Ok(7), Token::other(b'8')),
        ("123^^34X", Ok(1234), letter(b'X')),
    ] {
        let mut producer = Producer::<false>::new(Engine::new(source.as_bytes()));
        producer.engine.scalars[Target::catcode(b'^' as usize)].value = 7;
        let mut input = Frame {
            body: None,
            back: None,
            parent: std::ptr::null_mut(),
        };
        assert_eq!(producer.integer(&mut input, 0), expected, "{source}");
        // A category assignment after scanning must preserve the lookahead's
        // original token, including when it was found in a source digit run.
        producer
            .engine
            .assign_integer(Target::catcode(b'X' as usize), false, 12);
        let next = if expected.is_err() {
            Token::other(b'X')
        } else {
            next
        };
        assert_eq!(producer.raw(&mut input).unwrap().token, next, "{source}");
    }
}
#[test]
fn source_digit_runs_cross_expansion_and_line_boundaries() {
    compare(
        "\\def\\more{45}\\count0=12^^33\\more67\\relax\\count1=0000000007\\relax\\message{\\the\\count0/\\the\\count1}\\end",
        &[b"1234567/7"],
    );
    compare(
        "\\endlinechar=-1\\relax\n\\count0=12\n34\\relax\\message{\\the\\count0}\\end",
        &[b"1234"],
    );
    compare(
        r"\catcode49=13\def1{2}\count0=111\relax\message{\the\count0}\end",
        &[b"222"],
    );
}
#[test]
fn pending_suffix_observes_redefined_inert_commands() {
    compare(
        r"\def\value{12\relax\let\relax\message\relax{ACTIVE}}\count0=\value\message{\the\count0}\end",
        &[b"ACTIVE", b"12"],
    );
}
#[test]
fn nested_internal_counts_complete_in_order() {
    compare(
        r"\count0=1\relax\count1=2\relax\count2=7\relax\ifnum\count\count0<\count\count\count0 \message{LESS}\else\message{WRONG}\fi\message{\the\count\count0/\number-\count\count\count0/\number\number\count\count0}\end",
        &[b"LESS", b"2/-7/2"],
    );
}
#[test]
fn native_assignments_preserve_operand_terminators() {
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
fn native_assignments_cross_expansion_and_page_boundaries() {
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
fn native_assignments_preserve_supported_errors() {
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
fn native_numbers_and_nested_conditionals_preserve_order() {
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
    assert_eq!(stats.max_native_depth, 1);
    assert_eq!(stats.macro_calls, 50000);
    assert_eq!(stats.tail_calls, 49999);
    assert!(stats.max_integer_depth <= 3, "{stats:?}");
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

#[test]
fn native_scanners_cross_several_callers_and_prefixes() {
    compare(
        r"\def\inner{\count0=}\def\outer{\inner123\relax\advance\count1by2\relax}\outer\message{\the\count0/\the\count1}\end",
        &[b"123/2"],
    );
    compare(
        r"\let\O={\def\leaf{\message\O A}\def\middle{\leaf B}\def\outer{\middle C}\outer D}\end",
        &[b"ABCD"],
    );
    compare(
        r"\def\prefix{\global}\def\setter{\prefix\count0=}{\setter7\relax}\message{\the\count0}\end",
        &[b"7"],
    );
}
#[test]
fn pending_ancestor_ranges_can_end_under_a_native_scanner() {
    compare(
        r"\def\endset{\count1=}\def\value{12\relax\endset7}\count0=\value8\relax\message{\the\count0/\the\count1}\end",
        &[b"12/78"],
    );
    compare(
        r"\def\endset{\count1=\number}\def\value{12\relax\endset7}\count0=\value\relax\message{\the\count0/\the\count1}\end",
        &[b"12/7"],
    );
}
#[test]
fn pending_body_owners_survive_redefinition_and_execute_their_suffix() {
    compare(
        r"\def\value{12\relax\def\value{NEW}\message{OLD}}\count0=\value\message{\value/\the\count0}\end",
        &[b"OLD", b"NEW/12"],
    );
}
#[test]
fn decimal_expansion_precedes_its_backed_terminator() {
    compare(
        r"\count0=\number\number-123\relax\message{\number0/\number2147483647/\number-2147483647/\the\count0}\end",
        &[b"0/2147483647/-2147483647/-123"],
    );
}
#[test]
fn repeated_empty_expansions_keep_native_scanner_depth_bounded() {
    let source = format!(
        r"\def\empty{{}}\def\minus{{-}}\count0={}{}7\relax\message{{\the\count0}}\end",
        r"\empty".repeat(1024),
        r"\minus".repeat(1024),
    );
    let stats = compare(&source, &[b"7"]);
    assert_eq!(stats.max_native_depth, 0);
    assert_eq!(stats.max_integer_depth, 1);
    assert!(stats.max_pending_sources <= 3, "{stats:?}");
}
#[test]
fn native_scanner_recursion_has_a_checked_limit() {
    for source in [
        format!(r"\message{{{}0}}\end", r"\number".repeat(300)),
        format!(r"\message{{{}\count0}}\end", r"\the".repeat(300)),
        r"\def\again{\the\again}\message{\again}\end".into(),
    ] {
        assert_eq!(
            Engine::new(format!("{BOOT}{source}").as_bytes())
                .run_producer()
                .err(),
            Some(Error::Capacity)
        );
    }
}

#[test]
fn nested_decimal_expansion_releases_terminators_as_they_are_read() {
    let source = format!(r"\message{{{}7}}\end", r"\number".repeat(200));
    let stats = compare(&source, &[b"7"]);
    assert_eq!(stats.max_integer_depth, 200);
    assert!(stats.max_pending_sources <= 3, "{stats:?}");
}

#[test]
fn pending_bodies_can_read_suspended_ancestor_ranges() {
    compare(
        r"\def\endset{\count1=}\def\inner{12\relax\endset7}\def\outer{\inner8\relax}\count0=\outer\message{\the\count0/\the\count1}\end",
        &[b"12/78"],
    );
}

#[test]
fn native_keywords_cross_a_retained_page_boundary() {
    let source = format!(
        r"\def\work{{{}\advance\count0bY7\relax}}\work\message{{\the\count0}}\end",
        r"\relax".repeat(4091),
    );
    compare(&source, &[b"7"]);
    compare(
        r"\def\why{y}\def\by{b}\def\work{\advance\count0\by\why3\relax}\work\advance\count0 4\relax\message{\the\count0}\end",
        &[b"7"],
    );
}

#[test]
fn native_end_drops_unread_ancestor_input() {
    compare(
        r"\def\inner{12\relax\end}\def\outer{\inner\message{UNREACHABLE}}\count0=\outer",
        &[],
    );
}
