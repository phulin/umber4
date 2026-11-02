use super::*;
use crate::{Engine, Progress};
fn bytes(h: &Hyphenation) -> Vec<u8> {
    let mut w = Writer::default();
    w.bytes(&[0; 24]);
    write(h, &mut w).unwrap();
    w.finish().unwrap()[24..].to_vec()
}
fn decode(bytes: &[u8]) -> Result<Hyphenation> {
    let mut r = Reader::new(bytes)?;
    let h = read(&mut r)?;
    r.finish()?;
    Ok(h)
}
fn fixture() -> Hyphenation {
    let mut h = Hyphenation::default();
    h.pattern(0, b"ab", &[0, 1, 0]).unwrap();
    h.pattern(0, b"abc", &[0, 0, 2, 0]).unwrap();
    h.pattern(255, b"ab", &[0, 3, 0]).unwrap();
    h.exception(0, b"abcd", 1 << 2).unwrap();
    h.exception(255, b"abcd", 1 << 1).unwrap();
    h
}
#[test]
fn compiled_hyphenation_roundtrip_freeze_exception_cow_and_long_boundaries() {
    let mut original = fixture();
    let word = [b'a'; 63];
    let mut weights = [0; 64];
    weights[31] = 5;
    original.pattern(255, &word, &weights).unwrap();
    let image = bytes(&original);
    let mut h = decode(&image).unwrap();
    drop(image);
    for language in [0, 1, 255] {
        for word in [b"abc".as_slice(), b"abcd", word.as_slice()] {
            assert_eq!(
                h.positions(language, word, 1, 1),
                original.positions(language, word, 1, 1)
            );
        }
    }
    assert_eq!(h.check_patterns(), Err(Error::Syntax));
    let saved = h.clone();
    h.exception(0, b"abcd", 1 << 3).unwrap();
    assert_eq!(saved.positions(0, b"abcd", 1, 1), 1 << 2);
    assert_eq!(h.positions(0, b"abcd", 1, 1), 1 << 3);
    assert!(Rc::ptr_eq(
        h.patterns.as_ref().unwrap(),
        saved.patterns.as_ref().unwrap()
    ));
    assert!(!Rc::ptr_eq(
        h.exceptions.as_ref().unwrap(),
        saved.exceptions.as_ref().unwrap()
    ));
    let mut again = decode(&bytes(&h)).unwrap();
    again.exception(255, b"abcd", 0).unwrap();
    assert_eq!(saved.positions(255, b"abcd", 1, 1), 1 << 1);
    assert_eq!(again.positions(255, b"abcd", 1, 1), 0);
    // Buckets are derived, and can grow as ordinary document exceptions accrue.
    for c in b'a'..=b'z' {
        again.exception(1, &[c, b'x'], 1 << 1).unwrap();
    }
    assert_eq!(again.positions(1, b"zx", 1, 1), 2);
}
#[test]
fn loaded_image_freezes_empty_and_nonempty_patterns_but_accepts_new_exceptions() {
    for body in ["", r"\patterns{a1b}\language=255\hyphenation{ab-c}"] {
        let text = format!(r"\catcode123=1\catcode125=2 {body}\dump");
        let mut setup = Engine::new(text.as_bytes());
        setup.advance(usize::MAX).unwrap();
        let image = setup.encode_format().unwrap();
        let mut loaded = Engine::from_format(&image, br"\hyphenation{a-bc}\end").unwrap();
        let checkpoint = loaded.checkpoint().unwrap();
        for _ in 0..2 {
            assert_eq!(loaded.advance(usize::MAX), Ok(Progress::Complete));
            let language = loaded.document.scalars[crate::scalars::Target::LANGUAGE].value as u8;
            assert_eq!(
                loaded
                    .document
                    .hyphenation
                    .positions(language, b"abc", 1, 1),
                2
            );
            loaded.restore(&checkpoint).unwrap();
        }
        let mut loaded = Engine::from_format(&image, br"\patterns{c1d}\end").unwrap();
        assert_eq!(loaded.advance(usize::MAX), Err(Error::Syntax));
    }
    assert_eq!(bytes(&Hyphenation::default()), [0]);
    assert!(decode(&[0]).unwrap().frozen);
    assert!(matches!(decode(&[0, 0]), Err(Error::Format)));
}
#[test]
fn trie_wire_rejects_cycles_shared_unreachable_nodes_and_invalid_operations() {
    let h = fixture();
    let image = bytes(&h);
    for n in 0..image.len() {
        assert!(decode(&image[..n]).is_err(), "{n}");
    }
    for mutation in 0..8 {
        let mut bad = h.clone();
        let p = Rc::make_mut(bad.patterns.as_mut().unwrap());
        let language = p.nodes[0].child as usize;
        let letter = p.nodes[language].child as usize;
        match mutation {
            0 => p.nodes[0].child = u32::MAX,
            1 => p.nodes[language].next = language as u32,
            2 => p.nodes[letter].child = language as u32,
            3 => p.nodes[0].child = 0,
            4 => p.nodes[letter].operations = u32::MAX,
            5 => p.nodes[letter].count = 65,
            6 => p.operations[0].weight = 10,
            7 => p.operations[0].position = 63,
            _ => unreachable!(),
        }
        assert!(
            matches!(decode(&bytes(&bad)), Err(Error::Format)),
            "{mutation}"
        );
    }
    let mut bad = h.clone();
    let p = Rc::make_mut(bad.patterns.as_mut().unwrap());
    let id = p.nodes[0].child as usize;
    let next = p.nodes[id].next as usize;
    p.nodes[next].character = p.nodes[id].character;
    assert!(matches!(decode(&bytes(&bad)), Err(Error::Format)));
    // Language plus 64 character edges exceeds the supported 63-letter bound.
    let mut bad = Hyphenation::default();
    bad.pattern(0, &[b'a'; 64], &[0; 65]).unwrap();
    assert!(matches!(decode(&bytes(&bad)), Err(Error::Format)));
}
#[test]
fn exception_wire_rejects_lengths_duplicates_and_impossible_break_bits() {
    for word in [b"a".as_slice(), &[b'a'; 64], b"a\0"] {
        let mut w = Writer::default();
        w.bytes(&[0; 24]);
        w.u8(1);
        w.count(0).unwrap();
        w.count(0).unwrap();
        w.count(1).unwrap();
        w.u8(0);
        w.blob(word).unwrap();
        w.u64(2);
        assert!(matches!(
            decode(&w.finish().unwrap()[24..]),
            Err(Error::Format)
        ));
    }
    for (count, positions) in [(2, 2), (1, 1 << 63), (1, 1 << 4)] {
        let mut w = Writer::default();
        w.bytes(&[0; 24]);
        w.u8(1);
        w.count(0).unwrap();
        w.count(0).unwrap();
        w.count(count).unwrap();
        for _ in 0..count {
            w.u8(0);
            w.blob(b"abc").unwrap();
            w.u64(positions);
        }
        assert!(matches!(
            decode(&w.finish().unwrap()[24..]),
            Err(Error::Format)
        ));
    }
}
