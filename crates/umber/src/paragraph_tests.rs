use crate::{
    Engine, Progress,
    font_test_data::basic,
    nodes::{Kind, NodeId},
    numeric::Scaled,
    scalars::{ParagraphParam as P, Target},
    text::Mode,
};
fn engine(source: &[u8]) -> Engine<'_> {
    let mut e = Engine::new(source);
    e.register_tfm(b"test", &basic()).unwrap();
    e.assign_dimen(
        crate::layout_values::DimenParam::VSize.index(),
        Scaled(10000 * 65536),
        false,
    );
    e
}
fn lines(e: &Engine<'_>) -> Vec<NodeId> {
    let mut result = Vec::new();
    for list in [&e.document.page.current, &e.document.text.vertical] {
        let mut p = list.head();
        while !p.is_null() {
            let n = e.document.nodes.get(p);
            if matches!(n.kind, Kind::Box(_)) {
                result.push(p);
            }
            p = n.next;
        }
    }
    result
}
fn glyphs(e: &Engine<'_>) -> Vec<Vec<u8>> {
    let mut output = Vec::new();
    for page in e.document.page.output.iter() {
        let mut last = None;
        for p in page.placements() {
            if let crate::Placement::Glyph { character, y, .. } = p {
                if last != Some(y) {
                    output.push(Vec::new());
                    last = Some(y);
                }
                output.last_mut().unwrap().push(character);
            }
        }
    }
    output.extend(
        lines(e)
            .iter()
            .map(|&id| {
                let Kind::Box(b) = &e.document.nodes.get(id).kind else {
                    unreachable!()
                };
                let mut p = b.list.head();
                let mut text = Vec::new();
                while !p.is_null() {
                    let n = e.document.nodes.get(p);
                    if let Kind::Character { character, .. } | Kind::Ligature { character, .. } =
                        n.kind
                    {
                        text.push(character);
                    }
                    p = n.next;
                }
                text
            })
            .collect::<Vec<_>>(),
    );
    output
}
#[test]
fn forced_break_transfers_lines_and_interline_material() {
    let mut e=engine(br"\font\f=test\f\hsize=50pt\parfillskip=0pt plus1fil\baselineskip=12pt\lineskip=1pt\interlinepenalty=3\clubpenalty=5\widowpenalty=7\noindent abc\penalty-10000 def\par\end");
    while e.document.text.prev_graf == 0 {
        e.advance(1).unwrap();
    }
    assert_eq!(glyphs(&e), vec![b"abc".to_vec(), b"def".to_vec()]);
    assert_eq!(e.document.text.prev_graf, 2);
    let mut p = e.document.page.current.head();
    let mut penalty = Vec::new();
    let mut baseline = Vec::new();
    while !p.is_null() {
        let n = e.document.nodes.get(p);
        match n.kind {
            Kind::Penalty(p) => penalty.push(p),
            Kind::Glue {
                spec,
                subtype: crate::nodes::GlueKind::BaselineSkip,
            } => baseline.push(spec.width),
            _ => {}
        }
        p = n.next;
    }
    assert_eq!(penalty, vec![15]);
    assert_eq!(baseline.len(), 1);
    for id in lines(&e) {
        let Kind::Box(b) = &e.document.nodes.get(id).kind else {
            unreachable!()
        };
        assert_eq!(b.width, Scaled(50 * 65536));
    }
    assert!(e.paragraph.records.is_empty());
    assert!(e.paragraph.passive.is_empty());
    assert!(e.paragraph.breaks.is_empty());
}
#[test]
fn paragraph_shapes_reset_locally_and_restore_with_checkpoints() {
    let source=br"\catcode123=1 \catcode125=2\font\f=test\f\hsize=70pt\parfillskip=0pt plus1fil\hangindent=5pt\hangafter=-2\looseness=1\parshape2 3pt 40pt 7pt 50pt {\noindent a\penalty-10000 b\par\relax}\noindent c\par\end";
    let mut e = engine(source);
    while e.document.level == 0 {
        e.advance(1).unwrap();
    }
    let before = e.checkpoint().unwrap();
    e.advance(1).unwrap();
    assert_eq!(e.document.text.mode, Mode::Vertical);
    assert!(e.document.layout_values.shape.lines.is_none());
    assert_eq!(e.document.scalars[Target::paragraph(P::Looseness)].value, 0);
    assert_eq!(e.document.scalars[Target::paragraph(P::HangAfter)].value, 1);
    let after = e.checkpoint().unwrap();
    let first = glyphs(&e);
    let dimensions: Vec<_> = lines(&e)
        .iter()
        .map(|&id| {
            let Kind::Box(b) = &e.document.nodes.get(id).kind else {
                unreachable!()
            };
            (b.width, b.shift)
        })
        .collect();
    assert_eq!(
        dimensions,
        vec![
            (Scaled(40 * 65536), Scaled(3 * 65536)),
            (Scaled(50 * 65536), Scaled(7 * 65536))
        ]
    );
    while e.advance(1).unwrap() != Progress::Complete {}
    let completed = glyphs(&e);
    e.restore(&after).unwrap();
    assert_eq!(glyphs(&e), first);
    while e.advance(1).unwrap() != Progress::Complete {}
    assert_eq!(glyphs(&e), completed);
    e.restore(&before).unwrap();
    assert!(e.document.layout_values.shape.lines.is_some());
    assert!(e.document.text.vertical.is_empty());
    while e.advance(1).unwrap() != Progress::Complete {}
    assert_eq!(glyphs(&e), completed);
    drop(e);
    drop(before);
    drop(after);
}
#[test]
fn checkpoint_replays_pending_macro_and_open_group_over_paragraphs() {
    let source=br"\catcode123=1 \catcode125=2 \catcode35=6\font\f=test\f\hsize=45pt\parfillskip=0pt plus1fil\def\two#1{#1\par#1\par}{\count0=7\two{abc def ghi}\count0=9}\end";
    let mut e = engine(source);
    while e.document.text.prev_graf == 0 {
        e.advance(1).unwrap();
    }
    assert_eq!(e.document.level, 1);
    let saved = e.checkpoint().unwrap();
    let prefix = glyphs(&e);
    while e.advance(1).unwrap() != Progress::Complete {}
    let complete = glyphs(&e);
    assert_eq!(complete.len(), prefix.len() * 2);
    e.restore(&saved).unwrap();
    assert_eq!(glyphs(&e), prefix);
    while e.advance(1).unwrap() != Progress::Complete {}
    assert_eq!(glyphs(&e), complete);
    let retained = e.checkpoint().unwrap();
    drop(e);
    drop(retained);
    drop(saved);
}
#[test]
fn empty_paragraph_also_resets_shape_and_hanging_parameters() {
    let mut e = Engine::new(
        br"\hangindent=9pt\hangafter=4\looseness=3\parshape1 2pt 8pt\par\noindent\par\end",
    );
    e.advance(100).unwrap();
    assert!(e.document.layout_values.shape.lines.is_none());
    assert_eq!(e.document.scalars[Target::paragraph(P::HangAfter)].value, 1);
    assert_eq!(e.document.scalars[Target::paragraph(P::Looseness)].value, 0);
}

