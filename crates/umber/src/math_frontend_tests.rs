use super::*;
use crate::Progress;

const SETUP: &str = r"\catcode36=3\catcode94=7\catcode95=8\catcode35=6
\font\s=test at7pt\font\ss=test at5pt
\textfont0=\f\textfont1=\f\textfont2=\f\textfont3=\f
\scriptfont0=\s\scriptfont1=\s\scriptfont2=\s\scriptfont3=\s
\scriptscriptfont0=\ss\scriptscriptfont1=\ss\scriptscriptfont2=\ss\scriptscriptfont3=\ss
\thinmuskip=3mu\medmuskip=4mu plus2mu minus4mu\thickmuskip=5mu plus5mu\scriptspace=.5pt ";
fn metrics() -> Vec<u8> {
    let mut bytes = crate::font_test_data::basic();
    let words = u16::from_be_bytes([bytes[0], bytes[1]]) + 15;
    bytes[..2].copy_from_slice(&words.to_be_bytes());
    bytes[22..24].copy_from_slice(&22u16.to_be_bytes());
    for value in [
        700000i32, 400000, 300000, 680000, 350000, 420000, 370000, 290000, 160000, 260000, 100000,
        60000, 40000, 60000, 250000,
    ] {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    bytes
}
fn input(body: &str) -> String {
    format!(r"\catcode123=1\catcode125=2\font\f=test\f {SETUP}{body}")
}
fn engine(source: &[u8]) -> Engine<'_> {
    let mut e = Engine::new(source);
    e.register_tfm(b"test", &metrics()).unwrap();
    e
}
fn build(e: &mut Engine<'_>) -> Result<()> {
    loop {
        let read = e.next_expanded()?;
        if read.meaning.command == cmd::END {
            return Ok(());
        }
        e.execute(read)?;
    }
}
fn kinds<'e>(e: &'e Engine<'_>, list: &nodes::List) -> Vec<&'e nodes::Kind> {
    let mut p = list.head();
    let mut out = Vec::new();
    while !p.is_null() {
        let n = e.document.nodes.get(p);
        out.push(&n.kind);
        p = n.next;
    }
    out
}
fn glyphs(e: &Engine<'_>, list: &nodes::List) -> Vec<(u32, u8)> {
    let mut work = vec![list.head()];
    let mut out = Vec::new();
    while let Some(mut p) = work.pop() {
        while !p.is_null() {
            let n = e.document.nodes.get(p);
            match &n.kind {
                nodes::Kind::Character { font, character }
                | nodes::Kind::Ligature {
                    font, character, ..
                } => out.push((*font, *character)),
                nodes::Kind::Box(b) => work.push(e.document.nodes.box_data(b).list.head()),
                _ => {}
            }
            p = n.next;
        }
    }
    out
}

