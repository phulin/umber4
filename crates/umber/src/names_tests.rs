use super::{Names, SpellingStore};
use crate::{Engine, Error, Progress, token::Meaning};

fn retained_state(engine: &Engine<'_>) -> (usize, usize, usize, usize, usize) {
    let names = &engine.names;
    (
        names.len(),
        names.buckets.len(),
        names.bytes.chunks.len(),
        names.bytes.available,
        engine.document.bindings.len(),
    )
}

#[test]
fn lookup_shares_collision_probing_without_inserting_missing_names() {
    let mut names = Names::new();
    let left = names.intern(b"collisiongnzbie").unwrap();
    let right = names.intern(b"collisionsbsdql").unwrap();
    assert_ne!(left, right);
    assert_eq!(names.lookup(b"collisiongnzbie"), Some(left));
    assert_eq!(names.lookup(b"collisionsbsdql"), Some(right));
    let count = names.len();
    assert_eq!(names.lookup(b"notpresent"), None);
    assert_eq!(names.len(), count);
    assert_eq!(names.lookup(b""), Some(crate::token::NULL_CS));
    assert_eq!(
        names.lookup(b"q"),
        Some(crate::token::SINGLE_BASE + u32::from(b'q'))
    );
}

#[test]
fn negative_name_queries_do_not_retain_symbols_or_spellings_across_restore() {
    // These spellings occur only as character input, never as control words.
    for name in [
        "unseenquery".to_owned(),
        "longquery".repeat(20),
        String::new(),
        "q".to_owned(),
    ] {
        let source =
            format!(r"\ifcsname {name}\endcsname\count0=99\relax\else\count0=7\relax\fi\end");
        let mut engine = Engine::new(source.as_bytes());
        let before = retained_state(&engine);
        let initial = engine.checkpoint().unwrap();
        for _ in 0..3 {
            assert_eq!(engine.advance(usize::MAX), Ok(Progress::Complete));
            assert_eq!(engine.outcome().counts[0], 7);
            assert_eq!(retained_state(&engine), before);
            engine.restore(&initial).unwrap();
            assert_eq!(retained_state(&engine), before);
        }
        if name.len() > 64 {
            assert_eq!(engine.name_spills.len(), 1);
            assert!(engine.name_spills[0].is_empty());
        }
    }
}

#[test]
fn existing_undefined_names_remain_undefined_after_conditional_lookup() {
    let mut engine = Engine::new(
        br"\ifcsname residentundefined\endcsname\count0=99\relax\else\count0=7\relax\fi\end",
    );
    let id = engine.intern(b"residentundefined").unwrap();
    let before = retained_state(&engine);
    engine.run_input().unwrap();
    assert_eq!(engine.outcome().counts[0], 7);
    assert_eq!(engine.names.lookup(b"residentundefined"), Some(id));
    assert_eq!(
        engine.document.bindings[id as usize].meaning(),
        Meaning::UNDEFINED
    );
    assert_eq!(retained_state(&engine), before);
}

#[test]
fn conditional_name_expansion_keeps_legitimate_creation_side_effects() {
    let source = br"\escapechar=-1\relax
\ifcsname\expandafter\string\csname madebyexpansion\endcsname suffix\endcsname
\count0=99\relax\else\count0=7\relax\fi
\ifcsname madebyexpansion\endcsname\count1=1\relax\fi\end";
    let mut engine = Engine::new(source);
    let before = engine.names.len();
    let initial = engine.checkpoint().unwrap();
    for _ in 0..3 {
        engine.run_input().unwrap();
        assert_eq!(engine.outcome().counts[..2], [7, 1]);
        assert_eq!(engine.names.len(), before + 1);
        assert_eq!(engine.document.bindings.len(), before + 1);
        assert_eq!(engine.names.lookup(b"madebyexpansionsuffix"), None);
        let id = engine.names.lookup(b"madebyexpansion").unwrap();
        assert_eq!(
            engine.document.bindings[id as usize].meaning(),
            Meaning::RELAX
        );
        engine.restore(&initial).unwrap();
        assert_eq!(
            engine.document.bindings[id as usize].meaning(),
            Meaning::UNDEFINED
        );
    }
}

