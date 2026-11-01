use super::*;
use crate::{Engine, Progress, font_test_data, scalars::Target, scanner::Scanner};

pub(super) const BOOT: &str = "\\catcode123=1\\catcode125=2\\catcode35=6\\relax\n";

pub(super) fn loaded(body: &str) -> Hyphenation {
    let text = format!("{BOOT}{body}\\end");
    let mut engine = Engine::new(text.as_bytes());
    engine.advance(usize::MAX).unwrap();
    engine.document.hyphenation.clone()
}

#[test]
fn hyphenation_sparse_weights_boundaries_languages_and_minima() {
    assert_eq!(size_of::<TrieNode>(), 16);
    assert_eq!(size_of::<Operation>(), 2);
    let tables = loaded(r"\patterns{a1b ab2c b3c .ab4c a5bc.}");
    // Overlapping maximum weights can inhibit an otherwise odd break.
    assert_eq!(tables.positions(0, b"abc", 1, 1), 2);
    assert_eq!(tables.positions(0, b"xabc", 1, 1), 4 | 8);
    assert_eq!(tables.positions(0, b"abcx", 1, 1), 2);
    assert_eq!(tables.positions(0, b"abc", 2, 1), 0);
    assert_eq!(tables.positions(0, b"abc", 0, -1), 2);
    assert_eq!(tables.positions(0, b"abc", 63, 63), 0);
    assert_eq!(tables.positions(1, b"abc", 1, 1), 0);
    let tables = loaded(
        r"\language=255\patterns{a1b}\language=256\patterns{b1c}\language=-7\hyphenation{a-bc}",
    );
    assert_eq!(tables.positions(255, b"abc", 1, 1), 2);
    assert_eq!(tables.positions(0, b"abc", 1, 1), 2);
    assert_eq!(tables.positions(0, b"bc", 1, 1), 2);
}

#[test]
fn hyphenation_zero_patterns_duplicates_and_exception_precedence() {
    let tables = loaded(r"\patterns{ab ab a1b}\hyphenation{ab a-bc ab-c a--bc -a-b-c- a a-}");
    assert_eq!(tables.positions(0, b"ab", 1, 1), 0);
    assert_eq!(tables.positions(0, b"abc", 1, 1), 2 | 4);
    let patterns = tables.patterns.as_ref().unwrap();
    assert_eq!(patterns.operations.len(), 1);
    for body in [r"\patterns{a1b ab}", r"\patterns{a1b a1b}"] {
        let text = format!("{BOOT}{body}\\end");
        let mut engine = Engine::new(text.as_bytes());
        assert_eq!(engine.advance(usize::MAX), Err(Error::Syntax));
        assert_eq!(engine.document.hyphenation.positions(0, b"ab", 1, 1), 2);
    }
    let tables = loaded(r"\patterns{1.ab.1 2.ab.2 .a1b.}");
    assert_eq!(tables.positions(0, b"ab", 1, 1), 2);
}

#[test]
fn hyphenation_readers_expand_in_normal_context_and_publish_per_entry() {
    let text = format!(
        r"{BOOT}\outer\def\P{{A1B}}\outer\def\E{{A-B}}\def\S{{ }}
\let\letter=a\chardef\given=98
\def\after{{\advance\count0by1}}
{{\globaldefs=-1\afterassignment\after\global\patterns\relax{{\P\S\letter1c}}
\afterassignment\after\hyphenation{{\E\S a\given-c a\char98-d}}}}
\end"
    );
    let mut engine = Engine::new(text.as_bytes());
    engine.advance(usize::MAX).unwrap();
    assert_eq!(engine.scanner, Scanner::Normal);
    assert_eq!(engine.outcome().counts[0], 0); // afterassignment ran locally.
    let tables = &engine.document.hyphenation;
    assert_eq!(tables.positions(0, b"ab", 1, 1), 2);
    assert_eq!(tables.positions(0, b"ac", 1, 1), 2);
    assert_eq!(tables.positions(0, b"abc", 1, 1), 4);
    assert_eq!(tables.positions(0, b"abd", 1, 1), 4);
    for (body, word) in [
        (r"\patterns{a1b c1d\relax}", b"ab".as_slice()),
        (r"\hyphenation{a-b c-d\relax}", b"ab".as_slice()),
    ] {
        let text = format!("{BOOT}{body}\\end");
        let mut engine = Engine::new(text.as_bytes());
        let before = engine.checkpoint().unwrap();
        assert_eq!(engine.advance(usize::MAX), Err(Error::Syntax));
        assert_eq!(engine.document.hyphenation.positions(0, word, 1, 1), 2);
        assert_eq!(engine.document.hyphenation.positions(0, b"cd", 1, 1), 0);
        engine.restore(&before).unwrap();
        assert!(engine.document.hyphenation.patterns.is_none());
        assert!(engine.document.hyphenation.exceptions.is_none());
    }
}

