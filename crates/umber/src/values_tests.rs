use super::*;

fn value(values: &mut Values, count: usize, byte: u8) -> Owner {
    class_value(values, Class::Transient, count, byte)
}

fn class_value(values: &mut Values, class: Class, count: usize, byte: u8) -> Owner {
    let mut builder = values.builder(class).unwrap();
    for _ in 0..count {
        builder.push(Token::other(byte)).unwrap();
    }
    builder.finish()
}

#[test]
fn builder_reads_and_publication_cover_only_initialized_words() {
    let mut values = Values::default();
    let mut builder = values.builder(Class::Transient).unwrap();
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
    let owner = builder.finish();
    assert_eq!(owner.tokens.len, 4097);
    assert_eq!(owner.tokens.get(4096), Token::other(0));
}

#[test]
fn builder_id_is_stable_and_unpublished_until_finish() {
    let mut values = Values::default();
    let mut builder = values.builder(Class::Transient).unwrap();
    let id = builder.id();
    builder.push(Token::other(b'A')).unwrap();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| values.get(id))).is_err());
    let owner = builder.finish();
    assert_eq!(owner.id, id);
    assert_eq!(values.get(id).tokens.get(0), Token::other(b'A'));
}

#[test]
fn last_owner_reuses_header_and_initialized_bump_tail() {
    let mut values = Values::default();
    let first = value(&mut values, 32, b'A');
    let id = first.id;
    let header = std::ptr::from_ref(first.slot());
    let (cells, _, _) = first.tokens.window(0, 32);
    drop(first);
    assert_eq!(
        values.pool.active[Class::Transient as usize]
            .borrow()
            .as_ref()
            .unwrap()
            .1,
        0
    );
    let next = value(&mut values, 32, b'B');
    assert_eq!(next.id, id);
    assert_eq!(std::ptr::from_ref(next.slot()), header);
    assert_eq!(next.tokens.window(0, 32).0, cells);
    assert_eq!(next.tokens.get(31), Token::other(b'B'));
}

#[test]
fn last_owner_preserves_intervening_reservations() {
    let mut values = Values::default();
    let first = value(&mut values, 32, b'A');
    let second = value(&mut values, 32, b'B');
    drop(first);
    assert_eq!(
        values.pool.active[Class::Transient as usize]
            .borrow()
            .as_ref()
            .unwrap()
            .1,
        64
    );
    let third = value(&mut values, 32, b'C');
    assert_eq!(second.tokens.get(31), Token::other(b'B'));
    drop(third);
    assert_eq!(
        values.pool.active[Class::Transient as usize]
            .borrow()
            .as_ref()
            .unwrap()
            .1,
        64
    );
    drop(second);
    assert_eq!(
        values.pool.active[Class::Transient as usize]
            .borrow()
            .as_ref()
            .unwrap()
            .1,
        32
    );
}

#[test]
fn owners_and_unpublished_builders_survive_registry_drop() {
    let mut values = Values::default();
    let original = value(&mut values, 64, b'A');
    let alias = original.clone();
    let mut builder = values.builder(Class::Transient).unwrap();
    builder.push(Token::other(b'B')).unwrap();
    drop(values);
    drop(original);
    assert_eq!(alias.tokens.get(63), Token::other(b'A'));
    let completed = builder.finish();
    assert_eq!(completed.tokens.get(0), Token::other(b'B'));
    drop(alias);
    assert_eq!(completed.tokens.get(0), Token::other(b'B'));
}

#[test]
fn published_windows_survive_interleaved_builders_and_reuse() {
    let mut values = Values::default();
    let held = value(&mut values, 127, b'A');
    let (first, end, _) = held.tokens.window(0, 127);
    let mut a = values.builder(Class::Transient).unwrap();
    let mut b = values.builder(Class::Transient).unwrap();
    for index in 0..8200 {
        a.push(Token::other(index as u8)).unwrap();
        b.push(Token::other((index + 3) as u8)).unwrap();
    }
    let a = a.finish();
    let b = b.finish();
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
    assert!(values.len <= 2);
}