#[test]
fn discretionary_break_transfers_children_and_reuses_original_ids() {
    use crate::{
        layout_values::{DimenParam, GlueParam},
        nodes::{Kind, List},
        numeric::{Glue, GlueOrder},
    };
    let mut e = engine(br"\font\f=test\f\end");
    e.advance(99).unwrap();
    e.assign_dimen(DimenParam::HSize.index(), Scaled(30 * 65536), false);
    e.assign_glue(
        GlueParam::ParFill.index(),
        Glue {
            stretch: Scaled(65536),
            stretch_order: GlueOrder::Fil,
            ..Glue::default()
        },
        false,
    );
    e.document.scalars[Target::paragraph(P::HyphenPenalty)].value = -10000;
    e.new_graf(false).unwrap();
    let glyph = |e: &mut Engine<'_>, c: u8| {
        e.document
            .nodes
            .alloc(Kind::Character {
                font: 1,
                character: c,
            })
            .unwrap()
    };
    let a = glyph(&mut e, b'a');
    let a_id = a.id();
    e.document.nodes.append(&mut e.document.text.horizontal, a);
    let mut pre = List::default();
    let hyphen = glyph(&mut e, b'-');
    let hyphen_id = hyphen.id();
    e.document.nodes.append(&mut pre, hyphen);
    let mut post = List::default();
    let b = glyph(&mut e, b'b');
    let b_id = b.id();
    e.document.nodes.append(&mut post, b);
    let disc = e
        .document
        .nodes
        .alloc(Kind::discretionary(pre, post, 1))
        .unwrap();
    let disc_id = disc.id();
    e.document
        .nodes
        .append(&mut e.document.text.horizontal, disc);
    let c = glyph(&mut e, b'c');
    let c_id = c.id();
    e.document.nodes.append(&mut e.document.text.horizontal, c);
    let d = glyph(&mut e, b'd');
    let d_id = d.id();
    e.document.nodes.append(&mut e.document.text.horizontal, d);
    let old_store = e.document.nodes.clone();
    let old_list = e.document.text.horizontal.snapshot();
    e.end_graf().unwrap();
    assert_eq!(glyphs(&e), vec![b"a-".to_vec(), b"bd".to_vec()]);
    let mut reused = Vec::new();
    for line in lines(&e) {
        let Kind::Box(data) = &e.document.nodes.get(line).kind else {
            unreachable!()
        };
        let mut p = data.list.head();
        while !p.is_null() {
            reused.push(p);
            p = e.document.nodes.get(p).next;
        }
    }
    for id in [a_id, hyphen_id, b_id, d_id, disc_id] {
        assert!(reused.contains(&id));
    }
    assert!(!reused.contains(&c_id));
    let Kind::Discretionary {
        pre,
        post,
        replace_count,
    } = &e.document.nodes.get(disc_id).kind
    else {
        unreachable!()
    };
    assert!(pre.is_empty() && post.is_empty());
    assert_eq!(*replace_count, 0);
    assert!(matches!(
        old_store.get(c_id).kind,
        Kind::Character {
            character: b'c',
            ..
        }
    ));
    assert_eq!(old_store.get(old_list.head()).next, disc_id);
    drop(e);
    assert!(matches!(
        old_store.get(d_id).kind,
        Kind::Character {
            character: b'd',
            ..
        }
    ));
    drop(old_list);
    drop(old_store);
}
#[test]
fn discarded_tail_glue_does_not_require_infinite_shrink_recovery() {
    let mut e=engine(br"\font\f=test\f\hsize=40pt\parfillskip=0pt plus1fil\noindent a\hskip0pt minus1fil\par\end");
    assert_eq!(e.advance(99), Ok(Progress::Complete));
    assert_eq!(glyphs(&e), vec![b"a".to_vec()]);
}
#[test]
fn failed_paragraph_keeps_roots_reachable_and_restore_can_repeat() {
    let source = br"\font\f=test\f\hsize=40pt\noindent a\hskip0pt minus1fil b\par\end";
    let mut e = engine(source);
    e.advance(1).unwrap();
    let saved = e.checkpoint().unwrap();
    for _ in 0..3 {
        assert_eq!(e.advance(99), Err(crate::Error::Unsupported));
        assert!(!e.document.text.horizontal.is_empty());
        assert!(!e.can_checkpoint());
        assert!(
            e.paragraph.records.is_empty()
                && e.paragraph.passive.is_empty()
                && e.paragraph.breaks.is_empty()
        );
        e.restore(&saved).unwrap();
        assert!(e.document.text.horizontal.is_empty() && e.document.text.vertical.is_empty());
    }
    drop(e);
    drop(saved);
}
#[test]
fn shape_local_global_local_and_reset_restore_the_global_shape() {
    let source=br"\catcode123=1\catcode125=2\parshape1 1pt 10pt {\parshape1 2pt 20pt\global\parshape1 3pt 30pt\parshape1 4pt 40pt\par}\relax\end";
    let mut e = Engine::new(source);
    while e.document.level == 0 {
        e.advance(1).unwrap();
    }
    let saved = e.checkpoint().unwrap();
    while e.advance(1).unwrap() != Progress::Complete {}
    let shape = e.document.layout_values.shape.lines.as_ref().unwrap();
    assert_eq!(shape[0].width, Scaled(30 * 65536));
    e.restore(&saved).unwrap();
    assert_eq!(
        e.document.layout_values.shape.lines.as_ref().unwrap()[0].width,
        Scaled(10 * 65536)
    );
    while e.advance(1).unwrap() != Progress::Complete {}
    assert_eq!(
        e.document.layout_values.shape.lines.as_ref().unwrap()[0].width,
        Scaled(30 * 65536)
    );
    let retained = e.checkpoint().unwrap();
    drop(e);
    drop(saved);
    drop(retained);
}
#[test]
fn equal_demerit_predecessors_choose_later_but_terminal_selection_chooses_first() {
    use crate::{
        line_search::Search,
        paragraph::{Active, NONE, Parameters, Passive, RecordKind, Scratch},
    };
    let e = Engine::new(b"");
    let parameters = Parameters::capture(&e).unwrap();
    let mut scratch = Scratch::default();
    scratch.reset();
    scratch.passive.push(Passive {
        breakpoint: NodeId::NULL,
        previous: NONE,
    });
    scratch.passive.push(Passive {
        breakpoint: NodeId::NULL,
        previous: NONE,
    });
    scratch.alloc(
        RecordKind::Active(Active {
            line: i32::MAX,
            fitness: 0,
            hyphenated: true,
            passive: NONE,
            demerits: 0,
        }),
        1,
    );
    scratch.alloc(
        RecordKind::Active(Active {
            line: 1,
            fitness: 2,
            hyphenated: false,
            passive: 0,
            demerits: 7,
        }),
        2,
    );
    scratch.alloc(
        RecordKind::Active(Active {
            line: 1,
            fitness: 2,
            hyphenated: false,
            passive: 1,
            demerits: 7,
        }),
        0,
    );
    let mut search = Search {
        engine: &e,
        parameters: &parameters,
        scratch: &mut scratch,
        width: [0; 6],
        background: [0; 6],
        point: NodeId::NULL,
        threshold: 10000,
        final_pass: false,
        disc_width: 0,
    };
    search.try_break(-10000, true).unwrap();
    let (best, _, _) = search.best().unwrap();
    assert_eq!(search.scratch.passive[best].previous, 1);
    search.scratch.reset();
    search.scratch.alloc(
        RecordKind::Active(Active {
            line: i32::MAX,
            fitness: 0,
            hyphenated: true,
            passive: NONE,
            demerits: 0,
        }),
        1,
    );
    search.scratch.alloc(
        RecordKind::Active(Active {
            line: 2,
            fitness: 2,
            hyphenated: true,
            passive: 11,
            demerits: 7,
        }),
        2,
    );
    search.scratch.alloc(
        RecordKind::Active(Active {
            line: 3,
            fitness: 2,
            hyphenated: true,
            passive: 22,
            demerits: 7,
        }),
        0,
    );
    assert_eq!(search.best().unwrap().0, 11);
}
#[test]
fn final_pass_artificial_candidate_preserves_existing_demerits() {
    use crate::{
        line_search::Search,
        paragraph::{Active, NONE, Parameters, RecordKind, Scratch},
    };
    let e = Engine::new(b"");
    let parameters = Parameters::capture(&e).unwrap();
    for final_pass in [false, true] {
        let mut scratch = Scratch::default();
        scratch.reset();
        scratch.alloc(
            RecordKind::Active(Active {
                line: i32::MAX,
                fitness: 0,
                hyphenated: true,
                passive: NONE,
                demerits: 0,
            }),
            1,
        );
        scratch.alloc(
            RecordKind::Active(Active {
                line: 1,
                fitness: 2,
                hyphenated: false,
                passive: NONE,
                demerits: 7,
            }),
            0,
        );
        let mut search = Search {
            engine: &e,
            parameters: &parameters,
            scratch: &mut scratch,
            width: [100, 0, 0, 0, 0, 0],
            background: [0; 6],
            point: NodeId::NULL,
            threshold: 0,
            final_pass,
            disc_width: 0,
        };
        search.try_break(-10000, true).unwrap();
        if final_pass {
            let (passive, _, _) = search.best().unwrap();
            let record = search
                .scratch
                .records
                .iter()
                .find_map(|r| {
                    if let RecordKind::Active(a) = r.kind {
                        if a.passive == passive { Some(a) } else { None }
                    } else {
                        None
                    }
                })
                .unwrap();
            assert_eq!(record.demerits, 7);
        } else {
            assert!(search.best().is_none());
        }
    }
}
#[test]
fn packing_zero_flex_and_normal_over_shrink_follow_tex() {
    use crate::{nodes::GlueSign, numeric::GlueOrder, packing::glue_setting};
    assert_eq!(glue_setting(0, [1; 4], [1; 4], true), Default::default());
    let no_flex = glue_setting(10, [0; 4], [0; 4], true);
    assert_eq!(no_flex.sign, GlueSign::Normal);
    assert_eq!(no_flex.ratio, 0.0);
    let over = glue_setting(-10, [0; 4], [2, 0, 0, 0], true);
    assert_eq!(over.sign, GlueSign::Shrinking);
    assert_eq!(over.ratio, 1.0);
    let none = glue_setting(-10, [0; 4], [0; 4], true);
    assert_eq!(none.sign, GlueSign::Normal);
    assert_eq!(none.ratio, 1.0);
    let infinite = glue_setting(-10, [0; 4], [2, 1, 0, 0], true);
    assert_eq!(infinite.order, GlueOrder::Fil);
    assert_eq!(infinite.ratio, 10.0);
}