#[test]
fn hyphenation_scanner_command_distinctions_digits_and_truncation() {
    for body in [
        r"\patterns{a12b}",
        r"\chardef\a=97\patterns{\a1b}",
        r"\patterns{\char97b}",
        r"\patterns{{a1b}}",
        r"\hyphenation{a1b}",
        r"\hyphenation{\relax}",
    ] {
        let text = format!("{BOOT}{body}\\end");
        assert_eq!(
            Engine::new(text.as_bytes()).run().err(),
            Some(Error::Syntax),
            "{body}"
        );
    }
    let tables = loaded(r"\lccode`2=`x\patterns{a12b}");
    assert_eq!(tables.positions(0, b"axb", 1, 1), 2);
    let word = "a".repeat(63);
    let text = format!(
        "\\patterns{{a1{}aaaa}}\\hyphenation{{a-{}aaaa}}",
        "a".repeat(62),
        "a".repeat(62)
    );
    let tables = loaded(&text);
    assert_eq!(tables.positions(0, word.as_bytes(), 1, 1), 2);
    assert_eq!(tables.positions(0, "a".repeat(64).as_bytes(), 1, 1), 0);
    for command in ["patterns", "hyphenation"] {
        let text = format!("{BOOT}\\{command}{{{}!}}\\end", "a".repeat(64));
        assert_eq!(
            Engine::new(text.as_bytes()).run().err(),
            Some(Error::Syntax)
        );
    }
}

#[test]
fn hyphenation_lazy_roots_resize_replacement_and_checkpoint_ownership() {
    let mut tables = Hyphenation::default();
    assert!(tables.patterns.is_none() && tables.exceptions.is_none());
    tables.pattern(0, b"ab", &[0, 1, 0]).unwrap();
    let pattern_owner = Rc::downgrade(tables.patterns.as_ref().unwrap());
    let initial = tables.clone();
    assert_eq!(pattern_owner.strong_count(), 2);
    for i in 0..1000u32 {
        let word = [
            b'a' + (i / 26 / 26) as u8,
            b'a' + (i / 26 % 26) as u8,
            b'a' + (i % 26) as u8,
        ];
        tables.exception(0, &word, 2).unwrap();
    }
    let exceptions = tables.exceptions.as_ref().unwrap();
    assert!(exceptions.entries.len() - 1 <= exceptions.buckets.len() / 4 * 3);
    let exception_owner = Rc::downgrade(exceptions);
    let snapshot = tables.clone();
    assert_eq!(pattern_owner.strong_count(), 3);
    assert_eq!(exception_owner.strong_count(), 2);
    tables.exception(0, b"aaa", 0).unwrap();
    assert_eq!(tables.positions(0, b"aaa", 1, 1), 0);
    assert_eq!(snapshot.positions(0, b"aaa", 1, 1), 2);
    assert_eq!(pattern_owner.strong_count(), 3);
    for i in 0..1000u32 {
        let word = [
            b'a' + (i / 26 / 26) as u8,
            b'a' + (i / 26 % 26) as u8,
            b'a' + (i % 26) as u8,
        ];
        assert_eq!(snapshot.positions(0, &word, 1, 1), 2);
    }
    drop(tables);
    assert_eq!(snapshot.positions(0, b"ab", 1, 1), 2);
    drop(snapshot);
    assert!(exception_owner.upgrade().is_none());
    drop(initial);
    assert!(pattern_owner.upgrade().is_none());
}

#[test]
fn hyphenation_shared_roots_fork_pattern_pages_and_resized_exception_buckets() {
    let mut original = Hyphenation::default();
    original.pattern(0, b"ab", &[0, 1, 0]).unwrap();
    original.exception(0, b"ab", 0).unwrap();
    let mut branch = original.clone();
    branch.pattern(0, b"ac", &[0, 3, 0]).unwrap();
    branch.exception(0, b"ab", 2).unwrap();
    // Add enough keys to resize the branch's bucket directory while the
    // original still retains both old roots and their shared table pages.
    for c in b'a'..=b'z' {
        branch.exception(1, &[c, b'x'], 2).unwrap();
    }
    assert_eq!(original.positions(0, b"ab", 1, 1), 0);
    assert_eq!(original.positions(0, b"ac", 1, 1), 0);
    assert_eq!(original.positions(1, b"ax", 1, 1), 0);
    assert_eq!(branch.positions(0, b"ab", 1, 1), 2);
    assert_eq!(branch.positions(0, b"ac", 1, 1), 2);
    for c in b'a'..=b'z' {
        assert_eq!(branch.positions(1, &[c, b'x'], 1, 1), 2);
    }
    drop(branch);
    original.pattern(0, b"ad", &[0, 1, 0]).unwrap();
    assert_eq!(original.positions(0, b"ad", 1, 1), 2);
    assert_eq!(original.positions(0, b"ac", 1, 1), 0);
}

