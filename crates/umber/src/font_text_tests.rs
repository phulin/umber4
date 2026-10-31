use crate::{
    Engine, Error, Progress,
    font_test_data::{basic, tfm},
    nodes::Kind,
    numeric::Scaled,
    text::Mode,
    token::*,
};
fn engine(source: &[u8]) -> Engine<'_> {
    let mut e = Engine::new(source);
    e.register_tfm(b"test", &basic()).unwrap();
    for (c, cat) in [(123, 1), (125, 2), (35, 6)] {
        e.document.scalars[crate::scalars::Target::catcode(c)].value = cat;
    }
    e
}
fn build(e: &mut Engine<'_>) {
    loop {
        let read = e.next_expanded().unwrap();
        if read.meaning.command == cmd::END {
            break;
        }
        e.execute(read).unwrap();
    }
}
fn glyphs(e: &Engine<'_>) -> Vec<(u8, bool)> {
    let mut p = e.document.text.horizontal.head();
    let mut out = Vec::new();
    while !p.is_null() {
        let n = e.document.nodes.get(p);
        match &n.kind {
            Kind::Character { character, .. } => out.push((*character, false)),
            Kind::Ligature { character, .. } => out.push((*character, true)),
            _ => {}
        }
        p = n.next;
    }
    out
}
#[test]
fn loaded_fonts_grouping_mutation_and_restore() {
    let mut e=engine(br"\font\a=test \a \fontdimen2\a=3pt {\nullfont\fontdimen2\a=4pt}\relax \font\b=test \font\c=test at 12pt \fontdimen8\c=9pt \c\end");
    e.advance(5).unwrap();
    let saved = e.checkpoint().unwrap();
    let fonts = e.document.fonts.len;
    while e.advance(1).unwrap() != Progress::Complete {}
    assert_eq!(e.document.fonts.len, 3);
    assert_eq!(e.font_parameter(1, 2), Scaled(4 * 65536));
    assert_eq!(e.font_parameter(2, 8), Scaled(9 * 65536));
    e.restore(&saved).unwrap();
    assert_eq!(e.document.fonts.len, fonts);
    assert_eq!(e.font_parameter(1, 2), Scaled(3 * 65536));
    while e.advance(1).unwrap() != Progress::Complete {}
    assert_eq!(e.document.fonts.len, 3);
    assert_eq!(e.font_parameter(2, 8), Scaled(9 * 65536));
    assert_eq!(e.document.fonts.current.value, 2);
}
#[test]
fn font_dimensions_units_and_global_parameter_effects() {
    let o=engine(br"\font\f=test \f \dimen0=1em \dimen1=1ex {\fontdimen2\f=7pt\hyphenchar\f=64}\message{\the\dimen0;\the\dimen1;\the\fontdimen2\f;\the\hyphenchar\f;\the\fontcharwd\f97}\end").run().unwrap();
    assert_eq!(o.messages[0], b"10.0pt;4.49998pt;7.0pt;64;7.5pt");
    let mut e = engine(br"\font\f=test \font\g=test at 12pt \fontdimen8\f=1pt\end");
    assert_eq!(e.advance(99), Err(Error::Number));
}
#[test]
fn resource_registration_seals_across_restore() {
    let mut e = engine(br"\end");
    let s = e.checkpoint().unwrap();
    assert_eq!(e.register_tfm(b"test.tfm", &basic()), Err(Error::Resource));
    e.advance(0).unwrap();
    e.register_tfm(b"other", &basic()).unwrap();
    e.advance(1).unwrap();
    e.restore(&s).unwrap();
    assert_eq!(e.register_tfm(b"new", &basic()), Err(Error::Resource));
}
#[test]
fn ligatures_kerns_expansion_char_and_hyphen_components() {
    let mut e = engine(
        br"\defaulthyphenchar=45\font\f=test \f\noindent\def\i{i}f\i ff\char105 AV a-b\relax\end",
    );
    build(&mut e);
    assert_eq!(
        glyphs(&e),
        vec![
            (12, true),
            (14, true),
            (b'A', false),
            (b'V', false),
            (b'a', false),
            (b'-', false),
            (b'b', false)
        ]
    );
    let mut p = e.document.text.horizontal.head();
    let mut kerns = 0;
    let mut discs = 0;
    let mut comp = 0;
    while !p.is_null() {
        let n = e.document.nodes.get(p);
        match &n.kind {
            Kind::Kern { width, explicit } => {
                assert!(!explicit);
                assert_eq!(width.0, -65537);
                kerns += 1;
            }
            Kind::Discretionary {
                pre,
                post,
                replace_count,
            } => {
                assert!(pre.is_empty() && post.is_empty());
                assert_eq!(*replace_count, 0);
                discs += 1;
            }
            Kind::Ligature { components, .. } => {
                let mut c = components.head();
                while !c.is_null() {
                    comp += 1;
                    c = e.document.nodes.get(c).next;
                }
            }
            _ => {}
        }
        p = n.next;
    }
    assert_eq!((kerns, discs, comp), (1, 1, 5));
    assert_eq!(e.document.text.mode, Mode::Horizontal);
    assert!(!e.can_checkpoint());
}
#[test]
fn spacing_sf_codes_and_control_spaces() {
    let cases:[(&[u8],i32,i32);4]=[
        (br"\font\f=test \f\noindent\sfcode`\.=3000 a. \end",291270,327678),
        (br"\font\f=test \f\noindent\sfcode`\.=3000 A. \end",218453,109226),
        (br"\font\f=test \f\noindent\spacefactor=2000\spaceskip=5pt plus 3pt minus 2pt\fontdimen7\f=1pt \ \end",5*65536,3*65536),
        (br"\font\f=test \f\noindent\xspaceskip=9pt\spacefactor=2000\relax{} \end",9*65536,0),
    ];
    for (source, width, stretch) in cases {
        let mut e = engine(source);
        build(&mut e);
        let mut p = e.document.text.horizontal.head();
        let mut glues = Vec::new();
        while !p.is_null() {
            let n = e.document.nodes.get(p);
            if let Kind::Glue { spec, .. } = &n.kind {
                glues.push(*spec);
            }
            p = n.next;
        }
        assert_eq!(glues.len(), 1);
        assert_eq!(glues[0].width.0, width);
        assert_eq!(glues[0].stretch.0, stretch);
    }
}
#[test]
fn boundary_ligatures_noboundary_and_retaining_operations() {
    for op in [0, 1, 2, 3, 5, 6, 7, 11] {
        let program = [[128, b'b', op, b'c']];
        let mut e = Engine::new(br"\font\f=test \f\noindent ab\relax\end");
        e.register_tfm(b"test", &tfm(&program, &[(b'a', 1, 0)]))
            .unwrap();
        build(&mut e);
        let chars: Vec<u8> = glyphs(&e).iter().map(|x| x.0).collect();
        let expected = match op {
            0 => vec![b'c'],
            1 | 5 => vec![b'c', b'b'],
            2 | 6 => vec![b'a', b'c'],
            _ => vec![b'a', b'c', b'b'],
        };
        assert_eq!(chars, expected, "op {op}");
    }
    let program = [
        [255, b'|', 0, 1],
        [128, b'a', 0, b'c'],
        [128, b'|', 0, b'd'],
        [255, 0, 0, 1],
    ];
    let bytes = tfm(&program, &[(b'a', 1, 2)]);
    let mut e = Engine::new(br"\font\f=test \f\noindent a\relax\noboundary a\noboundary\relax\end");
    e.register_tfm(b"test", &bytes).unwrap();
    build(&mut e);
    assert_eq!(glyphs(&e), vec![(b'c', true), (b'a', false)]);
    let first = e.document.nodes.get(e.document.text.horizontal.head());
    assert!(matches!(
        &first.kind,
        Kind::Ligature {
            left_boundary: true,
            ..
        }
    ));
}
#[test]
fn paragraph_end_transfers_nodes_and_restore_returns_to_vertical_root() {
    let mut e = engine(br"\font\f=test \f\noindent a\end");
    assert_eq!(e.advance(1).unwrap(), Progress::Boundary);
    let saved = e.checkpoint().unwrap();
    assert_eq!(e.advance(100), Ok(Progress::Complete));
    assert!(e.document.text.horizontal.is_empty());
    assert!(e.document.text.vertical.is_empty());
    assert!(!e.document.page.output.is_empty());
    assert!(e.can_checkpoint());
    e.restore(&saved).unwrap();
    assert_eq!(e.document.text.mode, Mode::Vertical);
    assert!(e.document.text.vertical.is_empty());
    let mut empty = Engine::new(br"\noindent\par\end");
    assert_eq!(empty.advance(1).unwrap(), Progress::Boundary);
}

