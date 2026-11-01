use super::*;

const DISPLAY: &str = r"\hsize=60pt\vsize=200pt\baselineskip=12pt\lineskip=1pt\topskip=10pt\parfillskip=0pt plus1fil
\abovedisplayskip=6pt plus1pt\belowdisplayskip=7pt plus2pt\abovedisplayshortskip=2pt\belowdisplayshortskip=3pt\predisplaypenalty=17\postdisplaypenalty=19 ";

fn display_input(body: &str) -> String {
    input(&format!("{DISPLAY}{body}"))
}

#[test]
fn display_entry_lookahead_modes_hooks_and_resume_do_not_repeat_paragraph_entry() {
    let source = display_input(
        r"\count0=0\everypar{\global\advance\count0by1}\everymath{\message{I:\the\fam/\ifinner Y\else N\fi}}\everydisplay{\message{D:\the\fam/\ifmmode Y\else N\fi/\ifinner Y\else N\fi}\mathchoice{\message{C:\ifinner Y\else N\fi}}{}{}{}}\noindent$$\ifmmode a\else\undefined\fi\mathord{\message{F:\ifinner Y\else N\fi}b}\eqno c$$\message{R:\the\count0/\the\spacefactor}\par\end",
    );
    let mut e = engine(source.as_bytes());
    e.advance(usize::MAX).unwrap();
    assert_eq!(
        e.outcome().message_bytes(),
        [
            b"D:-1/Y/N".to_vec(),
            b"C:Y".to_vec(),
            b"F:Y".to_vec(),
            b"I:-1/Y".to_vec(),
            b"R:1/1000".to_vec(),
        ]
    );
    assert!(e.document.math_frames.is_empty());
    assert_eq!(e.math.as_ref().unwrap().live_count(), 0);
}

#[test]
fn entry_and_resume_backup_only_token_identity_after_consuming_noexpand() {
    for body in [
        r"\def\probe{\message{M}a}\noindent\expandafter$\noexpand\probe$\par\end",
        r"\def\probe{\message{M}a}\noindent$$a$$\noexpand\probe\par\end",
    ] {
        let source = display_input(body);
        let mut e = engine(source.as_bytes());
        e.advance(usize::MAX).unwrap();
        assert_eq!(e.outcome().message_bytes(), [b"M".to_vec()]);
    }
    let source = display_input(r"\def\probe{\message{BAD}a}\noindent$\noexpand\probe$\par\end");
    let mut e = engine(source.as_bytes());
    e.advance(usize::MAX).unwrap();
    assert!(e.outcome().message_bytes().is_empty());
}

#[test]
fn display_closer_and_equation_number_errors_keep_disjoint_roots_and_cursor_order() {
    for (body, error, next, frames, mode) in [
        (
            r"\noindent$$a$\message{UNREAD}",
            Error::Syntax,
            cmd::MESSAGE,
            1,
            Mode::DisplayMath,
        ),
        (
            r"\noindent$$a\eqno b$\message{UNREAD}",
            Error::Syntax,
            cmd::MESSAGE,
            2,
            Mode::Math,
        ),
        (
            r"\noindent$$a}\message{UNREAD}",
            Error::Group,
            cmd::MESSAGE,
            1,
            Mode::DisplayMath,
        ),
        (
            r"\noindent$$a\endgroup\message{UNREAD}",
            Error::Group,
            cmd::MESSAGE,
            1,
            Mode::DisplayMath,
        ),
        (
            r"\noindent$$a\eqno b\eqno\message{UNREAD}",
            Error::Syntax,
            cmd::MESSAGE,
            2,
            Mode::Math,
        ),
        (
            r"\noindent$$a\begingroup\eqno\message{UNREAD}",
            Error::Group,
            cmd::MESSAGE,
            1,
            Mode::DisplayMath,
        ),
        (
            r"\noindent$a\eqno\message{UNREAD}",
            Error::Syntax,
            cmd::MESSAGE,
            1,
            Mode::Math,
        ),
    ] {
        let source = display_input(&format!(r"\setbox0=\hbox{{z}}{body}"));
        let mut e = engine(source.as_bytes());
        let checkpoint = e.checkpoint().unwrap();
        assert_eq!(e.advance(usize::MAX), Err(error), "{body}");
        assert_eq!(e.document.math_frames.len(), frames);
        assert_eq!(e.document.text.mode, mode);
        assert_eq!(e.next_expanded().unwrap().meaning.command, next);
        assert_eq!(e.document.nodes.live_count(), 2);
        assert!(!e.can_checkpoint());
        e.restore(&checkpoint).unwrap();
        assert_eq!(e.document.nodes.live_count(), 0);
        assert!(e.document.math_frames.is_empty());
    }
}

#[test]
fn font_validation_precedes_closing_lookahead_and_repeats_after_number_unsave() {
    for (body, frames, number) in [
        (r"\noindent$$\textfont2=\nullfont a$\undefined", 1, false),
        (
            r"\noindent$$a\eqno\textfont2=\nullfont b$\undefined",
            2,
            false,
        ),
        (
            r"\noindent$$\textfont2=\nullfont a\eqno\textfont2=\f b$$\undefined",
            1,
            true,
        ),
    ] {
        let source = display_input(&format!(r"\setbox0=\hbox{{z}}{body}"));
        let mut e = engine(source.as_bytes());
        let checkpoint = e.checkpoint().unwrap();
        assert_eq!(e.advance(usize::MAX), Err(Error::Font));
        assert_eq!(e.document.math_frames.len(), frames);
        assert_eq!(e.next_token().unwrap().meaning.command, cmd::UNDEFINED);
        if number {
            assert!(matches!(
                e.document.math_frames.last().unwrap().context,
                Context::Display {
                    number: Some(_),
                    ..
                }
            ));
        }
        e.restore(&checkpoint).unwrap();
        assert_eq!(e.math.as_ref().unwrap().live_count(), 0);
        assert_eq!(e.document.nodes.live_count(), 0);
    }
}

#[test]
fn displays_resume_language_and_replay_only_at_quiescent_boundaries() {
    let source = display_input(
        r"\language=4\lefthyphenmin=2\righthyphenmin=3\hangindent=5pt\noindent ab$$\language=7\global\lefthyphenmin=9 a\leqno b$$\relax cd\par\relax\end",
    );
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
    assert_eq!(
        e.document.text.initial_language,
        crate::language::Language {
            number: 4,
            left: 9,
            right: 3
        }
    );
    assert_eq!(e.document.text.prev_graf, 4);
    assert_eq!(
        e.document.layout_values.dimens[DimenParam::HangIndent.index()].value,
        Scaled(5 * 65536)
    );
    while !e.can_checkpoint() {
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

#[path = "math_display_oracle_tests.rs"]
mod oracle;
