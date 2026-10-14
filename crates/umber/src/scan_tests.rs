use crate::{Engine, Error};

#[test]
fn expanded_names_preserve_identity_at_buffer_boundaries() {
    for length in [63, 64, 65, 127, 4097] {
        let prefix = "x".repeat(length - 1);
        let source = format!(
            r"\catcode123=1 \catcode125=2\relax
\def\piece{{{prefix}}}
\expandafter\def\csname\piece z\endcsname{{OK}}
\message{{UMBER:\csname {prefix}z\endcsname/\ifcsname {prefix}z\endcsname Y\else N\fi/\ifcsname {prefix}q\endcsname Y\else N\fi:END}}
\end"
        );
        assert_eq!(
            Engine::new(source.as_bytes()).run().unwrap().messages,
            [b"UMBER:OK/Y/N:END".as_slice()],
            "name length {length}"
        );
    }
}

#[test]
fn list_destination_failure_preserves_file_progress_and_lexical_state() {
    use crate::{scalars::Target, token::Token};
    let mut engine = Engine::new(b"{abc XYZ}");
    engine.scalars[Target::catcode(b'{' as usize)].value = 1;
    engine.scalars[Target::catcode(b'}' as usize)].value = 2;
    engine.opening_brace().unwrap();
    let mut consumed = 0;
    assert_eq!(
        engine.scan_raw_body(|_, _| {
            consumed += 1;
            if consumed == 3 {
                Err(Error::Capacity)
            } else {
                Ok(())
            }
        }),
        Err(Error::Capacity)
    );
    assert_eq!(engine.next_raw().unwrap().token, Token::character(10, b' '));
    assert_eq!(engine.next_raw().unwrap().token, Token::character(11, b'X'));
}

#[test]
fn file_integer_prefix_preserves_categories_and_progress() {
    use crate::{scalars::Target, token::Token};
    let letter = |byte| Token::character(11, byte);
    let cases = [
        ("7X", Ok(7), letter(b'X')),
        ("007X", Ok(7), letter(b'X')),
        ("0001234567X", Ok(1234567), letter(b'X')),
        ("-1234567X", Ok(-1234567), letter(b'X')),
        ("+1234567X", Ok(1234567), letter(b'X')),
        ("2147483647X", Ok(i32::MAX), letter(b'X')),
        ("2147483648X", Err(Error::Overflow), letter(b'X')),
        ("21474836470X", Err(Error::Overflow), letter(b'X')),
        ("7  X", Ok(7), letter(b'X')),
        ("7\tX", Ok(7), Token::other(9)),
        ("7% ignored\n8X", Ok(78), letter(b'X')),
        ("7\n8X", Ok(7), Token::other(b'8')),
    ];
    for loaded in [false, true] {
        for (operand, expected, next) in cases {
            let source = format!("{}{operand}", if loaded { "!" } else { "" });
            let mut engine = Engine::new(source.as_bytes());
            if loaded {
                assert_eq!(engine.next_raw().unwrap().token, Token::other(b'!'));
            }
            assert_eq!(engine.scan_int(), expected, "{operand:?}, loaded={loaded}");
            assert_eq!(engine.next_raw().unwrap().token, next);
        }
        let source = if loaded { "!123X" } else { "123X" };
        let mut engine = Engine::new(source.as_bytes());
        if loaded {
            engine.next_raw().unwrap();
        }
        assert_eq!(engine.scan_int(), Ok(123));
        engine.scalars[Target::catcode(b'X' as usize)].value = 12;
        assert_eq!(engine.next_raw().unwrap().token, letter(b'X'));
    }
    for (category, expected, next) in [
        (12, Ok(123), letter(b'X')),
        (11, Err(Error::Number), letter(b'1')),
        (10, Ok(23), letter(b'X')),
    ] {
        let mut engine = Engine::new(b"!123X");
        engine.scalars[Target::catcode(b'1' as usize)].value = category;
        engine.next_raw().unwrap();
        assert_eq!(engine.scan_int(), expected);
        assert_eq!(engine.next_raw().unwrap().token, next);
    }
    for (endline, expected) in [(-1, 78), (b'0' as i32, 708)] {
        let mut engine = Engine::new(b"!7\n8X");
        engine.scalars[Target::ENDLINE].value = endline;
        engine.next_raw().unwrap();
        assert_eq!(engine.scan_int(), Ok(expected));
        assert_eq!(engine.next_raw().unwrap().token, letter(b'X'));
    }
    for (category, expected, next) in [
        (11, 1, letter(b'2')),
        (10, 1, Token::other(b'3')),
        (9, 13, letter(b'X')),
    ] {
        let mut engine = Engine::new(b"!123X");
        engine.scalars[Target::catcode(b'2' as usize)].value = category;
        engine.next_raw().unwrap();
        assert_eq!(engine.scan_int(), Ok(expected));
        assert_eq!(engine.next_raw().unwrap().token, next);
    }
    let mut engine = Engine::new(b"!123^^34X");
    engine.scalars[Target::catcode(b'^' as usize)].value = 7;
    engine.next_raw().unwrap();
    assert_eq!(engine.scan_int(), Ok(1234));
    assert_eq!(engine.next_raw().unwrap().token, letter(b'X'));
}