#[test]
fn invalid_glyph_after_valid_font_fails_before_packing_and_restores() {
    for (font, character) in [(1, 0), (u32::MAX, b'a')] {
        let mut e = engine(br"\font\f=test\f\noindent a\par\end");
        e.advance(2).unwrap();
        let saved = e.checkpoint().unwrap();
        for _ in 0..2 {
            let read = e.next_expanded().unwrap();
            e.execute(read).unwrap();
        }
        e.append_horizontal(Kind::Character { font, character })
            .unwrap();
        assert_eq!(e.end_graf(), Err(crate::Error::Font));
        assert!(!e.document.text.horizontal.is_empty());
        assert!(e.document.text.vertical.is_empty());
        e.restore(&saved).unwrap();
        assert_eq!(e.advance(99), Ok(Progress::Complete));
        assert_eq!(glyphs(&e), vec![b"a".to_vec()]);
    }
}
#[test]
fn search_error_resets_checked_out_scratch_before_restore_and_successful_replay() {
    use crate::nodes::List;
    let mut e = engine(br"\font\f=test\f\noindent a\par\end");
    e.advance(2).unwrap();
    let saved = e.checkpoint().unwrap();
    let read = e.next_expanded().unwrap();
    e.execute(read).unwrap();
    let read = e.next_expanded().unwrap();
    e.execute(read).unwrap();
    assert_eq!(e.document.text.mode, Mode::Horizontal);
    // General discretionary scanners reject glue children; this internal
    // unsupported child forces a search Result error after scratch checkout.
    let mut pre = List::default();
    let glue = e
        .document
        .nodes
        .alloc(Kind::Glue {
            spec: Default::default(),
            subtype: Default::default(),
        })
        .unwrap();
    e.document.nodes.append(&mut pre, glue);
    e.append_horizontal(Kind::discretionary(pre, List::default(), 0))
        .unwrap();
    assert_eq!(e.end_graf(), Err(crate::Error::Unsupported));
    assert!(
        e.paragraph.records.is_empty()
            && e.paragraph.passive.is_empty()
            && e.paragraph.breaks.is_empty()
    );
    assert!(!e.document.text.horizontal.is_empty());
    e.restore(&saved).unwrap();
    assert!(e.document.text.horizontal.is_empty() && e.document.text.vertical.is_empty());
    assert_eq!(e.advance(99), Ok(Progress::Complete));
    assert_eq!(glyphs(&e), vec![b"a".to_vec()]);
    drop(e);
    drop(saved);
}

