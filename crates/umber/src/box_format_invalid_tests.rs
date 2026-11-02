use super::*;
fn wire(nodes: u32, roots: &[(u8, u32)], body: impl FnOnce(&mut Writer)) -> Vec<u8> {
    let mut w = Writer::default();
    w.bytes(&[0; 24]);
    w.u8(1);
    w.u32(roots.len() as u32);
    w.u32(nodes);
    for &(register, id) in roots {
        w.u8(register);
        w.u32(id);
    }
    body(&mut w);
    w.finish().unwrap()[24..].to_vec()
}
fn header(w: &mut Writer, tag: u8, next: u32) {
    w.u8(tag);
    w.u32(next);
}
fn rule(w: &mut Writer, next: u32) {
    header(w, 7, next);
    for _ in 0..3 {
        w.i32(1);
    }
}
fn box_node(w: &mut Writer, next: u32, child: u32) {
    header(w, 8, next);
    w.u8(0);
    for _ in 0..4 {
        w.i32(0);
    }
    w.u8(0);
    w.u8(0);
    w.u64(0);
    w.u32(child);
}
fn glue(w: &mut Writer) {
    for _ in 0..3 {
        w.i32(0);
    }
    w.u8(0);
    w.u8(0);
}
fn rejected(e: &Engine<'_>, bytes: &[u8]) {
    let mut store = NodeStore::default();
    assert!(matches!(
        crate::nodes::format::read(e, &mut Reader::new(bytes).unwrap(), &mut store),
        Err(Error::Format)
    ));
    assert_eq!(store.live_count(), 0);
}
#[test]
fn box_format_malformed_topology_and_structures_cannot_alias_or_escape_cleanup() {
    let e = engine();
    for bytes in [
        wire(1, &[(0, 1)], |w| rule(w, 0)), // nonbox root
        wire(2, &[(0, 2)], |w| {
            rule(w, 0);
            box_node(w, 1, 0);
        }), // nonsingleton root
        wire(2, &[(0, 2)], |w| {
            rule(w, 0);
            box_node(w, 0, 0);
        }), // orphan
        wire(2, &[(0, 2)], |w| {
            rule(w, 2);
            box_node(w, 0, 0);
        }), // forward/cycle
        wire(3, &[(0, 3)], |w| {
            rule(w, 0);
            box_node(w, 0, 1);
            box_node(w, 0, 1);
        }), // consumed child
        wire(2, &[(0, 2)], |w| {
            rule(w, 0);
            header(w, 12, 0);
            w.u32(1);
            w.u32(1);
            w.u8(0);
        }), // same pre/post owner
        wire(1, &[(0, 1), (1, 1)], |w| box_node(w, 0, 0)),
        wire(2, &[(0, 2), (0, 1)], |w| {
            box_node(w, 0, 0);
            box_node(w, 0, 0);
        }),
        wire(1, &[(0, 1)], |w| {
            header(w, 12, 0);
            w.u32(0);
            w.u32(0);
            w.u8(1);
        }), // missing replacement
        wire(2, &[(0, 2)], |w| {
            header(w, 5, 0);
            glue(w);
            w.u8(0);
            header(w, 12, 1);
            w.u32(0);
            w.u32(0);
            w.u8(1);
        }), // glue replacement
        wire(3, &[(0, 3)], |w| {
            header(w, 3, 0);
            w.i32(1);
            w.u8(0);
            header(w, 2, 0);
            w.u32(1);
            w.u8(b'A');
            w.u8(0);
            w.u8(0);
            w.u32(1);
            box_node(w, 0, 2);
        }), // noncharacter components
        wire(2, &[(0, 2)], |w| {
            header(w, 10, 0);
            glue(w);
            w.u8(10);
            w.u32(0);
            box_node(w, 0, 1);
        }), // void template
        wire(3, &[(0, 3)], |w| {
            header(w, 3, 0);
            w.i32(1);
            w.u8(0);
            header(w, 10, 0);
            glue(w);
            w.u8(10);
            w.u32(1);
            box_node(w, 0, 2);
        }),
    ] {
        rejected(&e, &bytes);
    }
}
#[test]
fn box_format_checked_scalar_tags_glyphs_and_nonfinite_settings() {
    let e = engine();
    let cases: [fn(&mut Writer); 12] = [
        |w| {
            header(w, 255, 0);
            w.bytes(&[0; 32]);
        },
        |w| {
            header(w, 3, 0);
            w.i32(0);
            w.u8(3);
        },
        |w| {
            header(w, 4, 0);
            w.i32(0);
            w.u8(2);
        },
        |w| {
            header(w, 5, 0);
            glue(w);
            w.u8(255);
        },
        |w| {
            header(w, 5, 0);
            glue(w);
            w.u8(10);
        },
        |w| {
            header(w, 5, 0);
            for _ in 0..3 {
                w.i32(0);
            }
            w.u8(4);
            w.u8(0);
            w.u8(0);
        },
        |w| {
            header(w, 0, 0);
            w.u8(0);
            w.u8(0);
            w.u8(63);
        },
        |w| {
            header(w, 0, 0);
            w.u8(0);
            w.u8(1);
            w.u8(64);
        },
        |w| {
            header(w, 1, 0);
            w.u32(99);
            w.u8(b'A');
        },
        |w| {
            header(w, 1, 0);
            w.u32(1);
            w.u8(0);
        },
        |w| {
            header(w, 9, 0);
            w.u8(255);
            w.i32(0);
            glue(w);
            w.i32(0);
            w.i32(0);
            w.u32(0);
        },
        |w| {
            header(w, 8, 0);
            w.u8(2);
            for _ in 0..4 {
                w.i32(0);
            }
            w.u8(0);
            w.u8(0);
            w.u64(0);
            w.u32(0);
        },
    ];
    for body in cases {
        rejected(
            &e,
            &wire(2, &[(0, 2)], |w| {
                body(w);
                box_node(w, 0, 1);
            }),
        );
    }
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        rejected(
            &e,
            &wire(1, &[(0, 1)], |w| {
                header(w, 8, 0);
                w.u8(0);
                for _ in 0..4 {
                    w.i32(0);
                }
                w.u8(1);
                w.u8(0);
                w.u64(value.to_bits());
                w.u32(0);
            }),
        );
    }
}
#[test]
fn box_format_truncation_trailing_bytes_and_failures_leave_existing_pair_unchanged() {
    let mut e = engine();
    forest(&mut e);
    let before = encoded(&e);
    let good = wire(2, &[(17, 2)], |w| {
        rule(w, 0);
        box_node(w, 0, 1);
    });
    for end in 0..good.len() {
        assert!(read(&mut e, &mut Reader::new(&good[..end]).unwrap()).is_err());
        assert_eq!(encoded(&e), before);
    }
    let mut trailing = good.clone();
    trailing.push(0);
    rejected(&e, &trailing);
    assert!(read(&mut e, &mut Reader::new(&trailing).unwrap()).is_err());
    assert_eq!(encoded(&e), before);
    read(&mut e, &mut Reader::new(&good).unwrap()).unwrap();
    assert_eq!(e.document.nodes.live_count(), 2);
    assert!(e.document.boxes.head(0).is_null());
    assert!(!e.document.boxes.head(17).is_null());
}
#[test]
fn box_format_writer_rejects_shared_cycle_unfinished_and_free_nodes() {
    let mut e = Engine::new(b"");
    let node = e.document.nodes.alloc_box(BoxData::default()).unwrap();
    install(&mut e, 0, node);
    e.document.boxes.0[1].list = e.document.boxes.0[0].list.snapshot();
    let mut w = Writer::default();
    assert_eq!(write(&e, &mut w), Err(Error::Format));
    e.document.boxes.0[1].list = List::default();
    let root = e.document.boxes.head(0);
    e.document.nodes.box_mut(root).list = e.document.boxes.0[0].list.snapshot();
    assert_eq!(write(&e, &mut w), Err(Error::Format));
    let mut e = Engine::new(b"");
    let node = e.document.nodes.alloc_box(BoxData::default()).unwrap();
    let node = e.document.nodes.mark_unset(node);
    install(&mut e, 0, node);
    assert_eq!(write(&e, &mut w), Err(Error::Format));
    *e.document.nodes.kind_mut(e.document.boxes.head(0)) = Kind::Free;
    assert_eq!(write(&e, &mut w), Err(Error::Format));
}

#[test]
fn box_format_explicit_discretionary_count_uses_full_native_quarterword_range() {
    for count in [128, 255] {
        let bytes = wire(count + 2, &[(0, count + 2)], |w| {
            for id in 1..=count {
                rule(w, id - 1);
            }
            header(w, 12, count);
            w.u32(0);
            w.u32(0);
            w.u8(count as u8);
            box_node(w, 0, count + 1);
        });
        let mut e = Engine::new(b"");
        read(&mut e, &mut Reader::new(&bytes).unwrap()).unwrap();
        assert_eq!(encoded(&e), bytes);
    }
}
