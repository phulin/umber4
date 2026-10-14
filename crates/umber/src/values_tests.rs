use super::*;

fn value(values: &mut Values, count: usize, byte: u8) -> Owner {
    let mut builder = values.builder().unwrap();
    for _ in 0..count {
        builder.push(Token::other(byte)).unwrap();
    }
    builder.finish(None)
}

#[test]
fn builder_reads_and_publication_cover_only_initialized_words() {
    let mut values = Values::default();
    let mut builder = values.builder().unwrap();
    for length in [1, 63, 64, 65, 4095, 4096, 4097] {
        while builder.len() < length {
            builder.push(Token::other(builder.len() as u8)).unwrap();
        }
        assert_eq!(builder.len(), length);
        assert_eq!(builder.get(length - 1), Token::other((length - 1) as u8));
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| builder.get(length))).is_err()
        );
    }
    let owner = builder.finish(None);
    assert_eq!(owner.tokens.len, 4097);
    assert_eq!(owner.tokens.get(4096), Token::other(0));
}

#[test]
fn builder_id_is_stable_and_unpublished_until_finish() {
    let mut values = Values::default();
    let mut builder = values.builder().unwrap();
    let id = builder.id();
    builder.push(Token::other(b'A')).unwrap();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| values.get(id))).is_err());
    let owner = builder.finish(None);
    assert_eq!(owner.id, id);
    assert_eq!(values.get(id).tokens.get(0), Token::other(b'A'));
}

#[test]
fn last_owner_reuses_header_and_initialized_bump_tail() {
    let mut values = Values::default();
    let first = value(&mut values, 32, b'A');
    let id = first.id;
    let header = Rc::as_ptr(&first.0);
    let (cells, _, _) = first.tokens.window(0, 32);
    drop(first);
    assert_eq!(values.pool.active.borrow().as_ref().unwrap().1, 0);
    let next = value(&mut values, 32, b'B');
    assert_eq!(next.id, id);
    assert_eq!(Rc::as_ptr(&next.0), header);
    assert_eq!(next.tokens.window(0, 32).0, cells);
    assert_eq!(next.tokens.get(31), Token::other(b'B'));
}

#[test]
fn last_owner_preserves_intervening_reservations() {
    let mut values = Values::default();
    let first = value(&mut values, 32, b'A');
    let second = value(&mut values, 32, b'B');
    drop(first);
    assert_eq!(values.pool.active.borrow().as_ref().unwrap().1, 64);
    let third = value(&mut values, 32, b'C');
    assert_eq!(second.tokens.get(31), Token::other(b'B'));
    drop(third);
    assert_eq!(values.pool.active.borrow().as_ref().unwrap().1, 64);
    drop(second);
    assert_eq!(values.pool.active.borrow().as_ref().unwrap().1, 32);
}

#[test]
fn owners_and_unpublished_builders_survive_registry_drop() {
    let mut values = Values::default();
    let original = value(&mut values, 64, b'A');
    let alias = original.clone();
    let mut builder = values.builder().unwrap();
    builder.push(Token::other(b'B')).unwrap();
    drop(values);
    drop(original);
    assert_eq!(alias.tokens.get(63), Token::other(b'A'));
    let completed = builder.finish(None);
    assert_eq!(completed.tokens.get(0), Token::other(b'B'));
    drop(alias);
    assert_eq!(completed.tokens.get(0), Token::other(b'B'));
}

#[test]
fn published_windows_survive_interleaved_builders_and_reuse() {
    let mut values = Values::default();
    let held = value(&mut values, 127, b'A');
    let (first, end, _) = held.tokens.window(0, 127);
    let mut a = values.builder().unwrap();
    let mut b = values.builder().unwrap();
    for index in 0..8200 {
        a.push(Token::other(index as u8)).unwrap();
        b.push(Token::other((index + 3) as u8)).unwrap();
    }
    let a = a.finish(None);
    let b = b.finish(None);
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
    let alias = original.clone();
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
    let mut abandoned = values.builder().unwrap();
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
    let mut first = values.builder().unwrap();
    first.push(Token::other(b'A')).unwrap();
    let mut second = values.builder().unwrap();
    second.push(Token::other(b'B')).unwrap();
    let first = first.finish(None);
    assert_eq!(values.pool.active.borrow().as_ref().unwrap().1, 128);
    let second = second.finish(None);
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
    let mut values = Values::default();
    let mut builder = values.builder().unwrap();
    // Exercise the final reservation without allocating the preceding words.
    builder.start = u32::MAX - 1;
    builder.push(Token::other(b'Z')).unwrap();
    let piece = builder.tokens.first.as_ref().unwrap();
    assert_eq!(piece.start, u32::MAX - 1);
    assert_eq!(piece.capacity, 1);
    assert_eq!(piece.start.checked_add(piece.capacity), Some(u32::MAX));
    assert_eq!(builder.len(), u32::MAX);
    assert_eq!(builder.push(Token::other(b'X')), Err(Error::Capacity));
}
