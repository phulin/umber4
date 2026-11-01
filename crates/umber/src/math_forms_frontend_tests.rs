use super::*;

#[test]
fn forms_are_rooted_before_selector_and_field_scan_failures() {
    for (body, error, next, live) in [
        (
            r"\radical-1\message{UNREAD}",
            Error::Number,
            cmd::MESSAGE,
            2,
        ),
        (
            r"\radical134217728\message{UNREAD}",
            Error::Number,
            cmd::MESSAGE,
            2,
        ),
        (
            r"\mathaccent-1\message{UNREAD}",
            Error::Number,
            cmd::MESSAGE,
            2,
        ),
        (
            r"\mathaccent32768\message{UNREAD}",
            Error::Number,
            cmd::MESSAGE,
            2,
        ),
        (r"\radical\delimiter0", Error::Number, cmd::DELIMITER, 2),
        (r"\overline\hbox{}", Error::Syntax, cmd::BEGIN, 2),
        (r"\mathaccent94 \hbox{}", Error::Syntax, cmd::BEGIN, 2),
        (r"\accent97 b", Error::Syntax, 12, 1),
    ] {
        let source = input(&format!(r"\setbox0=\hbox{{z}}\noindent$a{body}"));
        let mut e = engine(source.as_bytes());
        let checkpoint = e.checkpoint().unwrap();
        assert_eq!(e.advance(usize::MAX), Err(error), "{body}");
        assert_eq!(e.math.as_ref().unwrap().live_count(), live);
        assert_eq!(e.next_expanded().unwrap().meaning.command, next);
        assert_eq!(e.document.nodes.live_count(), 2);
        e.restore(&checkpoint).unwrap();
        assert_eq!(e.document.nodes.live_count(), 0);
        assert_eq!(e.math.as_ref().unwrap().live_count(), 0);
    }
}

#[test]
fn accent_fold_keeps_existing_scripts_and_form_limits_stay_op_only() {
    for body in [
        r"{\mathaccent94 a^b}^c",
        r"{{\mathaccent94 a_b}}_c",
        r"\mathord{\mathaccent94 a^b}^c",
    ] {
        let source = input(&format!(r"\noindent${body}\message{{UNREAD}}"));
        let mut e = engine(source.as_bytes());
        assert_eq!(e.advance(usize::MAX), Err(Error::Syntax));
        assert_eq!(e.math.as_ref().unwrap().live_count(), 1);
        assert_eq!(e.next_expanded().unwrap().token, Token::character(11, b'c'));
    }
    for body in [
        r"\overline a\limits",
        r"\underline a\limits",
        r#"\radical"061000 a\limits"#,
        r"\mathaccent94 a\limits",
        r"\vcenter{\kern1pt}\limits",
    ] {
        let source = input(&format!(r"\noindent${body}\message{{UNREAD}}"));
        let mut e = engine(source.as_bytes());
        assert_eq!(e.advance(usize::MAX), Err(Error::Syntax), "{body}");
        assert_eq!(e.next_expanded().unwrap().meaning.command, cmd::MESSAGE);
    }
}

#[test]
fn vcenter_enters_internal_vertical_mode_with_local_resets_and_aftergroup_scripts() {
    let source = input(
        r"\hangindent=3pt\everyvbox{\ifvmode\ifinner\message{V:\the\prevdepth/\the\hangindent}\fi\fi}\def\after{^b}\noindent$a\vcenter{\aftergroup\after\kern1pt}\message{H:\the\hangindent}$\end",
    );
    let mut e = engine(source.as_bytes());
    build(&mut e).unwrap();
    assert_eq!(
        e.outcome().message_bytes(),
        [b"V:-1000.0pt/0.0pt".to_vec(), b"H:3.0pt".to_vec()]
    );
    assert!(e.document.box_frames.is_empty());
    assert!(e.document.math_frames.is_empty());
    assert_eq!(e.math.as_ref().unwrap().live_count(), 0);
}

#[test]
fn nested_form_failure_preserves_outer_math_and_unrelated_box_before_restore() {
    let source = input(
        r#"\setbox0=\hbox{z}\noindent$a\hbox{$\overline{\mathaccent94 {\hbox{b}}_{\hbox{c}}}\mathchar"0F63$}$\end"#,
    );
    let mut e = engine(source.as_bytes());
    let checkpoint = e.checkpoint().unwrap();
    assert_eq!(e.advance(usize::MAX), Err(Error::Font));
    assert_eq!(e.math.as_ref().unwrap().live_count(), 1);
    assert_eq!(e.document.math_frames.len(), 1);
    assert_eq!(e.document.nodes.live_count(), 2);
    assert!(e.math_scratch.as_ref().unwrap().is_empty());
    e.restore(&checkpoint).unwrap();
    assert_eq!(e.document.nodes.live_count(), 0);
    assert!(e.document.box_frames.is_empty());
}

#[test]
fn vcenter_wrong_closers_keep_construction_roots_and_nested_forms_replay() {
    for closer in [r"\endgroup", r"\right."] {
        let source = input(&format!(r"\noindent$a\vcenter{{\hbox{{b}}{closer}"));
        let mut e = engine(source.as_bytes());
        let checkpoint = e.checkpoint().unwrap();
        assert!(e.advance(usize::MAX).is_err());
        assert_eq!(e.current_group(), Some(GroupKind::VCenter));
        assert_eq!(e.document.box_frames.len(), 1);
        assert!(!e.can_checkpoint());
        e.restore(&checkpoint).unwrap();
        assert_eq!(e.document.nodes.live_count(), 0);
    }
    let source = input(
        r#"\vsize=100pt\hsize=70pt\parfillskip=0pt plus1fil\noindent$a^{\overline b}\vcenter{\hbox{$\radical"061000 {c\over d}$}}_{\mathaccent94 e}$\par\relax\end"#,
    );
    let mut e = engine(source.as_bytes());
    while e.document.text.prev_graf == 0 {
        e.advance(1).unwrap();
    }
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
    assert_eq!(e.math.as_ref().unwrap().live_count(), 0);
}

#[path = "math_forms_frontend_oracle_tests.rs"]
mod oracle;
