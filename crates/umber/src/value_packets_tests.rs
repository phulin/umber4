use super::*;

#[test]
fn alternating_arities_retain_nested_span_capacity_and_distinct_live_packets() {
    let mut engine = crate::Engine::new(b"");
    let first = engine.take_packet(Arity::Nine).unwrap();
    let second = engine.take_packet(Arity::Nine).unwrap();
    let single = engine.take_packet(Arity::One).unwrap();
    assert_ne!(first, second);
    assert_ne!(single, first);
    for start in 0..8 {
        engine.packets[first as usize].arguments[8].append(Span {
            store: start,
            start,
            end: start + 1,
        });
    }
    let span_capacity = engine.packets[first as usize].arguments[8].rest.capacity();
    let span_backing = engine.packets[first as usize].arguments[8].rest.as_ptr();
    engine.release_packet(first);
    engine.release_packet(single);
    engine.release_packet(second);
    assert_eq!(engine.take_packet(Arity::Nine).unwrap(), second);
    assert_eq!(engine.take_packet(Arity::One).unwrap(), single);
    assert_eq!(engine.take_packet(Arity::Nine).unwrap(), first);
    let argument = &engine.packets[first as usize].arguments[8];
    assert!(argument.first.empty());
    assert!(argument.rest.is_empty());
    assert_eq!(argument.rest.capacity(), span_capacity);
    assert_eq!(argument.rest.as_ptr(), span_backing);
}

#[test]
fn packet_blocks_share_live_backing_and_recycle_cleared_sequences() {
    let values = Values::default();
    let mut packets: Vec<_> = (0..97)
        .map(|index| {
            let mut packet = values.packet();
            values.prepare_packet(&mut packet, 9);
            for argument in packet.arguments.iter_mut() {
                for offset in 0..7 {
                    argument.append(Span {
                        store: (index + offset) as u32,
                        start: offset as u32,
                        end: offset as u32 + 1,
                    });
                }
            }
            packet
        })
        .collect();
    assert!(values.pool.arguments.pages() < packets.len() / 2);
    assert!(values.pool.spans.pages() < packets.len());
    let argument_pages = values.pool.arguments.pages();
    let span_pages = values.pool.spans.pages();
    for packet in &mut packets {
        packet.clear();
        values.prepare_packet(packet, 1);
        values.prepare_packet(packet, 9);
        for argument in packet.arguments.iter_mut() {
            for offset in 0..7 {
                argument.append(Span {
                    store: offset,
                    start: offset,
                    end: offset + 1,
                });
            }
        }
    }
    assert_eq!(values.pool.arguments.pages(), argument_pages);
    assert_eq!(values.pool.spans.pages(), span_pages);
    let spans = Rc::downgrade(&values.pool.spans);
    drop(values);
    assert_eq!(packets[96].arguments[8].span(6).unwrap().store, 6);
    drop(packets);
    assert_eq!(spans.strong_count(), 0);
}

#[test]
fn packet_owners_and_spans_survive_registry_and_release_without_cycles() {
    let mut values = Values::default();
    let mut packet = values.packet();
    values.prepare_packet(&mut packet, 9);
    for byte in b"AB" {
        let mut builder = values.builder(Class::Durable).unwrap();
        builder.extend(&[Token::other(*byte); 16]).unwrap();
        packet.owners.push(builder.finish());
    }
    for index in 0..9 {
        packet.arguments[0].append(Span {
            store: packet.owners[index % 2].id,
            start: index as u32,
            end: index as u32 + 1,
        });
    }
    let first = packet.owners[0].tokens.window(0, 16).0;
    let page = Rc::downgrade(&packet.owners[0].tokens.first.as_ref().unwrap().page.0);
    let owners = Rc::downgrade(&values.pool.owners);
    values.prepare_packet(&mut packet, 1);
    drop(values);
    assert_eq!(packet.arguments[0].span(8).unwrap().start, 8);
    assert_eq!(packet.owners[1].tokens.get(15), Token::other(b'B'));
    // SAFETY: the retained packet owner keeps the first published token alive.
    assert_eq!(unsafe { *first }, Token::other(b'A'));
    packet.clear();
    assert_eq!(page.strong_count(), 0);
    assert!(packet.owners.is_empty());
    assert!(packet.arguments[0].first.empty());
    assert!(packet.arguments[0].rest.is_empty());
    drop(packet);
    assert_eq!(owners.strong_count(), 0);
}

#[test]
fn failed_capture_releases_partially_initialized_packet_on_engine_drop() {
    let source = br"\catcode123=1 \catcode125=2 \catcode35=6\relax
\def\fail#1#2{#1}\def\input{{held}\par}\expandafter\fail\input";
    let mut engine = crate::Engine::new(source);
    assert_eq!(engine.run_input(), Err(Error::Argument));
    assert!(!engine.packets.is_empty());
    assert!(!engine.packets[0].owners.is_empty());
    let argument_store = Rc::downgrade(&engine.values.pool.arguments);
    let owner_store = Rc::downgrade(&engine.values.pool.owners);
    let span_store = Rc::downgrade(&engine.values.pool.spans);
    drop(engine);
    assert_eq!(argument_store.strong_count(), 0);
    assert_eq!(owner_store.strong_count(), 0);
    assert_eq!(span_store.strong_count(), 0);
}

#[test]
fn nested_long_name_scratch_reaches_fixed_capacity_and_returns_on_error() {
    let outer = "a".repeat(80);
    let inner = "b".repeat(80);
    let mut previous = None;
    for iterations in [10, 100] {
        let source = format!(
            r"\catcode123=1 \catcode125=2 \catcode35=6\relax
\expandafter\def\csname {inner}\endcsname{{suffix}}
\expandafter\def\csname {outer}suffix\endcsname{{X}}
\def\wanted{{X}}\def\step{{
\edef\actual{{\csname {outer}\csname {inner}\endcsname\endcsname}}
\ifx\actual\wanted\else\undefined\fi
\advance\count0by1\relax\ifnum\count0<{iterations} \expandafter\step\fi}}
\step\end"
        );
        let mut engine = crate::Engine::new(source.as_bytes());
        engine.run_input().unwrap();
        let capacity = (
            engine.values.pool.name_bytes.pages(),
            engine
                .name_spills
                .iter()
                .map(|b| b.capacity())
                .sum::<usize>(),
        );
        if let Some(previous) = previous {
            assert_eq!(capacity, previous);
        }
        previous = Some(capacity);
        assert_eq!(engine.name_spills.len(), 2);
        assert!(engine.name_spills.iter().all(|buffer| buffer.is_empty()));
    }
    let source = format!(r"{outer}\relax {outer}\endcsname");
    let mut engine = crate::Engine::new(source.as_bytes());
    assert_eq!(engine.scan_name(), Err(Error::Syntax));
    assert_eq!(engine.name_spills.len(), 1);
    let pages = engine.values.pool.name_bytes.pages();
    let id = engine.scan_name().unwrap();
    assert_eq!(id, engine.intern(outer.as_bytes()).unwrap());
    assert_eq!(engine.values.pool.name_bytes.pages(), pages);
}