#[test]
fn macro_integer_arguments_cross_windows() {
    // The padding and seven definitions put take's first argument word at
    // padding + 7. These offsets cross its two argument words and terminator.
    for padding in [0, 4087, 4088, 4089, 4094, 4095, 4096] {
        let source = format!(
            "\\catcode123=1 \\catcode125=2 \\catcode35=6\\relax\n\\def\\padding{{{}}}\n\
             \\def\\seven{{7}}\\chardef\\charseven=7\\relax\
             \\def\\take#1#2{{\\number#1#2X}}\
             \\message{{\\take{{7}}{{8}}/\\take{{ }}{{7}}/\\take{{}}{{7}}/\
             \\take{{\\seven}}{{8}}/\\take{{-7}}{{}}/\\take{{+7}}{{}}/\
             \\take{{\\charseven}}{{}}/\\take{{007}}{{8}}}}\\end\n",
            "P".repeat(padding)
        );
        let outcome = Engine::new(source.as_bytes()).run().unwrap();
        assert_eq!(
            outcome.messages,
            vec![b"78X/7X/7X/78X/-7X/7X/7X/78X".to_vec()],
            "padding {padding}"
        );
    }
}

#[test]
fn short_integer_terminators_and_progress_cross_windows() {
    use crate::token::{FROZEN_RELAX, Meaning, Token, cmd};
    let letter = |byte| Token::character(11, byte);
    for padding in [0, 4094, 4095, 4096] {
        for case in 0..10 {
            let mut engine = Engine::new(b"X");
            let alias = engine.intern(b"space").unwrap();
            engine.assign_meaning(
                alias as usize,
                Meaning {
                    command: cmd::SPACE,
                    value: 32,
                },
                false,
            );
            let (words, result, rest) = match case {
                0 => (
                    vec![Token::other(b'7'), letter(b'X')],
                    Ok(7),
                    vec![letter(b'X')],
                ),
                1 => (
                    vec![Token::other(b'7'), Token::character(10, b' '), letter(b'X')],
                    Ok(7),
                    vec![letter(b'X')],
                ),
                2 => (
                    vec![Token::other(b'7'), Token::cs(alias), letter(b'X')],
                    Ok(7),
                    vec![letter(b'X')],
                ),
                3 => (
                    vec![Token::other(b'7'), Token::character(10, b'0'), letter(b'X')],
                    Ok(7),
                    vec![letter(b'X')],
                ),
                4 => (vec![Token::other(b'7')], Ok(7), vec![letter(b'X')]),
                5 => (
                    vec![Token::other(b'7'), Token::other(b'8'), letter(b'X')],
                    Ok(78),
                    vec![letter(b'X')],
                ),
                6 => (
                    vec![Token::other(b'-'), Token::other(b'7'), letter(b'X')],
                    Ok(-7),
                    vec![letter(b'X')],
                ),
                7 => (
                    vec![Token::other(b'+'), Token::other(b'7'), letter(b'X')],
                    Ok(7),
                    vec![letter(b'X')],
                ),
                8 => (
                    vec![letter(b'7'), letter(b'X')],
                    Err(Error::Number),
                    vec![letter(b'7'), letter(b'X')],
                ),
                9 => (
                    vec![Token::other(b'7'), FROZEN_RELAX, letter(b'X')],
                    Ok(7),
                    vec![FROZEN_RELAX, letter(b'X')],
                ),
                _ => unreachable!(),
            };
            let mut tokens = engine.values.builder().unwrap();
            for _ in 0..padding {
                tokens.push(letter(b'P')).unwrap();
            }
            let start = tokens.len();
            for word in words {
                tokens.push(word).unwrap();
            }
            let owner = tokens.finish(None);
            engine.push_tokens(owner, start).unwrap();
            assert_eq!(engine.scan_int(), result, "padding {padding}, case {case}");
            for word in rest {
                assert_eq!(
                    engine.next_raw().unwrap().token,
                    word,
                    "padding {padding}, case {case}"
                );
            }
        }
    }
}

