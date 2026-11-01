use super::*;

const EFFECTS: &str = r"\count0=0\count1=7\count2=0
\everymath{\global\advance\count2by1}
\def\opener{\ifnum\count1=7 \ifmmode\ifinner\relax\else\undefined\fi\else\undefined\fi\else\undefined\fi}
\noindent$\mathchoice
{\message{UMBER:D:\the\count1/\the\fam:END}\count1=1\fam=1\global\advance\count0by1\aftergroup\opener a}
{\message{UMBER:T:\the\count1/\the\fam:END}\count1=2\fam=2\global\advance\count0by1 b}
{\message{UMBER:S:\the\count1/\the\fam:END}\count1=3\fam=3\global\advance\count0by1 c}
{\message{UMBER:SS:\the\count1/\the\fam:END}\count1=4\fam=4\global\advance\count0by1 d}
\message{UMBER:F:\the\count0/\the\count1/\the\count2/\the\fam:END}$\par";

#[test]
fn choices_execute_every_branch_with_independent_locals_and_one_inline_hook() {
    let source = input(&format!(r"{EFFECTS}\end"));
    let mut e = engine(source.as_bytes());
    e.advance(usize::MAX).unwrap();
    assert_eq!(
        e.outcome().message_bytes(),
        [
            b"UMBER:D:7/-1:END".to_vec(),
            b"UMBER:T:7/-1:END".to_vec(),
            b"UMBER:S:7/-1:END".to_vec(),
            b"UMBER:SS:7/-1:END".to_vec(),
            b"UMBER:F:4/7/1/-1:END".to_vec(),
        ]
    );
    assert_eq!(e.math.as_ref().unwrap().live_count(), 0);
    assert!(e.document.math_frames.is_empty());
}

#[test]
fn choice_missing_openers_and_wrong_closers_preserve_the_active_branch() {
    for (body, error, next, branch, live, nodes) in [
        (
            r"\mathchoice\message{UNREAD}",
            Error::Syntax,
            cmd::MESSAGE,
            0,
            2,
            2,
        ),
        (
            r"\mathchoice{\hbox{b}}{\hbox{c}}\message{UNREAD}",
            Error::Syntax,
            cmd::MESSAGE,
            2,
            4,
            6,
        ),
        (
            r"\mathchoice{b}{c}{d}\message{UNREAD}",
            Error::Syntax,
            cmd::MESSAGE,
            3,
            5,
            2,
        ),
        (
            r"\mathchoice{b}{c}{d}{e\endgroup\message{UNREAD}",
            Error::Group,
            cmd::MESSAGE,
            3,
            6,
            2,
        ),
        (
            r"\mathchoice{b}{c}{d}{e\right.\message{UNREAD}",
            Error::Group,
            12,
            3,
            6,
            2,
        ),
        (
            r"\mathchoice{b}{c}{d}{e$\message{UNREAD}",
            Error::Group,
            cmd::MESSAGE,
            3,
            6,
            2,
        ),
    ] {
        let source = input(&format!(r"\setbox0=\hbox{{z}}\noindent$a{body}"));
        let mut e = engine(source.as_bytes());
        let checkpoint = e.checkpoint().unwrap();
        assert_eq!(e.advance(usize::MAX), Err(error), "{body}");
        assert_eq!(e.current_group(), Some(GroupKind::MathChoice));
        let frame = e.document.math_frames.last().unwrap();
        assert!(matches!(frame.context,Context::Choice{branch:b,..} if b==branch));
        assert_eq!(e.math.as_ref().unwrap().live_count(), live);
        assert_eq!(e.document.nodes.live_count(), nodes);
        assert_eq!(e.next_expanded().unwrap().meaning.command, next);
        assert!(!e.can_checkpoint());
        e.restore(&checkpoint).unwrap();
        assert_eq!(e.document.nodes.live_count(), 0);
        assert_eq!(e.math.as_ref().unwrap().live_count(), 0);
    }
}

#[test]
fn branch_opener_expansion_errors_and_eof_keep_the_new_group() {
    for (body, error, branch) in [
        (r"\mathchoice\undefined", Error::Undefined, 0),
        (r"\mathchoice{}{}\undefined", Error::Undefined, 2),
        (r"\mathchoice{}{}{}", Error::Eof, 3),
        (r"\mathchoice{}{}{}{", Error::Eof, 3),
    ] {
        let source = input(&format!(r"\noindent${body}"));
        let mut e = engine(source.as_bytes());
        let checkpoint = e.checkpoint().unwrap();
        assert_eq!(e.advance(usize::MAX), Err(error));
        assert!(
            matches!(e.document.math_frames.last().unwrap().context,Context::Choice{branch:b,..} if b==branch)
        );
        assert_eq!(e.current_group(), Some(GroupKind::MathChoice));
        e.restore(&checkpoint).unwrap();
        assert!(e.document.math_frames.is_empty());
    }
}

#[test]
fn failed_nested_choice_conversion_preserves_outer_owners_and_reuses_storage() {
    let source = input(
        r#"\setbox0=\hbox{z}\noindent$a\hbox{$\mathchoice{\hbox{b}}{\mathchar"0F63}{\hbox{c}}{\hbox{d}}$}$"#,
    );
    let mut e = engine(source.as_bytes());
    let checkpoint = e.checkpoint().unwrap();
    for _ in 0..3 {
        assert_eq!(e.advance(usize::MAX), Err(Error::Font));
        assert_eq!(e.math.as_ref().unwrap().live_count(), 1);
        assert_eq!(e.document.math_frames.len(), 1);
        assert_eq!(e.document.nodes.live_count(), 2);
        assert!(e.math_scratch.as_ref().unwrap().is_empty());
        e.restore(&checkpoint).unwrap();
        assert_eq!(e.math.as_ref().unwrap().live_count(), 0);
        assert_eq!(e.document.nodes.live_count(), 0);
    }
}

#[test]
fn nested_choices_fractions_and_forms_replay_from_a_quiescent_boundary() {
    let source = input(
        r"\vsize=100pt\hsize=70pt\noindent$a\mathchoice{b\over c}{\vcenter{\hbox{$\mathchoice{d}{e}{f}{g}$}}}{\overline h}{\mathaccent94 i}^j$\par\relax\end",
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
    assert!(!first.is_empty());
    e.restore(&checkpoint).unwrap();
    e.advance(usize::MAX).unwrap();
    let second: Vec<_> = e
        .outcome()
        .pages
        .iter()
        .flat_map(|p| p.placements().collect::<Vec<_>>())
        .collect();
    assert_eq!(first, second);
}

#[path = "math_choice_frontend_oracle_tests.rs"]
mod oracle;
