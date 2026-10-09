use super::*;

fn value(values: &mut Values, count: usize, byte: u8) -> Owner {
    let mut builder = values.builder();
    for _ in 0..count {
        builder.push(Token::other(byte)).unwrap();
    }
    values.publish(builder, None).unwrap()
}

#[test]
fn published_windows_survive_interleaved_builders_and_reuse() {
    let mut values = Values::default();
    let held = value(&mut values, 127, b'A');
    let (first, end, _) = held.tokens.window(0, 127);
    let mut a = values.builder();
    let mut b = values.builder();
    for index in 0..8200 {
        a.push(Token::other(index as u8)).unwrap();
        b.push(Token::other((index + 3) as u8)).unwrap();
    }
    let a = values.publish(a, None).unwrap();
    let b = values.publish(b, None).unwrap();
    for index in 0..8200 {
        assert_eq!(a.tokens.get(index), Token::other(index as u8));
        assert_eq!(b.tokens.get(index), Token::other((index + 3) as u8));
    }
    drop(a);
    drop(b);
    for _ in 0..32 {
        drop(value(&mut values, 9000, b'Z'));
    }
    // SAFETY: held owns this immutable window through all neighboring writes
    // and recycling of pages whose values have been released.
    unsafe {
        assert_eq!(end.offset_from(first), 127);
        assert!(
            std::slice::from_raw_parts(first, 127)
                .iter()
                .all(|t| *t == Token::other(b'A'))
        );
    }
}

#[test]
fn slots_recycle_after_last_owner_and_page_cache_is_bounded() {
    let mut values = Values::default();
    let original = value(&mut values, 10, b'A');
    let alias = Rc::clone(&original);
    let id = original.id;
    drop(original);
    let other = value(&mut values, 10, b'B');
    assert_ne!(other.id, id);
    drop(alias);
    let replacement = value(&mut values, 10, b'C');
    assert_eq!(replacement.id, id);
    assert_eq!(other.tokens.get(0), Token::other(b'B'));
    drop(replacement);
    drop(other);
    drop(value(&mut values, WORDS * (CACHE_CHUNKS + 4), b'X'));
    assert!(values.pool.free.borrow().len() <= CACHE_CHUNKS);
    for _ in 0..100 {
        drop(value(&mut values, 20, b'Y'));
    }
    assert!(values.slots.len() <= 2);
}

#[test]
fn failed_builders_release_pages_and_owners_outlive_registry() {
    let mut values = Values::default();
    let mut abandoned = values.builder();
    for _ in 0..WORDS * 3 {
        abandoned.push(Token::other(b'X')).unwrap();
    }
    drop(abandoned);
    assert!(!values.pool.free.borrow().is_empty());
    let held = value(&mut values, 5000, b'K');
    drop(values);
    assert_eq!(held.tokens.get(4999), Token::other(b'K'));
}

#[test]
fn publication_returns_only_the_unused_bump_tail() {
    let mut values = Values::default();
    let mut first = values.builder();
    first.push(Token::other(b'A')).unwrap();
    let mut second = values.builder();
    second.push(Token::other(b'B')).unwrap();
    let first = values.publish(first, None).unwrap();
    assert_eq!(values.pool.active.borrow().as_ref().unwrap().1, 128);
    let second = values.publish(second, None).unwrap();
    assert_eq!(values.pool.active.borrow().as_ref().unwrap().1, 65);
    let next = value(&mut values, 1, b'C');
    assert_eq!(values.pool.active.borrow().as_ref().unwrap().1, 66);
    let (b, _, _) = second.tokens.window(0, 1);
    let (c, _, _) = next.tokens.window(0, 1);
    assert_eq!(b.wrapping_add(1), c);
    assert_eq!(first.tokens.get(0), Token::other(b'A'));
    assert_eq!(second.tokens.get(0), Token::other(b'B'));
    assert_eq!(next.tokens.get(0), Token::other(b'C'));
}

#[test]
fn reservation_stays_inside_value_coordinates() {
    let values = Values::default();
    let mut builder = values.builder();
    // Exercise the final reservation without allocating the preceding words.
    builder.tokens.len = u32::MAX - 1;
    builder.push(Token::other(b'Z')).unwrap();
    let piece = builder.tokens.first.as_ref().unwrap();
    assert_eq!(piece.start, u32::MAX - 1);
    assert_eq!(piece.capacity, 1);
    assert_eq!(piece.start.checked_add(piece.capacity), Some(u32::MAX));
    assert_eq!(builder.tokens.len, u32::MAX);
    assert_eq!(builder.push(Token::other(b'X')), Err(Error::Capacity));
}