#[test]
fn false_boundary_missing_glyphs_and_nullfont_preserve_input() {
    let bytes = tfm(&[[255, 200, 0, 1], [128, 200, 0, b'd']], &[(b'a', 1, 1)]);
    let mut e = Engine::new(br"\font\f=test \f\noindent a\relax a\char200a\relax\end");
    e.register_tfm(b"test", &bytes).unwrap();
    build(&mut e);
    assert_eq!(glyphs(&e), vec![(b'd', true), (b'a', false), (b'd', true)]);
    assert!(matches!(
        &e.document.nodes.get(e.document.text.horizontal.head()).kind,
        Kind::Ligature {
            right_boundary: true,
            ..
        }
    ));
    let mut e = engine(br"\noindent a\font\f=test \f\char0b\relax\end");
    build(&mut e);
    assert_eq!(glyphs(&e), vec![(b'b', false)]);
}
#[test]
fn everypar_precedes_implicit_character_and_budget_waits_for_vertical_mode() {
    let mut e = engine(br"\everypar{A}\font\f=test \f fi\relax\end");
    build(&mut e);
    assert_eq!(glyphs(&e), vec![(b'A', false), (12, true)]);
    assert!(matches!(
        &e.document.nodes.get(e.document.text.horizontal.head()).kind,
        Kind::Box(_)
    ));
    let mut e = engine(br"\everypar{\count0=7}\noindent\par\end");
    e.advance(1).unwrap();
    assert_eq!(e.advance(1).unwrap(), Progress::Boundary);
    assert_eq!(
        e.document.scalars[crate::scalars::Target::count(0)].value,
        7
    );
    assert!(e.can_checkpoint());
}
#[test]
fn unsupported_internal_font_tokens_are_explicit() {
    let mut e = engine(br"\font\f=test \f\message{\the\font}\end");
    assert_eq!(e.advance(100), Err(Error::Unsupported));
}