#[test]
fn inline_entry_uses_unexpanded_lookahead_hooks_and_restores_family_and_surround() {
    let source = input(
        r"\fam=9\mathsurround=1pt\everymath={\ifmmode\message{M}\else\message{BAD}\fi\message{\the\fam}}
\noindent$\ifmmode a\else b\fi\mathsurround=-2pt\fam=0$\message{\the\fam/\the\mathsurround/\the\spacefactor}\end",
    );
    let mut e = engine(source.as_bytes());
    build(&mut e).unwrap();
    assert_eq!(
        e.outcome().message_bytes(),
        [b"M".to_vec(), b"-1".to_vec(), b"9/1.0pt/1000".to_vec()]
    );
    let boundaries: Vec<_> = kinds(&e, &e.document.text.horizontal)
        .into_iter()
        .filter_map(|k| {
            if let nodes::Kind::MathBoundary { width, after } = k {
                Some((width.0, *after))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(boundaries, [(-2 * 65536, false), (-2 * 65536, true)]);
    assert_eq!(glyphs(&e, &e.document.text.horizontal), [(1, b'a')]);
    assert!(e.document.math_frames.is_empty());
    assert_eq!(e.math.as_ref().unwrap().live_count(), 0);
}

#[test]
fn field_targets_capture_families_unwrap_ords_and_preserve_empty_scripts() {
    let source =
        input(r#"\noindent$\fam=1 a\fam=0\mathchar"7062 {}^{}\mathord{c}_{\mathchar100 }$\end"#);
    let mut e = engine(source.as_bytes());
    build(&mut e).unwrap();
    assert_eq!(e.math.as_ref().unwrap().live_count(), 0);
    assert!(e.math_scratch.as_ref().unwrap().is_empty());
    assert!(!e.document.text.horizontal.is_empty());
    assert_eq!(e.document.level, 0);
}

#[test]
fn mathactive_characters_expand_in_list_and_immediate_field_scans() {
    let source = input(
        r#"\catcode126=13\outer\def~{\mathchar97 }\catcode126=12\mathcode126="8000\noindent$~^~\mathop~\limits_~$\end"#,
    );
    let mut e = engine(source.as_bytes());
    build(&mut e).unwrap();
    assert_eq!(
        glyphs(&e, &e.document.text.horizontal)
            .iter()
            .map(|(_, c)| *c)
            .collect::<Vec<_>>(),
        b"aaaa"
    );
    assert_eq!(e.math.as_ref().unwrap().live_count(), 0);
}

#[test]
fn nested_math_boxes_void_transfers_and_field_groups_keep_parent_ids_live() {
    let source = input(
        r"\setbox0=\hbox{b}\noindent$a^{\hbox{c$^{d}$}}\copy0\raise2pt\box0\box0\hbox{$$}$\end",
    );
    let mut e = engine(source.as_bytes());
    build(&mut e).unwrap();
    assert!(e.document.boxes.head(0).is_null());
    assert!(e.document.math_frames.is_empty());
    assert!(e.document.box_frames.is_empty());
    assert_eq!(e.math.as_ref().unwrap().live_count(), 0);
    assert_eq!(e.document.level, 0);
    let list = std::mem::take(&mut e.document.text.horizontal);
    e.document
        .nodes
        .release(list, &mut nodes::ReleaseScratch::default());
    assert_eq!(e.document.nodes.live_count(), 0);
}

#[test]
fn math_scripts_follow_the_immediate_tail_and_duplicate_checks_precede_scanning() {
    let source = input(r"\noindent$a\kern1pt^b\scriptstyle_c$\end");
    let mut e = engine(source.as_bytes());
    build(&mut e).unwrap();
    assert_eq!(e.math.as_ref().unwrap().live_count(), 0);
    let source = input(r"\noindent$a^{}^\message{NOT-SCANNED}$\end");
    let mut e = engine(source.as_bytes());
    assert_eq!(build(&mut e), Err(Error::Syntax));
    assert!(e.outcome().diagnostics.is_empty());
    assert_eq!(e.next_expanded().unwrap().meaning.command, cmd::MESSAGE);
}

#[test]
fn malformed_closers_preserve_group_and_field_owners_until_restore() {
    for body in [
        r"\noindent${a$",
        r"\noindent$a}",
        r"\noindent$a\endgroup",
        r"\noindent$\begingroup a$",
        r"\noindent$a^{b\endgroup",
    ] {
        let source = input(body);
        let mut e = engine(source.as_bytes());
        let initial = e.checkpoint().unwrap();
        assert_eq!(e.advance(usize::MAX), Err(Error::Group), "{body}");
        assert!(!e.can_checkpoint());
        let level = e.document.level;
        assert!(level > 0);
        assert!(e.math.as_ref().unwrap().live_count() > 0);
        assert_eq!(
            e.current_group(),
            Some(if body.contains("begingroup") {
                GroupKind::SemiSimple
            } else if body.contains('{') {
                GroupKind::MathField
            } else {
                GroupKind::MathShift
            })
        );
        e.restore(&initial).unwrap();
        assert_eq!(e.math.as_ref().unwrap().live_count(), 0);
        assert!(e.document.math_frames.is_empty());
        assert_eq!(e.document.nodes.live_count(), 0);
        assert!(e.can_checkpoint());
    }
}

#[test]
fn failed_inner_conversion_preserves_outer_math_and_bulk_restore_abandons_old_ids() {
    for inner in [r"\textfont2=\nullfont b", r#"\hbox{b}^c\mathchar"0F63"#] {
        let source = input(&format!(
            r"\setbox0=\hbox{{z}}\noindent$a\hbox{{${inner}$}}$\par\end"
        ));
        let mut e = engine(source.as_bytes());
        let initial = e.checkpoint().unwrap();
        assert_eq!(e.advance(usize::MAX), Err(Error::Font));
        assert_eq!(e.document.math_frames.len(), 1);
        assert_eq!(e.math.as_ref().unwrap().live_count(), 1);
        assert!(e.math_scratch.as_ref().unwrap().is_empty());
        assert_eq!(
            e.document.nodes.live_count(),
            2,
            "only the unrelated register box survives"
        );
        let foreign = Engine::new(b"\\end").checkpoint().unwrap();
        assert_eq!(e.restore(&foreign), Err(Error::Checkpoint));
        assert_eq!(e.math.as_ref().unwrap().live_count(), 1);
        e.restore(&initial).unwrap();
        assert_eq!(e.math.as_ref().unwrap().live_count(), 0);
        assert_eq!(e.document.nodes.live_count(), 0);
        assert!(e.document.box_frames.is_empty());
        assert!(e.document.math_frames.is_empty());
    }
}

#[test]
fn quiescent_budget_defers_across_math_and_replay_preserves_published_output() {
    let source = input(
        r"\vsize=100pt\hsize=50pt\parfillskip=0pt plus1fil\noindent$^{1}$a$bc$\par\relax\end",
    );
    let mut e = engine(source.as_bytes());
    while e.document.text.prev_graf == 0 {
        assert_eq!(e.advance(1), Ok(Progress::Boundary));
    }
    assert!(e.can_checkpoint());
    let checkpoint = e.checkpoint().unwrap();
    e.advance(usize::MAX).unwrap();
    let first: Vec<_> = e
        .outcome()
        .pages
        .iter()
        .flat_map(|p| p.placements().collect::<Vec<_>>())
        .collect();
    e.restore(&checkpoint).unwrap();
    e.advance(usize::MAX).unwrap();
    let second: Vec<_> = e
        .outcome()
        .pages
        .iter()
        .flat_map(|p| p.placements().collect::<Vec<_>>())
        .collect();
    assert_eq!(first, second);
    assert!(!first.is_empty());
}

#[test]
fn restricted_double_shift_is_empty_inline() {
    let source = input(r"\setbox0=\hbox{$$}\end");
    let mut e = engine(source.as_bytes());
    e.advance(usize::MAX).unwrap();
    assert!(!e.document.boxes.head(0).is_null());
}

#[test]
fn lastbox_rejects_math_in_every_box_context_without_taking_the_destination() {
    for command in [
        r"\lastbox",
        r"\setbox0=\lastbox",
        r"\shipout\lastbox",
        r"\raise1pt\lastbox",
    ] {
        let source = input(&format!(r"\setbox0=\hbox{{b}}\noindent$a{command}"));
        let mut e = engine(source.as_bytes());
        assert_eq!(e.advance(usize::MAX), Err(Error::Syntax), "{command}");
        assert!(!e.document.boxes.head(0).is_null());
        assert_eq!(e.math.as_ref().unwrap().live_count(), 1);
        assert!(e.outcome().pages.is_empty());
    }
}

#[path = "math_choice_frontend_tests.rs"]
mod choices;
#[path = "math_delimiter_frontend_tests.rs"]
mod delimiters;
#[path = "math_display_tests.rs"]
mod displays;
#[path = "math_forms_frontend_tests.rs"]
mod forms;
#[path = "math_fraction_frontend_tests.rs"]
mod fractions;
#[path = "math_mu_frontend_tests.rs"]
mod mu;
#[path = "math_frontend_oracle_tests.rs"]
mod oracle;