#[test]
fn failed_builders_release_pages_and_owners_outlive_registry() {
    let mut values = Values::default();
    let mut abandoned = values.builder(Class::Transient).unwrap();
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
    let mut first = values.builder(Class::Transient).unwrap();
    first.push(Token::other(b'A')).unwrap();
    let mut second = values.builder(Class::Transient).unwrap();
    second.push(Token::other(b'B')).unwrap();
    let first = first.finish();
    assert_eq!(
        values.pool.active[Class::Transient as usize]
            .borrow()
            .as_ref()
            .unwrap()
            .1,
        128
    );
    let second = second.finish();
    assert_eq!(
        values.pool.active[Class::Transient as usize]
            .borrow()
            .as_ref()
            .unwrap()
            .1,
        65
    );
    let next = value(&mut values, 1, b'C');
    assert_eq!(
        values.pool.active[Class::Transient as usize]
            .borrow()
            .as_ref()
            .unwrap()
            .1,
        66
    );
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
    let mut builder = values.builder(Class::Transient).unwrap();
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

#[test]
fn separate_tails_rewind_without_touching_the_other_class() {
    let mut values = Values::default();
    let durable = class_value(&mut values, Class::Durable, 32, b'D');
    let transient = class_value(&mut values, Class::Transient, 32, b'T');
    assert!(!Rc::ptr_eq(
        &durable.tokens.first.as_ref().unwrap().page.0,
        &transient.tokens.first.as_ref().unwrap().page.0,
    ));
    let alias = transient.clone();
    drop(transient);
    let durable_tail = class_value(&mut values, Class::Durable, 32, b'E');
    assert_eq!(
        values.pool.active[Class::Durable as usize]
            .borrow()
            .as_ref()
            .unwrap()
            .1,
        64
    );
    drop(alias);
    assert_eq!(
        values.pool.active[Class::Transient as usize]
            .borrow()
            .as_ref()
            .unwrap()
            .1,
        0
    );
    assert_eq!(durable.tokens.get(31), Token::other(b'D'));
    assert_eq!(durable_tail.tokens.get(31), Token::other(b'E'));
}

#[test]
fn cached_page_records_transfer_classes_only_after_the_last_owner() {
    let mut values = Values::default();
    let original = class_value(&mut values, Class::Transient, WORDS, b'A');
    let page = Rc::downgrade(&original.tokens.first.as_ref().unwrap().page.0);
    // Moving the active tail leaves original as the page's only checked-out owner.
    let next = class_value(&mut values, Class::Transient, 1, b'B');
    assert!(values.pool.free.borrow().is_empty());
    assert_eq!(page.strong_count(), 1);
    let alias = original.clone();
    drop(original);
    assert!(values.pool.free.borrow().is_empty());
    assert_eq!(alias.tokens.get(WORDS as u32 - 1), Token::other(b'A'));
    drop(alias);
    assert_eq!(values.pool.free.borrow().len(), 1);
    assert_eq!(page.strong_count(), 1); // the cache, including the reused counter
    let durable = class_value(&mut values, Class::Durable, 32, b'D');
    assert_eq!(values.pool.created.get(), 2);
    assert_eq!(values.pool.free.borrow().len(), 0);
    assert!(std::ptr::eq(
        page.as_ptr(),
        Rc::as_ptr(&durable.tokens.first.as_ref().unwrap().page.0)
    ));
    assert_eq!(
        durable.tokens.first.as_ref().unwrap().page.class.get(),
        Class::Durable
    );
    assert_eq!(next.tokens.get(0), Token::other(b'B'));
    drop(values);
    assert_eq!(durable.tokens.get(31), Token::other(b'D'));
    drop(durable);
    assert_eq!(page.strong_count(), 0);
}

#[test]
fn warm_mixed_survivors_reuse_complete_page_records() {
    let mut values = Values::default();
    let survivor = class_value(&mut values, Class::Durable, 97, b'S');
    let mut durable = class_value(&mut values, Class::Durable, 1000, b'D');
    let mut transient = class_value(&mut values, Class::Transient, 1000, b'T');
    let mut warm = None;
    for iteration in 0..128 {
        durable = class_value(&mut values, Class::Durable, 1000, b'D');
        transient = class_value(&mut values, Class::Transient, 1000, b'T');
        if iteration == 15 {
            warm = Some((values.pool.created.get(), values.len));
        }
    }
    assert_eq!(warm.unwrap(), (values.pool.created.get(), values.len));
    assert!(values.pool.created.get() <= 6);
    assert!(values.pool.free.borrow().len() <= CACHE_CHUNKS);
    assert_eq!(survivor.tokens.get(96), Token::other(b'S'));
    assert_eq!(durable.tokens.get(999), Token::other(b'D'));
    assert_eq!(transient.tokens.get(999), Token::other(b'T'));
}

#[test]
fn both_classes_keep_unpublished_and_published_owners_after_teardown() {
    let mut values = Values::default();
    let durable = class_value(&mut values, Class::Durable, 97, b'D');
    let transient = class_value(&mut values, Class::Transient, 97, b'T');
    let mut building = values.builder(Class::Durable).unwrap();
    building.push(Token::other(b'B')).unwrap();
    let mut unfinished = values.builder(Class::Transient).unwrap();
    unfinished.push(Token::other(b'U')).unwrap();
    drop(values);
    drop(unfinished);
    let completed = building.finish();
    drop(transient);
    assert_eq!(durable.tokens.get(96), Token::other(b'D'));
    assert_eq!(completed.tokens.get(0), Token::other(b'B'));
}

#[test]
fn engine_destinations_choose_classes_without_changing_escaped_tokens() {
    let source = br"\catcode123=1 \catcode125=2 \catcode35=6\relax
\def\save#1{\edef\saved{#1}\toks0={#1}}
\save{escaped argument}\def\wanted{escaped argument}
\message{\ifx\saved\wanted YES\else NO\fi}
\def\name{letter}\message{\string\name}\end";
    let mut engine = crate::Engine::new(source);
    engine.run_input().unwrap();
    assert_eq!(engine.messages, [b"YES".as_slice(), b"\\name"]);
    for owner in engine.binding_owners.iter().flatten().chain(
        engine
            .token_registers
            .iter()
            .filter_map(|register| register.owner.as_ref()),
    ) {
        assert_eq!(
            owner.tokens.first.as_ref().unwrap().page.class.get(),
            Class::Durable
        );
    }
    assert!(
        engine.values.pool.active[Class::Transient as usize]
            .borrow()
            .is_some()
    );
}

#[test]
fn bounded_cache_releases_excess_records_and_has_no_teardown_cycle() {
    let mut values = Values::default();
    let owner = value(&mut values, WORDS * (CACHE_CHUNKS + 4), b'X');
    let pages: Vec<_> = owner
        .tokens
        .first
        .iter()
        .chain(&owner.tokens.rest)
        .map(|piece| Rc::downgrade(&piece.page.0))
        .collect();
    assert_eq!(pages.len(), CACHE_CHUNKS + 4);
    drop(owner);
    assert_eq!(values.pool.free.borrow().len(), CACHE_CHUNKS);
    // One checked-out active tail survives alongside the bounded empty cache.
    assert_eq!(
        pages.iter().filter(|page| page.strong_count() != 0).count(),
        CACHE_CHUNKS + 1
    );
    drop(values);
    assert!(pages.iter().all(|page| page.strong_count() == 0));
}

#[test]
fn multiblock_metadata_reuses_capacity_without_retaining_old_pages() {
    let mut values = Values::default();
    let survivor = class_value(&mut values, Class::Durable, 97, b'S');
    let first = value(&mut values, WORDS * 3, b'A');
    let id = first.id;
    let descriptors = first.tokens.rest.as_ptr();
    let capacity = first.tokens.rest.capacity();
    drop(first);
    // SAFETY: the registered free slot is unpublished and has no external owner.
    let cleared = unsafe { &*values.slot(id).value.get() };
    assert!(cleared.tokens.first.is_none());
    assert!(cleared.tokens.rest.is_empty());
    assert_eq!(cleared.tokens.rest.capacity(), capacity);
    for _ in 0..16 {
        let next = value(&mut values, WORDS * 3, b'B');
        assert_eq!(next.id, id);
        assert_eq!(next.tokens.rest.as_ptr(), descriptors);
        assert_eq!(next.tokens.get((WORDS * 3 - 1) as u32), Token::other(b'B'));
    }
    assert_eq!(survivor.tokens.get(96), Token::other(b'S'));
}

#[test]
fn failed_builder_returns_metadata_and_can_outlive_registry() {
    let mut values = Values::default();
    let mut failed = values.builder(Class::Durable).unwrap();
    failed.extend(&[Token::other(b'X'); WORDS * 2]).unwrap();
    failed.definition_mut().parameters.push(Parameter {
        delimiter: ArenaRange { start: 0, end: 3 },
        failure_start: 0,
    });
    failed.definition_mut().failure.extend([0, 0, 1]);
    let id = failed.id();
    let pieces = failed.tokens.rest.as_ptr();
    let parameters = failed.definition_mut().parameters.as_ptr();
    let failures = failed.definition_mut().failure.as_ptr();
    drop(failed);
    let mut next = values.builder(Class::Durable).unwrap();
    assert_eq!(next.id(), id);
    assert!(next.tokens.first.is_none());
    assert!(next.tokens.rest.is_empty());
    assert!(next.definition_mut().parameters.is_empty());
    assert!(next.definition_mut().failure.is_empty());
    assert_eq!(next.tokens.rest.as_ptr(), pieces);
    assert_eq!(next.definition_mut().parameters.as_ptr(), parameters);
    assert_eq!(next.definition_mut().failure.as_ptr(), failures);
    next.push(Token::other(b'Y')).unwrap();
    next.definition_mut().parameters.push(Parameter {
        delimiter: ArenaRange { start: 0, end: 1 },
        failure_start: 0,
    });
    next.definition_mut().failure.push(0);
    drop(values);
    let owner = next.finish();
    let alias = owner.clone();
    assert_eq!(owner.tokens.get(0), Token::other(b'Y'));
    drop(owner);
    assert_eq!(alias.definition().parameters[0].delimiter.end, 1);
    assert_eq!(alias.definition().failure, [0]);
    assert_eq!(alias.tokens.get(0), Token::other(b'Y'));
}

#[test]
fn alternating_definition_metadata_reaches_a_fixed_capacity() {
    let mut previous = None;
    for iterations in [20, 200] {
        let source = format!(
            r"\catcode123=1 \catcode125=2 \catcode35=6\relax
\def\step{{
\def\actual##1abab##2xyxy##3##4##5##6##7##8##9{{##9##1##2}}
\edef\result{{\actual aababBxxyxyCDEFGHI}}
\def\wanted{{IaBx}}\ifx\result\wanted\else\undefined\fi
\def\actual##1aba{{##1}}\edef\result{{\actual aaba}}
\def\wanted{{a}}\ifx\result\wanted\else\undefined\fi
\def\actual{{zero}}\advance\count0by1\relax
\ifnum\count0<{iterations} \expandafter\step\fi}}
\step\end"
        );
        let mut engine = crate::Engine::new(source.as_bytes());
        engine.run_input().unwrap();
        let capacities: Vec<_> = engine
            .values
            .slots()
            .map(|slot| {
                // SAFETY: this test reads initialized slots with no engine mutation.
                let value = unsafe { &*slot.value.get() };
                (
                    value.definition.parameters.capacity(),
                    value.definition.failure.capacity(),
                )
            })
            .collect();
        if let Some(previous) = &previous {
            assert_eq!(&capacities, previous);
        }
        previous = Some(capacities);
        assert_eq!(engine.scalars.counts()[0].value, iterations);
    }
}

#[test]
fn cross_slab_owners_keep_windows_stable_during_reuse_and_teardown() {
    let mut values = Values::default();
    let mut owners: Vec<_> = (0..193)
        .map(|index| value(&mut values, 3, index as u8))
        .collect();
    let survivor = owners.swap_remove(64);
    let window = survivor.tokens.window(0, 3).0;
    let survivor_id = survivor.id;
    drop(owners);
    for index in 0..193 {
        let next = value(&mut values, 3, index as u8);
        assert_ne!(next.id, survivor_id);
        assert_eq!(survivor.tokens.window(0, 3).0, window);
    }
    let mut unfinished = values.builder(Class::Durable).unwrap();
    unfinished.extend(&[Token::other(b'X'); 5]).unwrap();
    let mut failed = values.builder(Class::Transient).unwrap();
    failed.push(Token::other(b'F')).unwrap();
    let alias = survivor.clone();
    drop(values);
    drop(failed);
    drop(survivor);
    // SAFETY: alias still retains this immutable initialized window after every
    // other registry slab and all neighboring occupied slots have been released.
    assert_eq!(unsafe { *window }, Token::other(64));
    let completed = unfinished.finish();
    drop(alias);
    assert_eq!(completed.tokens.get(4), Token::other(b'X'));
}