#[test]
fn terminal_break_uses_final_hyphen_demerits_and_disc_uses_double_hyphen_demerits() {
    use crate::{
        line_search::Search,
        nodes::List,
        paragraph::{Active, NONE, Parameters, RecordKind, Scratch},
    };
    for terminal in [false, true] {
        let mut e = Engine::new(b"");
        e.append_horizontal(Kind::discretionary(List::default(), List::default(), 0))
            .unwrap();
        let mut parameters = Parameters::capture(&e).unwrap();
        parameters.integers[P::FinalHyphenDemerits as usize] = 9;
        parameters.integers[P::DoubleHyphenDemerits as usize] = 17;
        let mut scratch = Scratch::default();
        scratch.reset();
        scratch.alloc(
            RecordKind::Active(Active {
                line: i32::MAX,
                fitness: 0,
                hyphenated: true,
                passive: NONE,
                demerits: 0,
            }),
            1,
        );
        scratch.alloc(
            RecordKind::Active(Active {
                line: 1,
                fitness: 2,
                hyphenated: true,
                passive: NONE,
                demerits: 0,
            }),
            0,
        );
        let mut search = Search {
            engine: &e,
            parameters: &parameters,
            scratch: &mut scratch,
            width: [0; 6],
            background: [0; 6],
            point: if terminal {
                NodeId::NULL
            } else {
                e.document.text.horizontal.head()
            },
            threshold: 10000,
            final_pass: false,
            disc_width: 0,
        };
        search.try_break(-10000, true).unwrap();
        let (passive, _, _) = search.best().unwrap();
        let demerits = search
            .scratch
            .records
            .iter()
            .find_map(|r| match r.kind {
                RecordKind::Active(a) if a.passive == passive => Some(a.demerits),
                _ => None,
            })
            .unwrap();
        assert_eq!(demerits, if terminal { 9 } else { 17 });
    }
}
