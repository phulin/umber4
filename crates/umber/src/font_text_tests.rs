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
fn discretionaries(e: &Engine<'_>) -> usize {
    let mut p = e.document.text.horizontal.head();
    let mut count = 0;
    while !p.is_null() {
        let node = e.document.nodes.get(p);
        count += usize::from(matches!(node.kind, Kind::Discretionary { .. }));
        p = node.next;
    }
    count
}
#[test]
fn borrowed_metrics_switch_fonts_and_check_each_character() {
    let mut e = engine(br"\font\a=test\font\b=test at12pt\end");
    build(&mut e);
    let mut metrics = e.metric_reader();
    assert_eq!(metrics.get(1, b'a').unwrap().width, Scaled(491520));
    // A cached font must still reject absent and out-of-range characters.
    assert!(metrics.get(1, 0).is_none());
    assert!(metrics.get(1, 200).is_none());
    assert_eq!(metrics.get(1, b'A').unwrap().width, Scaled(163840));
    assert_eq!(metrics.get(2, b'a').unwrap().width, Scaled(589824));
    assert_eq!(metrics.get(1, b'a').unwrap().width, Scaled(491520));
    assert!(metrics.get(0, b'a').is_none());
    assert!(metrics.get(u32::MAX, b'a').is_none());
    assert_eq!(metrics.get(2, b'A').unwrap().width, Scaled(196608));
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
        for hyphen in *b"abc" {
            let program = [[128, b'b', op, b'c']];
            let source =
                format!("\\defaulthyphenchar={hyphen}\\font\\f=test \\f\\noindent ab\\relax\\end");
            let mut e = Engine::new(source.as_bytes());
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
            // Breaks follow original components, including retained characters,
            // rather than the replacement glyph c or an empty inserted ligature.
            let expected = usize::from(hyphen == b'b' || (hyphen == b'a' && op != 0));
            assert_eq!(discretionaries(&e), expected, "op {op}, hyphen {hyphen}");
        }
    }
    let program = [
        [255, b'|', 0, 1],
        [128, b'a', 0, b'c'],
        [128, b'|', 0, b'd'],
        [255, 0, 0, 1],
    ];
    let bytes = tfm(&program, &[(b'a', 1, 2)]);
    let mut e = Engine::new(
        br"\defaulthyphenchar=97\font\f=test \f\noindent a\relax\noboundary a\noboundary\relax\end",
    );
    e.register_tfm(b"test", &bytes).unwrap();
    build(&mut e);
    assert_eq!(glyphs(&e), vec![(b'c', true), (b'a', false)]);
    assert_eq!(discretionaries(&e), 2);
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
    let mut e =
        Engine::new(br"\defaulthyphenchar=97\font\f=test \f\noindent a\relax a\char200a\relax\end");
    e.register_tfm(b"test", &bytes).unwrap();
    build(&mut e);
    assert_eq!(glyphs(&e), vec![(b'd', true), (b'a', false), (b'd', true)]);
    assert_eq!(discretionaries(&e), 3);
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
fn missing_only_character_runs_preserve_following_assignments() {
    let mut nullfont = Engine::new(br"\noindent abc\relax\count0=7\end");
    build(&mut nullfont);
    assert!(nullfont.document.text.horizontal.is_empty());
    assert_eq!(nullfont.outcome().counts[0], 7);

    let mut missing = engine(br"\font\f=test \f\noindent\char0\char200\relax\count0=7\end");
    build(&mut missing);
    assert!(missing.document.text.horizontal.is_empty());
    assert_eq!(missing.outcome().counts[0], 7);
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

#[test]
fn literal_character_reads_preserve_dynamic_categories_and_line_state() {
    let cases: &[(&[u8], &[u8])] = &[
        (br"\catcode64=11\relax a@b", b"a@b"),
        (br"\catcode64=13\relax\def@{b}a@c", b"abc"),
        (br"\catcode94=7\relax a^^62c", b"abc"),
        (br"\catcode94=7\def\m{\catcode94=12\relax}a\m^^2bb", b"a+b"),
        (
            br"\catcode64=11\relax a@{\catcode64=13\relax\def@{b}a@c}a@",
            b"a@abca@",
        ),
        (br"\catcode126=9\relax a~b", b"ab"),
        (b"a% discard\n b", b"ab"),
        (b"a\nb", b"ab"),
        (b"\\endlinechar=98\\relax a\nc\n", b"acb"),
    ];
    for &(body, expected) in cases {
        let mut source = br"\font\f=test \f\noindent ".to_vec();
        source.extend_from_slice(body);
        source.extend_from_slice(br"\relax\end\relax");
        let mut e = engine(&source);
        build(&mut e);
        assert_eq!(
            glyphs(&e),
            expected.iter().map(|&c| (c, false)).collect::<Vec<_>>(),
            "{}",
            String::from_utf8_lossy(body)
        );
    }
    for (body, spaces) in [(b"a% discard\n b".as_slice(), 0), (b"a\nb", 1)] {
        let mut source = br"\font\f=test \f\noindent ".to_vec();
        source.extend_from_slice(body);
        source.extend_from_slice(br"\relax\end\relax");
        let mut e = engine(&source);
        build(&mut e);
        let mut p = e.document.text.horizontal.head();
        let mut actual = 0;
        while !p.is_null() {
            let node = e.document.nodes.get(p);
            actual += usize::from(matches!(node.kind, Kind::Glue { .. }));
            p = node.next;
        }
        assert_eq!(actual, spaces);
    }
}

#[test]
fn literal_character_reads_preserve_aliases_and_nested_argument_boundaries() {
    let cases: &[(&[u8], &[u8])] = &[
        (br"\let\letter=b a\letter c", b"abc"),
        (br"\chardef\given=98 a\given c", b"abc"),
        (br"\def\id#1{#1}\def\m#1{a\id{#1}b}\m{C}d", b"aCbd"),
        (br"\def\empty{}\def\m#1{a#1b}\m{\empty}c", b"abc"),
        (
            br"\catcode64=11\def\m{a@b}\catcode64=13\relax\def@{C}\m a@b",
            b"a@baCb",
        ),
    ];
    for &(body, expected) in cases {
        let mut source = br"\font\f=test \f\noindent ".to_vec();
        source.extend_from_slice(body);
        source.extend_from_slice(br"\relax\end\relax");
        let mut e = engine(&source);
        build(&mut e);
        assert_eq!(
            glyphs(&e),
            expected.iter().map(|&c| (c, false)).collect::<Vec<_>>()
        );
    }
}

#[test]
fn literal_character_reads_preserve_missing_character_and_error_consumption() {
    let mut source = br"\font\f=test \f\noindent\sfcode0=2000\catcode0=12 a".to_vec();
    source.extend_from_slice(b"\0b\\relax\\end");
    let mut e = engine(&source);
    loop {
        let read = e.next_expanded().unwrap();
        let first = read.meaning.command == 11 && read.meaning.value == i32::from(b'a');
        e.execute(read).unwrap();
        if first {
            break;
        }
    }
    assert_eq!(glyphs(&e), vec![(b'a', false)]);
    assert_eq!(e.document.text.space_factor, 2000);
    assert_eq!(e.next_token().unwrap().token, Token::character(11, b'b'));
    let mut e = engine(br"\font\f=test \f\noindent\catcode33=15\relax abc!d");
    assert_eq!(e.advance(usize::MAX), Err(Error::Syntax));
    assert_eq!(e.next_token().unwrap().token, Token::character(11, b'd'));
    let mut e = engine(br"\font\f=test \f\noindent abc");
    assert_eq!(e.advance(usize::MAX), Err(Error::Eof));
    assert_eq!(
        glyphs(&e),
        vec![(b'a', false), (b'b', false), (b'c', false)]
    );
}