#[test]
fn same_source_initex_font_defaults() {
    let o = Engine::new(include_bytes!("../../../fixtures/mvp/font-defaults.tex"))
        .run()
        .unwrap();
    assert_eq!(
        o.messages,
        [
            b"UMBER:0.0pt/0.0pt/45/-1:END".as_slice(),
            b"UMBER:0.0pt/0.0pt/0/0/999/1000:END",
            b"UMBER:12.0pt/3.0pt/5.0pt/64/1:END"
        ]
        .map(Vec::from)
    );
}

#[test]
fn abandoned_font_mutations_do_not_pollute_cached_metric_baselines() {
    let mut e = engine(
        br"\font\a=test \relax \font\b=test at12pt \fontdimen2\b=99pt \fontdimen8\b=9pt \end",
    );
    e.advance(2).unwrap();
    let saved = e.checkpoint().unwrap();
    while e.advance(1).unwrap() != Progress::Complete {}
    assert_eq!(e.document.fonts.descriptors[2].count, 8);
    assert_eq!(e.font_parameter(2, 2), Scaled(99 * 65536));
    e.restore(&saved).unwrap();
    e.advance(1).unwrap();
    assert_eq!(e.document.fonts.descriptors[2].count, 7);
    assert_eq!(e.font_parameter(2, 2), Scaled(262143));
}
