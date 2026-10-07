use crate::{Engine, Error};

#[test]
fn short_integer_terminators_and_progress_cross_windows() {
    use crate::token::{DEFS, FROZEN_RELAX, Meaning, Span, Token, cmd};
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
            for _ in 0..padding {
                engine.arena.push(letter(b'P')).unwrap();
            }
            let start = engine.arena.len;
            for word in words {
                engine.arena.push(word).unwrap();
            }
            engine
                .push_tokens(Span {
                    store: DEFS,
                    start,
                    end: engine.arena.len,
                })
                .unwrap();
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
    use crate::token::{DEFS, Meaning, Span, Token, cmd};
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
            for _ in 0..padding {
                engine.arena.push(letter(b'P')).unwrap();
            }
            let start = engine.arena.len;
            for &word in &words {
                engine.arena.push(word).unwrap();
            }
            engine.arena.push(letter(b'X')).unwrap();
            engine
                .push_tokens(Span {
                    store: DEFS,
                    start,
                    end: engine.arena.len,
                })
                .unwrap();
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
