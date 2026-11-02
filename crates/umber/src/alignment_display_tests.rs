//! Direct displays keep alignment layout separate from formula conversion.
use super::*;
const DISPLAY: &str = r"\catcode36=3\abovedisplayskip=6pt plus1pt\belowdisplayskip=7pt plus2pt\abovedisplayshortskip=1pt\belowdisplayshortskip=2pt\predisplaypenalty=17\postdisplaypenalty=19 ";
const CASES: &[&str] = &[
    r"\setbox0=\vbox{\noindent$$\displayindent=3pt\halign{#\cr a\cr\noalign{\hrule height1pt\kern0pt}}$$ab\par}\noindent\box0\par",
    r"\noindent$$\halign{#\cr}$$ab\par",
    r"\noindent$$\halign{#&#\cr ab&cd\cr e&f\cr}$$ab\par",
    r"ab$$\tabskip=0pt plus1fil\halign to\displaywidth{#&#\cr a&b\cr c&d\cr}$$ef\par",
    r"\noindent$$\displayindent=3pt\halign{\displayindent=9pt#\cr a\cr\noalign{\hrule height1pt\moveleft2pt\hbox{b}}c\cr}\displayindent=11pt\abovedisplayskip=8pt\belowdisplayskip=9pt$$ab\par",
    r"\noindent$$\displayindent=-3pt\halign{#&#\cr\noalign{\hrule height1pt}\hbox{a}\span b\cr\noalign{\hrule width10pt height2pt depth1pt}}$$ab\par",
    r"\noindent$$\halign{#\cr\noalign{\hrule height2pt}}$$ab\par",
    r"\noindent$$\displayindent=2pt\halign{#\cr a\cr\noalign{\hrule height1pt\hrule height2pt\hrule height3pt}}$$ab\par",
    r"\noindent$$\halign{#\cr a\cr\noalign{\hbox{\vrule height2pt depth5pt}}}$$ab\par",
    r"\def\dollar{\ifmmode\ifinner\undefined\else$\fi\else\undefined\fi}\noindent$$\halign{#\cr a\cr}\count0=3\dollar\dollar\message{UMBER:C:\the\count0:END}ab\par",
    r"\def\after{\belowdisplayskip=9pt}\noindent$$\halign{#\cr a\cr\noalign{\afterassignment\after}}\count0=3$$ab\par",
    r"\def\late{\noalign{\global\displayindent=6pt}}\noindent$$\halign{#\cr a\aftergroup\late\cr}$$ab\par",
    r"\setbox0=\vbox{\prevdepth=3pt\noindent$$\displayindent=2pt\halign{#\cr a\cr\noalign{\prevdepth=5pt}}$$\par\message{UMBER:DEPTH:\the\prevdepth:END}}\noindent\box0\par",
    r"\noindent$$\halign{#\cr\vbox{\noindent$$\halign{#\cr a\cr}$$b\par}\cr}$$c\par",
    r"\def\blank{ }\everypar{\message{UMBER:PAR:END}}\noindent$$\halign{#\cr a\cr}$$\blank ab\par",
];
fn source(body: &str) -> String {
    format!(r"{BOOT}\font\f=test\f {SETTINGS}{DISPLAY}{body}\end")
}

#[test]
fn direct_alignment_displays_use_no_math_fonts_and_release_all_construction_roots() {
    // basic.tfm has seven parameters and no assigned math fonts. Text-only
    // alignment cells therefore prove that whole-formula validation is absent.
    for body in CASES {
        let source = source(body);
        let mut e = engine(source.as_bytes());
        assert_eq!(e.advance(usize::MAX), Ok(Progress::Complete), "{body}");
        assert!(e.document.math_frames.is_empty());
        assert!(e.document.alignment_frames.is_empty());
        assert!(e.document.alignment_layouts.is_empty());
        assert!(!e.alignment_reader.active());
        assert_eq!(e.document.nodes.live_count(), 0);
    }
}