#[test]
fn ordinary_token_reads_and_csname_still_create_names() {
    let mut engine = Engine::new(br"\ordinaryfresh");
    let count = engine.names.len();
    let read = engine.next_token().unwrap();
    assert_eq!(read.meaning, Meaning::UNDEFINED);
    assert_eq!(
        engine.names.lookup(b"ordinaryfresh"),
        Some(read.token.id() as u32)
    );
    assert_eq!(engine.names.len(), count + 1);

    let mut engine = Engine::new(br"\csname freshrelax\endcsname\end");
    let initial = engine.checkpoint().unwrap();
    assert_eq!(engine.advance(1), Ok(Progress::Boundary));
    let id = engine.names.lookup(b"freshrelax").unwrap();
    assert_eq!(
        engine.document.bindings[id as usize].meaning(),
        Meaning::RELAX
    );
    engine.restore(&initial).unwrap();
    assert_eq!(
        engine.document.bindings[id as usize].meaning(),
        Meaning::UNDEFINED
    );
    engine.run_input().unwrap();
    assert_eq!(engine.names.lookup(b"freshrelax"), Some(id));
    assert_eq!(
        engine.document.bindings[id as usize].meaning(),
        Meaning::RELAX
    );
}

#[test]
fn malformed_conditional_names_preserve_error_order_and_return_scratch() {
    let pending = "pending".repeat(12);
    for (suffix, error, token_name) in [
        (r"\relax\endcsname", Error::Syntax, None),
        (
            r"\unknownoperand\endcsname",
            Error::Undefined,
            Some(b"unknownoperand".as_slice()),
        ),
        (
            r"\noexpand\unknownoperand\endcsname",
            Error::Syntax,
            Some(b"unknownoperand".as_slice()),
        ),
        ("", Error::Eof, None),
    ] {
        let source = format!(r"\ifcsname {pending}{suffix}");
        let mut engine = Engine::new(source.as_bytes());
        let count = engine.names.len();
        assert_eq!(engine.run_input(), Err(error));
        assert_eq!(engine.names.lookup(pending.as_bytes()), None);
        assert_eq!(
            engine.names.len(),
            count + usize::from(token_name.is_some())
        );
        if let Some(name) = token_name {
            assert!(engine.names.lookup(name).is_some());
        }
        assert_eq!(engine.name_spills.len(), 1);
        assert!(engine.name_spills[0].is_empty());
        assert!(!engine.can_checkpoint());
    }
}

#[test]
fn published_spellings_survive_suffix_writes_and_chunk_growth() {
    let mut store = SpellingStore::default();
    let empty = store.append(b"");
    // SAFETY: the empty descriptor uses a static empty slice.
    assert!(unsafe { empty.as_ref() }.is_empty());
    let first = store.append(b"retained");
    // SAFETY: first is initialized and immutable. store owns its chunk until
    // after this view's last read; appends initialize only disjoint cells.
    let retained = unsafe { first.as_ref() };
    store.append(b"neighbor");
    assert_eq!(retained, b"retained");
    let mut spellings = Vec::new();
    for (index, length) in [4080, 4096, 4097, 8193].into_iter().enumerate() {
        let expected = vec![index as u8; length];
        let spelling = store.append(&expected);
        spellings.push((spelling, expected));
    }
    for index in 0..12 {
        store.append(&vec![index; 4096]);
    }
    assert_eq!(retained, b"retained");
    for (spelling, expected) in spellings {
        // SAFETY: directory growth preserves every initialized boxed chunk,
        // and the store remains alive through each descriptor's last read.
        assert_eq!(unsafe { spelling.as_ref() }, expected);
    }
}