#[test]
fn keyword_matches_and_rollback_cross_arena_windows() {
    use crate::token::{Meaning, Token, cmd};
    let letter = |byte| Token::character(11, byte);
    for padding in [0, 4093, 4094, 4095, 4096, 8191] {
        for case in 0..15 {
            let mut engine = Engine::new(b"");
            let alias = engine.intern(b"alias").unwrap();
            engine.assign_meaning(
                alias as usize,
                Meaning {
                    command: if case == 10 { cmd::SPACE } else { 11 },
                    value: if case == 7 { b'y' } else { b'b' } as i32,
                },
                false,
            );
            let space = Token::character(10, b' ');
            let (words, matched): (Vec<_>, _) = match case {
                0 => (vec![letter(b'b'), letter(b'y')], true),
                1 => (vec![letter(b'B'), letter(b'Y')], true),
                2 => (
                    vec![Token::character(1, b'b'), Token::character(2, b'y')],
                    true,
                ),
                3 => (vec![Token::character(6, b'b'), Token::other(b'y')], true),
                4 => (vec![letter(b'b'), letter(b'Z')], false),
                5 => (vec![letter(b'y'), letter(b'b')], false),
                6 => (vec![Token::cs(alias), letter(b'y')], false),
                7 => (vec![letter(b'b'), Token::cs(alias)], false),
                8 => (vec![space, letter(b'b'), letter(b'y')], true),
                9 => (vec![space, letter(b'b'), letter(b'Z')], false),
                10 => (vec![Token::cs(alias), letter(b'b'), letter(b'y')], true),
                11 => (vec![Token::character(10, b'b'), letter(b'y')], true),
                12 => (
                    vec![Token::character(10, b'q'), letter(b'b'), letter(b'y')],
                    true,
                ),
                13 => (vec![letter(b'b'), Token::character(10, b'y')], true),
                14 => (vec![Token::character(10, b'B'), letter(b'Y')], true),
                _ => unreachable!(),
            };
            let mut tokens = engine.values.builder().unwrap();
            for _ in 0..padding {
                tokens.push(letter(b'P')).unwrap();
            }
            let start = tokens.len();
            for &word in &words {
                tokens.push(word).unwrap();
            }
            tokens.push(letter(b'X')).unwrap();
            let owner = tokens.finish(None);
            engine.push_tokens(owner, start).unwrap();
            assert_eq!(engine.scan_keyword(b"by").unwrap(), matched);
            if !matched {
                // Leading spaces are discarded; the partially matched prefix
                // and failing token retain their original identities and order.
                for &word in words.iter().skip_while(|word| word.is_space()) {
                    assert_eq!(engine.next_raw().unwrap().token, word);
                }
            }
            assert_eq!(engine.next_raw().unwrap().token, letter(b'X'));
        }
    }
}
