use super::*;
const DELCODES: &str = r#"\delcode40="061000\delcode41="062000 "#;

#[test]
fn duplicate_fraction_scans_its_operands_without_replacing_either_owner() {
    for command in [
        r"\over",
        r"\atop",
        r"\above-.5pt",
        r"\overwithdelims()",
        r"\atopwithdelims()",
        r"\abovewithdelims()-.5pt",
    ] {
        let source = input(&format!(
            r"{DELCODES}\noindent$\hbox{{a}}\over\hbox{{b}}{command}\message{{UNREAD}}"
        ));
        let mut e = engine(source.as_bytes());
        let checkpoint = e.checkpoint().unwrap();
        while e
            .document
            .math_frames
            .last()
            .is_none_or(|f| f.incomplete.is_none())
        {
            let read = e.next_expanded().unwrap();
            e.execute(read).unwrap();
        }
        let owner = e
            .document
            .math_frames
            .last()
            .unwrap()
            .incomplete
            .as_ref()
            .unwrap()
            .tail();
        assert_eq!(build(&mut e), Err(Error::Syntax), "{command}");
        let frame = e.document.math_frames.last().unwrap();
        assert_eq!(frame.incomplete.as_ref().unwrap().tail(), owner);
        assert_eq!(e.math.as_ref().unwrap().live_count(), 3);
        assert_eq!(e.document.nodes.live_count(), 4);
        assert_eq!(e.next_expanded().unwrap().meaning.command, cmd::MESSAGE);
        e.restore(&checkpoint).unwrap();
        assert_eq!(e.document.nodes.live_count(), 0);
        assert_eq!(e.math.as_ref().unwrap().live_count(), 0);
    }
}

#[test]
fn fraction_operand_errors_leave_the_original_forest_rooted_and_cursor_native() {
    for (first, second, error, next, live) in [
        ("", r"\above1mu", Error::Number, 11, 2),
        ("", r"\overwithdelims(\char40", Error::Syntax, cmd::CHAR, 2),
        (
            r"\over\hbox{b}",
            r"\abovewithdelims(\char40",
            Error::Syntax,
            cmd::CHAR,
            3,
        ),
        (r"\over\hbox{b}", r"\above1mu", Error::Number, 11, 3),
    ] {
        let source = input(&format!(
            r"{DELCODES}\noindent$\hbox{{a}}{first}{second}\message{{UNREAD}}"
        ));
        let mut e = engine(source.as_bytes());
        let checkpoint = e.checkpoint().unwrap();
        assert_eq!(e.advance(usize::MAX), Err(error));
        assert!(e.document.math_frames.last().unwrap().incomplete.is_some());
        assert_eq!(e.math.as_ref().unwrap().live_count(), live);
        assert_eq!(e.next_expanded().unwrap().meaning.command, next);
        e.restore(&checkpoint).unwrap();
        assert_eq!(e.document.nodes.live_count(), 0);
        assert!(e.document.math_frames.is_empty());
    }
}

#[test]
fn field_restoration_and_nested_fraction_replay_finish_all_three_contexts() {
    let source = input(&format!(
        r"{DELCODES}\def\after{{^c}}\vsize=100pt\hsize=70pt\parfillskip=0pt plus1fil\noindent$a^{{b\over c}}\left(\aftergroup\after{{a\atop b}}\over\hbox{{$c\above.5pt d$}}\right)$\par\relax\end"
    ));
    let mut e = engine(source.as_bytes());
    while e.document.text.prev_graf == 0 {
        e.advance(1).unwrap();
    }
    assert!(e.can_checkpoint());
    assert!(e.document.math_frames.is_empty());
    assert_eq!(e.math.as_ref().unwrap().live_count(), 0);
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
fn failed_fraction_conversion_releases_boxes_but_preserves_outer_and_register_roots() {
    let source =
        input(r#"\setbox0=\hbox{z}\noindent$a\hbox{${\hbox{b}\over\hbox{c}\mathchar"0F63}$}$\end"#);
    let mut e = engine(source.as_bytes());
    let checkpoint = e.checkpoint().unwrap();
    assert_eq!(e.advance(usize::MAX), Err(Error::Font));
    assert_eq!(e.document.math_frames.len(), 1);
    assert_eq!(e.math.as_ref().unwrap().live_count(), 1);
    assert_eq!(e.document.nodes.live_count(), 2);
    assert!(e.math_scratch.as_ref().unwrap().is_empty());
    e.restore(&checkpoint).unwrap();
    assert_eq!(e.document.nodes.live_count(), 0);
    assert_eq!(e.math.as_ref().unwrap().live_count(), 0);
}

#[path = "math_fraction_frontend_oracle_tests.rs"]
mod oracle;