#[test]
fn hyphenation_state_is_scoped_but_tables_are_global_and_snapshots_restore_both() {
    let text = format!(
        r"{BOOT}\count0=1\relax
{{\language=7\lefthyphenmin=-5\righthyphenmin=63\patterns{{a1b}}\hyphenation{{a-bc}}
\message{{\the\language/\the\lefthyphenmin/\the\righthyphenmin}}\count0=2\relax}}
\message{{\the\language/\the\lefthyphenmin/\the\righthyphenmin}}\end"
    );
    let mut engine = Engine::new(text.as_bytes());
    while engine.outcome().counts[0] != 1 {
        engine.advance(1).unwrap();
    }
    let before = engine.checkpoint().unwrap();
    while engine.outcome().counts[0] != 2 {
        engine.advance(1).unwrap();
    }
    let inside = engine.checkpoint().unwrap();
    for _ in 0..2 {
        engine.advance(usize::MAX).unwrap();
        assert_eq!(
            engine.outcome().message_bytes(),
            [b"7/-5/63".to_vec(), b"0/0/0".to_vec()]
        );
        assert_eq!(engine.document.hyphenation.positions(7, b"abc", 1, 1), 2);
        engine.restore(&inside).unwrap();
    }
    engine.restore(&before).unwrap();
    assert!(engine.document.hyphenation.patterns.is_none());
    assert_eq!(engine.document.scalars[Target::LANGUAGE].value, 0);
    engine.advance(usize::MAX).unwrap();
    assert_eq!(engine.document.hyphenation.positions(7, b"abc", 1, 1), 2);
    drop(engine);
    drop(inside);
    drop(before);
}

#[test]
fn hyphenation_real_pass_boundary_freezes_and_guards_unimplemented_reconstruction() {
    for first_pass in [true, false] {
        let text = format!(
            r"{BOOT}\font\f=test \f\hsize100pt\vsize1000pt\parfillskip0pt plus1fil
\pretolerance={}\count0=1\relax\noindent ab\par\count0=2\relax\patterns{{a1b}}\end",
            if first_pass { 10000 } else { -1 }
        );
        let mut engine = Engine::new(text.as_bytes());
        engine
            .register_tfm(b"test", &font_test_data::basic())
            .unwrap();
        while engine.outcome().counts[0] != 1 {
            engine.advance(1).unwrap();
        }
        let before = engine.checkpoint().unwrap();
        while engine.outcome().counts[0] != 2 {
            engine.advance(1).unwrap();
        }
        assert_eq!(engine.document.hyphenation.frozen, !first_pass);
        let after = engine.checkpoint().unwrap();
        let result = engine.advance(usize::MAX);
        assert_eq!(
            result,
            if first_pass {
                Ok(Progress::Complete)
            } else {
                Err(Error::Syntax)
            }
        );
        engine.restore(&before).unwrap();
        assert!(!engine.document.hyphenation.frozen);
        engine.restore(&after).unwrap();
        assert_eq!(engine.document.hyphenation.frozen, !first_pass);
    }
    for load in [r"\patterns{ab}", r"\hyphenation{ab}"] {
        let text =
            format!(r"{BOOT}{load}\font\f=test \f\hsize100pt\pretolerance=-1\noindent ab\par\end");
        let mut engine = Engine::new(text.as_bytes());
        engine
            .register_tfm(b"test", &font_test_data::basic())
            .unwrap();
        assert_eq!(engine.advance(usize::MAX), Err(Error::Unsupported));
        assert!(engine.document.hyphenation.frozen);
        assert!(!engine.can_checkpoint());
    }
    let mut tables = Hyphenation::default();
    tables.begin_pass().unwrap();
    tables.exception(0, b"abc", 2).unwrap();
    assert_eq!(tables.positions(0, b"abc", 1, 1), 2);
    assert_eq!(tables.check_patterns(), Err(Error::Syntax));
}
