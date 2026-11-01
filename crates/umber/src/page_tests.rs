use crate::{
    Engine, Error, Pages, Placement, Progress,
    font_test_data::basic,
    nodes::{BoxData, Kind, List},
    numeric::Scaled,
    packing::PackSpec,
    scalars::{PageParam, Target},
};
fn engine(source: &[u8]) -> Engine<'_> {
    let mut e = Engine::new(source);
    e.register_tfm(b"test", &basic()).unwrap();
    e
}
fn complete(e: &mut Engine<'_>) {
    while e.advance(1).unwrap() != Progress::Complete {}
}
type Signature = (
    i32,
    Vec<crate::PackedBox>,
    Vec<Placement>,
    Vec<(Vec<u8>, i32)>,
);
fn signature(pages: &Pages) -> Vec<Signature> {
    pages
        .iter()
        .map(|p| {
            (
                p.output_penalty,
                p.boxes().to_vec(),
                p.placements().collect::<Vec<_>>(),
                p.fonts()
                    .iter()
                    .map(|f| (f.name().to_vec(), f.size()))
                    .collect(),
            )
        })
        .collect()
}
#[test]
fn end_flush_is_exact_height_with_fill_and_no_empty_job_page() {
    let mut empty = Engine::new(br"\end");
    complete(&mut empty);
    assert!(empty.outcome().pages.is_empty());
    let mut e=engine(br"\font\f=test\f\hsize=40pt\vsize=50pt\maxdepth=3pt\topskip=10pt\parfillskip=0pt plus1fil\noindent abc\end");
    complete(&mut e);
    let out = e.outcome();
    assert_eq!(out.pages.len(), 1);
    let p = out.pages.get(0).unwrap();
    let b = p.boxes()[0];
    assert_eq!((b.width, b.height, b.depth), (40 * 65536, 50 * 65536, 0));
    assert_eq!((b.glue.sign, b.glue.order), (1, 2));
    assert_eq!(p.output_penalty, -1_073_741_824);
    assert!(matches!(
        p.placements().next().unwrap(),
        Placement::Glyph {
            x: 0,
            y: 655360,
            ..
        }
    ));
    assert!(e.document.page.current.is_empty() && e.document.text.vertical.is_empty());
    assert_eq!(e.document.page.quantity(0), Scaled::MAX);
}
#[test]
fn discarded_top_material_updates_caches_and_kern_waits_for_successor() {
    let mut e=Engine::new(br"\catcode123=1\catcode125=2\vskip3pt plus2fil\par\message{\the\lastskip}\kern7pt\par\message{\the\lastkern}\penalty9\message{\the\lastpenalty}\end");
    complete(&mut e);
    assert_eq!(
        e.outcome().message_bytes(),
        vec![
            b"3.0pt plus 2.0fil".to_vec(),
            b"7.0pt".to_vec(),
            b"9".to_vec()
        ]
    );
    assert!(e.outcome().pages.is_empty());
    let mut e = Engine::new(
        br"\vsize=40pt\hrule width10pt height3pt\par\kern7pt\par\relax\vskip2pt\par\end",
    );
    while e.document.page.state.last_kern != 7 * 65536 {
        e.advance(1).unwrap();
    }
    assert!(matches!(
        e.document.nodes.get(e.document.text.vertical.head()).kind,
        Kind::Kern { .. }
    ));
    assert_eq!(e.document.page.state.totals[1], 3 * 65536);
    let saved = e.checkpoint().unwrap();
    complete(&mut e);
    let expected = signature(&e.outcome().pages);
    e.restore(&saved).unwrap();
    complete(&mut e);
    assert_eq!(signature(&e.outcome().pages), expected);
}
#[test]
fn frozen_geometry_prevdepth_and_forced_penalty_survive_page_reset() {
    let source=br"\catcode123=1\catcode125=2\vsize=30pt\maxdepth=2pt\hrule width10pt height3pt depth5pt\par\vsize=90pt\maxdepth=9pt\prevdepth=7pt\penalty-12345\message{\the\outputpenalty/\the\lastpenalty/\the\prevdepth}\end";
    let mut e = Engine::new(source);
    complete(&mut e);
    let out = e.outcome();
    let b = out.pages.get(0).unwrap().boxes()[0];
    assert_eq!((b.height, b.depth), (30 * 65536, 2 * 65536));
    assert_eq!(out.message_bytes(), vec![b"-12345/10000/7.0pt".to_vec()]);
    assert_eq!(out.pages.get(0).unwrap().output_penalty, -12345);
}
#[test]
fn equal_cost_selects_later_break_and_returns_old_best_remainder() {
    let mut e=Engine::new(br"\vsize=10pt\hrule width4pt height2pt\vfil\penalty0\kern0pt\penalty0\pagegoal=1pt\penalty1\end");
    // Every underfull candidate has zero badness once fil stretch is present.
    while e.document.page.output.is_empty() {
        e.advance(1).unwrap();
    }
    let p = e.document.page.output.get(0).unwrap();
    assert_eq!(p.output_penalty, 0);
    assert_eq!(p.boxes()[0].height, 10 * 65536);
    // The best candidate is the later penalty; its predecessor is the zero kern.
    assert_eq!(p.boxes()[0].glue.order, 1);
    complete(&mut e);
}
#[test]
fn incomplete_page_open_group_pending_macro_and_old_results_replay() {
    let source=br"\catcode123=1\catcode125=2\catcode35=6\font\f=test\f\hsize=45pt\vsize=35pt\topskip=10pt\maxdepth=3pt\baselineskip=12pt\parfillskip=0pt plus1fil\def\two#1{#1\par#1\par}{\count0=7\two{abc def ghi}\count0=9}\end";
    let mut e = engine(source);
    while e.document.text.prev_graf == 0 {
        e.advance(1).unwrap();
    }
    assert_eq!(e.document.level, 1);
    assert!(!e.document.page.current.is_empty());
    let saved = e.checkpoint().unwrap();
    complete(&mut e);
    let held = e.outcome();
    let expected = signature(&held.pages);
    e.restore(&saved).unwrap();
    assert!(e.outcome().pages.len() < held.pages.len());
    complete(&mut e);
    assert_eq!(signature(&e.outcome().pages), expected);
    drop(e);
    drop(saved);
    assert_eq!(signature(&held.pages), expected);
    assert_eq!(
        held.pages.iter().next().unwrap().fonts()[0]
            .metrics(b'a')
            .unwrap()
            .width,
        491520
    );
}
#[test]
fn retained_font_versions_outlive_id_reuse_and_cache_growth() {
    let source=br"\ifnum\count0=0\font\f=test at10pt\else\font\f=other at20pt\fi\f\hsize=40pt\vsize=30pt\parfillskip=0pt plus1fil\noindent a\end";
    let mut e = engine(source);
    e.register_tfm(b"other", &basic()).unwrap();
    let before = e.checkpoint().unwrap();
    complete(&mut e);
    let old = e.outcome();
    let oldfont = old.pages.get(0).unwrap().fonts()[0].clone();
    assert_eq!(oldfont.name(), b"test");
    assert_eq!(oldfont.size(), 10 * 65536);
    e.restore(&before).unwrap();
    e.document.scalars[Target::count(0)].value = 1;
    complete(&mut e);
    let new = e.outcome();
    assert_eq!(new.pages.get(0).unwrap().fonts()[0].name(), b"other");
    assert_eq!(new.pages.get(0).unwrap().fonts()[0].size(), 20 * 65536);
    for size in 1..35 {
        e.font_catalog.scaled(0, Scaled(size * 65536)).unwrap();
    }
    drop(before);
    drop(e);
    assert_eq!(oldfont.metrics(b'a').unwrap().width, 491520);
    assert_eq!(
        new.pages.get(0).unwrap().fonts()[0]
            .metrics(b'a')
            .unwrap()
            .width,
        983040
    );
    drop(new);
    drop(old);
    assert_eq!(oldfont.name(), b"test");
}
#[test]
fn mixed_font_line_uses_each_glyphs_metrics_and_retains_both_resources() {
    let mut e = engine(
        br"\font\f=test at10pt\font\g=test at20pt\hsize=100pt\vsize=50pt\parfillskip=0pt plus1fil\noindent\f a\g a\f a\end",
    );
    complete(&mut e);
    let out = e.outcome();
    drop(e);
    let page = out.pages.get(0).unwrap();
    assert_eq!(page.fonts().len(), 2);
    assert_eq!(page.fonts()[0].size(), 10 * 65536);
    assert_eq!(page.fonts()[1].size(), 20 * 65536);
    let glyphs: Vec<_> = page
        .placements()
        .filter_map(|p| match p {
            Placement::Glyph {
                font, character, x, ..
            } => Some((font, character, x)),
            _ => None,
        })
        .collect();
    assert_eq!(
        glyphs,
        [(0, b'a', 0), (1, b'a', 491520), (0, b'a', 1474560)]
    );
}
#[test]
fn output_prefix_branch_append_and_both_drop_orders() {
    let mut e=Engine::new(br"\catcode123=1\catcode125=2\vsize=10pt\def\p{\hrule width5pt height2pt\penalty-10000}\p\p\p\p\p\p\p\p\p\p\p\p\p\p\p\p\p\p\p\p\end");
    while e.document.page.output.len() < 3 {
        e.advance(1).unwrap();
    }
    let prefix = e.outcome();
    let saved = e.checkpoint().unwrap();
    complete(&mut e);
    let old = e.outcome();
    assert_eq!(old.pages.len(), 20);
    assert_eq!(prefix.pages.len(), 3);
    e.restore(&saved).unwrap();
    complete(&mut e);
    assert_eq!(signature(&old.pages), signature(&e.outcome().pages));
    assert_eq!(prefix.pages.iter().count(), 3);
    assert!(prefix.pages.get(3).is_none());
    drop(saved);
    drop(e);
    assert_eq!(old.pages.iter().count(), 20);
    drop(old);
    assert_eq!(prefix.pages.iter().count(), 3);
    let mut e = Engine::new(br"\hrule width2pt\penalty-10000\end");
    complete(&mut e);
    let checkpoint = e.checkpoint().unwrap();
    let held = e.outcome();
    drop(e);
    drop(held);
    drop(checkpoint);
}
#[test]
fn lowering_error_publishes_nothing_and_reuses_cleared_scratch() {
    let mut e = engine(br"\font\f=test\f\end");
    complete(&mut e);
    // A populated table slot above the logical loaded-font end must still be
    // rejected, as must nullfont and an absent character in a valid font.
    e.document.fonts.descriptors[2] = e.document.fonts.descriptors[1];
    for (font, character) in [(0, b'a'), (1, 0), (2, b'a'), (u32::MAX, b'a')] {
        let mut list = List::default();
        // Prime the binding before a missing glyph or invalid font is visited.
        let valid = e
            .document
            .nodes
            .alloc(Kind::Character {
                font: 1,
                character: b'a',
            })
            .unwrap();
        e.document.nodes.append(&mut list, valid);
        let glyph = e
            .document
            .nodes
            .alloc(Kind::Character { font, character })
            .unwrap();
        e.document.nodes.append(&mut list, glyph);
        let bad = e
            .document
            .nodes
            .alloc_box(BoxData {
                list,
                ..BoxData::default()
            })
            .unwrap();
        assert_eq!(e.publish_page(bad, 0), Err(Error::Font));
        assert!(e.outcome().pages.is_empty());
    }
    let mut list = List::default();
    let glyph = e
        .document
        .nodes
        .alloc(Kind::Character {
            font: 1,
            character: b'a',
        })
        .unwrap();
    e.document.nodes.append(&mut list, glyph);
    let bad = e
        .document
        .nodes
        .alloc_box(BoxData {
            list,
            vertical: true,
            ..BoxData::default()
        })
        .unwrap();
    assert_eq!(e.publish_page(bad, 0), Err(Error::Unsupported));
    assert!(e.outcome().pages.is_empty());
    let rule = e
        .document
        .nodes
        .alloc(Kind::Rule {
            width: Scaled(10),
            height: Scaled(10),
            depth: Scaled(0),
        })
        .unwrap();
    let mut list = List::default();
    e.document.nodes.append(&mut list, rule);
    let good = e
        .vpackage(list, PackSpec::To(Scaled(10)), Scaled(0))
        .unwrap();
    e.publish_page(good, 0).unwrap();
    assert_eq!(
        e.outcome()
            .pages
            .get(0)
            .unwrap()
            .placements()
            .collect::<Vec<_>>(),
        [Placement::Rule {
            x: 0,
            y: 0,
            width: 10,
            height: 10
        }]
    );
}
#[test]
fn page_quantities_are_ungrouped_but_masked_while_empty() {
    let mut e=Engine::new(br"\catcode123=1\catcode125=2{\pagegoal=4pt}\message{\the\pagegoal/\the\pagetotal}\vsize=20pt\hrule height2pt width1pt\par{\pagegoal=7pt}\message{\the\pagegoal}\penalty-10000\message{\the\pagegoal/\the\pagetotal}\end");
    complete(&mut e);
    assert_eq!(
        e.outcome().message_bytes(),
        vec![
            b"16383.99998pt/0.0pt".to_vec(),
            b"7.0pt".to_vec(),
            b"16383.99998pt/0.0pt".to_vec()
        ]
    );
    assert_eq!(
        e.document.scalars[Target::page(PageParam::OutputPenalty)].value,
        -10000
    );
}
#[test]
fn unsupported_page_features_and_infinite_shrink_fail_explicitly() {
    for source in [
        br"\insert0{}\end".as_slice(),
        br"\mark{}\end",
        br"\catcode36=3$x$\end",
        br"\vsize=100pt\hrule width1pt\vskip0pt minus1fil\par\end",
    ] {
        let mut e = Engine::new(source);
        assert_eq!(e.advance(100), Err(Error::Unsupported));
        assert!(!e.can_checkpoint());
    }
}
#[test]
fn vpackage_limits_final_depth_without_clipping_contents() {
    let mut e = Engine::new(b"");
    let mut list = List::default();
    let rule = e
        .document
        .nodes
        .alloc(Kind::Rule {
            width: Scaled(100),
            height: Scaled(10),
            depth: Scaled(7),
        })
        .unwrap();
    e.document.nodes.append(&mut list, rule);
    let boxed = e
        .vpackage(list, PackSpec::To(Scaled(20)), Scaled(2))
        .unwrap();
    let Kind::Box(b) = &e.document.nodes.get(boxed.id()).kind else {
        unreachable!()
    };
    let b = e.document.nodes.box_data(b);
    assert_eq!(
        (b.width, b.height, b.depth),
        (Scaled(100), Scaled(20), Scaled(2))
    );
    e.publish_page(boxed, 0).unwrap();
    assert_eq!(
        e.outcome()
            .pages
            .get(0)
            .unwrap()
            .placements()
            .next()
            .unwrap(),
        Placement::Rule {
            x: 0,
            y: 0,
            width: 100,
            height: 17
        }
    );
}
#[test]
fn new_internal_quantities_share_integer_dimension_and_glue_scanning() {
    let mut e=Engine::new(br"\catcode123=1\catcode125=2\vsize=20pt\hrule width1pt height2pt\par\count0=\pagegoal\dimen0=\pagetotal\prevdepth=7pt\count1=\prevdepth\penalty-12345\relax\ifnum\lastpenalty=10000\message{\number\outputpenalty/\number\pagegoal/\the\dimen0}\fi\vskip3pt plus2fil\par\skip0=\lastskip\message{\the\skip0}\end");
    complete(&mut e);
    assert_eq!(e.outcome().counts[0], 20 * 65536);
    assert_eq!(e.outcome().counts[1], 7 * 65536);
    assert_eq!(
        e.outcome().message_bytes(),
        vec![
            b"-12345/1073741823/2.0pt".to_vec(),
            b"3.0pt plus 2.0fil".to_vec()
        ]
    );
}
#[test]
fn invalid_prevdepth_mode_rejects_before_operand_and_nointerlineskip_is_undefined() {
    let mut e = Engine::new(br"\noindent\prevdepth=\count0 7\par\end");
    let read = e.next_expanded().unwrap();
    e.execute(read).unwrap();
    let read = e.next_expanded().unwrap();
    assert_eq!(e.execute(read), Err(Error::Syntax));
    assert_eq!(
        e.next_expanded().unwrap().token,
        crate::token::Token::other(b'=')
    );
    let mut e = Engine::new(br"\nointerlineskip\end");
    assert_eq!(e.advance(1), Err(Error::Undefined));
}
#[test]
fn first_accepted_page_freezes_mag_but_huge_rejection_does_not() {
    let mut e = Engine::new(br"\mag=1200\hrule width1pt\penalty-10000\mag=1300\dimen0=1truept\end");
    assert_eq!(e.advance(99), Err(Error::Number));
    assert_eq!(e.document.layout_values.prepared_mag, Some(1200));
    let mut e = Engine::new(br"\mag=1200\end");
    complete(&mut e);
    assert_eq!(e.document.layout_values.prepared_mag, None);
    let huge = e
        .document
        .nodes
        .alloc_box(BoxData {
            height: Scaled::MAX,
            depth: Scaled(1),
            ..BoxData::default()
        })
        .unwrap();
    assert_eq!(e.publish_page(huge, 0), Err(Error::Overflow));
    assert!(e.outcome().pages.is_empty());
    assert_eq!(e.document.layout_values.prepared_mag, None);
}