#[test]
fn display_alignment_failure_releases_completed_body_and_restores_other_roots() {
    for (body, error, next) in [
        (
            r"\noindent$$a\halign\message{UNREAD}",
            Error::Syntax,
            cmd::MESSAGE,
        ),
        (
            r"\noindent$$\over\halign\message{UNREAD}",
            Error::Syntax,
            cmd::MESSAGE,
        ),
        (
            r"\noindent$$\halign{#\cr\hbox{a}\cr}\message{UNREAD}",
            Error::Syntax,
            cmd::MESSAGE,
        ),
        (
            r"\noindent$$\halign{#\cr a\cr}$\message{UNREAD}",
            Error::Syntax,
            cmd::MESSAGE,
        ),
        (
            r"\noindent$$\halign{#\cr a\cr}\setbox1=\hbox{b}",
            Error::Syntax,
            cmd::MAKE_BOX,
        ),
        (
            r"\noindent$$\halign{#\cr a\cr}\global\setbox1=\hbox{b}",
            Error::Syntax,
            cmd::MAKE_BOX,
        ),
        (
            r"\noindent$$\begingroup\halign{#\cr a\cr}$$\message{UNREAD}",
            Error::Group,
            cmd::MESSAGE,
        ),
        (
            r"\noindent$$\halign{#\cr a\cr}\eqno\message{UNREAD}",
            Error::Syntax,
            cmd::EQNO,
        ),
        (
            r"\noindent$$\halign{#\cr a\cr}\count0=\undefined\message{UNREAD}",
            Error::Undefined,
            cmd::MESSAGE,
        ),
    ] {
        let source = source(&format!(r"\setbox0=\hbox{{z}}{body}"));
        let mut e = engine(source.as_bytes());
        let checkpoint = e.checkpoint().unwrap();
        let mut capacity = None;
        for _ in 0..3 {
            assert_eq!(e.advance(usize::MAX), Err(error), "{body}");
            assert_eq!(e.next_token().unwrap().meaning.command, next, "{body}");
            assert!(!e.document.boxes.head(0).is_null());
            assert!(e.document.boxes.head(1).is_null());
            assert_eq!(e.document.math_frames.len(), 1);
            assert_eq!(e.document.text.mode, Mode::DisplayMath);
            assert!(!e.can_checkpoint());
            if body.contains(r"\cr}") {
                assert!(e.document.alignment_frames.is_empty());
                assert!(e.document.alignment_layouts.is_empty());
                assert!(!e.alignment_reader.active());
                assert_eq!(e.document.nodes.live_count(), 2);
            }
            let foreign = Engine::new(b"").checkpoint().unwrap();
            assert_eq!(e.restore(&foreign), Err(Error::Checkpoint));
            assert!(!e.document.boxes.head(0).is_null());
            e.restore(&checkpoint).unwrap();
            assert_eq!(e.document.nodes.live_count(), 0);
            assert!(e.document.math_frames.is_empty());
            let next_capacity = (
                e.alignment_execution_pool.len(),
                e.alignment_layout_pool.len(),
            );
            if let Some(capacity) = capacity {
                assert_eq!(next_capacity, capacity);
            }
            capacity = Some(next_capacity);
        }
    }
}

#[test]
fn internal_shifted_terminal_rule_is_explicitly_bounded_with_rooted_failure_and_restore() {
    // Native keeps the contribution append position inside the final wrapper
    // even through subsequent paragraphs. A following kern avoids that native
    // cross-chain tail and is exercised among the supported cases above.
    let source = source(
        r"\setbox9=\hbox{z}\setbox0=\vbox{\noindent$$\displayindent=3pt\halign{#\cr a\cr\noalign{\hrule height1pt}}$$\par\noindent ab\par\hbox{c}}\par",
    );
    let mut e = engine(source.as_bytes());
    let checkpoint = e.checkpoint().unwrap();
    let mut capacities = None;
    for _ in 0..3 {
        assert_eq!(e.advance(usize::MAX), Err(Error::Unsupported));
        assert!(!e.document.boxes.head(9).is_null());
        assert_eq!(e.document.math_frames.len(), 1);
        assert_eq!(e.document.alignment_frames.len(), 1);
        assert!(e.document.alignment_layouts.is_empty());
        assert!(e.alignment_reader.active());
        assert!(!e.document.text.vertical.is_empty());
        assert!(!e.can_checkpoint());
        let foreign = Engine::new(b"").checkpoint().unwrap();
        assert_eq!(e.restore(&foreign), Err(Error::Checkpoint));
        assert!(!e.document.boxes.head(9).is_null());
        e.restore(&checkpoint).unwrap();
        assert!(e.document.math_frames.is_empty());
        assert!(e.document.alignment_frames.is_empty());
        assert!(!e.alignment_reader.active());
        assert_eq!(e.document.nodes.live_count(), 0);
        let next = (
            e.alignment_layout_pool.len(),
            e.alignment_execution_pool.len(),
        );
        if let Some(capacities) = capacities {
            assert_eq!(next, capacities);
        }
        capacities = Some(next);
    }
}

#[test]
fn display_alignment_continuation_preserves_parent_depth_and_line_count() {
    let source =
        source(r"\noindent$$\displayindent=3pt\halign{#\cr a\cr\noalign{\prevdepth=5pt}}$$\relax");
    let mut e = engine(source.as_bytes());
    loop {
        let read = e.next_expanded().unwrap();
        if read.meaning.command == cmd::RELAX
            && e.document.text.mode == Mode::Horizontal
            && e.document.math_frames.is_empty()
        {
            break;
        }
        e.execute(read).unwrap();
    }
    assert_eq!(e.document.text.prev_graf, 3);
    assert_eq!(e.document.text.prev_depth, Scaled(5 * 65536));
    assert_eq!(e.document.text.space_factor, 1000);
    assert_eq!(e.document.level, 0);
    assert!(e.document.text.horizontal.is_empty());
}

#[test]
#[ignore = "native direct display halign: set UMBER_PDFTEX"]
fn upstream_direct_display_alignment_geometry_order_and_scanning() {
    let oracle = crate::page_oracle_tests::Oracle::new();
    for body in CASES {
        oracle.compare(&basic(), &format!("{BOOT}{SETTINGS}{DISPLAY}"), body);
    }
}