#[test]
fn end_and_vertical_commands_insert_the_current_par_token() {
    for trigger in ["\\end", "\\vskip2pt\\end", "\\hrule width1pt\\end"] {
        let source = format!(
            r"\catcode123=1\catcode125=2\let\realpar=\par\def\par{{\count0=7\realpar}}x{trigger}"
        );
        let mut e = Engine::new(source.as_bytes());
        complete(&mut e);
        assert_eq!(e.outcome().counts[0], 7);
    }
}

#[test]
fn output_font_binding_crosses_nested_boxes_and_resets_for_each_page() {
    fn boxed(e: &mut Engine<'_>, fonts: &[u32]) -> crate::nodes::DetachedNode {
        let mut list = List::default();
        for &font in fonts {
            let glyph = e
                .document
                .nodes
                .alloc(Kind::Character {
                    font,
                    character: b'a',
                })
                .unwrap();
            e.document.nodes.append(&mut list, glyph);
        }
        e.document
            .nodes
            .alloc_box(BoxData {
                list,
                ..BoxData::default()
            })
            .unwrap()
    }
    let mut e = engine(br"\font\f=test at10pt\font\g=test at20pt\font\h=other at10pt\end");
    e.register_tfm(b"other", &basic()).unwrap();
    complete(&mut e);
    // A distinct document-font ID can alias an already loaded immutable resource.
    e.document.fonts.descriptors[4] = e.document.fonts.descriptors[1];
    e.document.fonts.len = 5;
    let mut list = List::default();
    for fonts in [&[1, 1][..], &[2, 2, 4, 4, 2]] {
        let child = boxed(&mut e, fonts);
        e.document.nodes.append(&mut list, child);
    }
    // Returning from the mixed-font child resumes its parent's font.
    let glyph = e
        .document
        .nodes
        .alloc(Kind::Character {
            font: 1,
            character: b'a',
        })
        .unwrap();
    e.document.nodes.append(&mut list, glyph);
    let child = boxed(&mut e, &[3, 3]);
    e.document.nodes.append(&mut list, child);
    let outer = e
        .document
        .nodes
        .alloc_box(BoxData {
            list,
            ..BoxData::default()
        })
        .unwrap();
    e.publish_page(outer, 0).unwrap();
    // The last font on the first page is the first font on the next page.
    let next = boxed(&mut e, &[3, 3, 1, 4]);
    e.publish_page(next, 0).unwrap();
    let out = e.outcome();
    drop(e);
    let p = out.pages.get(0).unwrap();
    assert_eq!(
        p.fonts()
            .iter()
            .map(|f| (f.name(), f.size()))
            .collect::<Vec<_>>(),
        [
            (b"test".as_slice(), 655360),
            (b"test".as_slice(), 1310720),
            (b"other".as_slice(), 655360)
        ]
    );
    let indices = |p: &crate::Page| {
        p.placements()
            .map(|x| match x {
                Placement::Glyph { font, .. } => font,
                _ => panic!("expected glyph"),
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(indices(p), [0, 0, 1, 1, 0, 0, 1, 0, 2, 2]);
    let p = out.pages.get(1).unwrap();
    assert_eq!(p.fonts().len(), 2);
    assert_eq!(p.fonts()[0].name(), b"other");
    assert_eq!(p.fonts()[1].name(), b"test");
    assert_eq!(indices(p), [0, 0, 1, 1]);
}
