use super::*;

const DELCODES: &str = r#"\delcode40="061000\delcode41="062000 "#;
fn delimiter_tuple(d: Delimiter) -> (u8, u8, u8, u8) {
    (d.small_family, d.small_char, d.large_family, d.large_char)
}

#[test]
fn delimiter_codes_split_the_low_24_bits_and_accept_only_the_native_operand_kinds() {
    for (operand, want) in [
        ("(", (0, 97, 0, 0)),
        (r"\aschar", (0, 97, 0, 0)),
        (r"\explicit", (1, 35, 4, 86)),
        (r#"\delimiter"7FFFFFF"#, (15, 255, 15, 255)),
        (r"\relax\spaces.", (0, 0, 0, 0)),
        (r"~", (0, 97, 0, 0)),
    ] {
        let source = input(&format!(
            r#"{DELCODES}\let\aschar=(\def\explicit{{\delimiter"7123456 }}\def\spaces{{ }}\catcode126=13\def~{{(}}\catcode40=13\def({{\undefined}}\catcode40=12\mathcode40="8000\relax {operand}"#
        ));
        let mut e = engine(source.as_bytes());
        while e
            .next_expanded()
            .and_then(|read| {
                if read.meaning.command == cmd::RELAX {
                    return Ok(false);
                }
                e.execute(read)?;
                Ok(true)
            })
            .unwrap()
        {}
        assert_eq!(
            delimiter_tuple(e.scan_math_delimiter().unwrap()),
            want,
            "{operand}"
        );
    }
    for operand in [
        r"\given",
        r"\mathgiven",
        r"\char40",
        r"\mathchar40",
        r"{",
        r"x",
    ] {
        let source = input(&format!(
            r"{DELCODES}\chardef\given=40\mathchardef\mathgiven=40\relax {operand}"
        ));
        let mut e = engine(source.as_bytes());
        loop {
            let read = e.next_expanded().unwrap();
            if read.meaning.command == cmd::RELAX {
                break;
            }
            e.execute(read).unwrap();
        }
        let first = e.next_expanded().unwrap();
        e.back_read(first);
        assert!(
            matches!(e.scan_math_delimiter(), Err(Error::Syntax)),
            "{operand}"
        );
        let backed = e.next_expanded().unwrap();
        assert_eq!(
            backed.token, first.token,
            "missing delimiter backs its token"
        );
        assert_eq!(backed.meaning, first.meaning);
    }
}

#[test]
fn numeric_delimiter_range_checks_preserve_the_following_operand_cursor() {
    for (operand, want) in [
        ("0", Ok(0)),
        ("134217727", Ok(134217727)),
        ("-1", Err(Error::Number)),
        ("134217728", Err(Error::Number)),
    ] {
        let source = format!("{operand} !");
        let mut e = Engine::new(source.as_bytes());
        assert_eq!(e.scan_delimiter_code(), want);
        assert_eq!(e.next_expanded().unwrap().token, Token::other(b'!'));
    }
}

#[test]
fn right_mismatch_consumes_only_the_operand_prescribed_by_the_current_group() {
    for (body, want, next, live) in [
        (
            r#"\noindent$a\right\delimiter"4061000\message{X}"#,
            Error::Syntax,
            cmd::MESSAGE,
            1,
        ),
        (
            r#"\noindent${a\right\delimiter"4061000\message{X}"#,
            Error::Group,
            cmd::DELIMITER,
            2,
        ),
        (
            r"\noindent$a\left(b\right\char40",
            Error::Syntax,
            cmd::CHAR,
            3,
        ),
    ] {
        let source = input(&format!("{DELCODES}{body}"));
        let mut e = engine(source.as_bytes());
        let initial = e.checkpoint().unwrap();
        assert_eq!(e.advance(usize::MAX), Err(want), "{body}");
        assert_eq!(e.next_expanded().unwrap().meaning.command, next);
        assert_eq!(
            e.math.as_ref().unwrap().live_count(),
            live,
            "temporary boundary released"
        );
        assert!(!e.can_checkpoint());
        e.restore(&initial).unwrap();
        assert!(e.document.math_frames.is_empty());
        assert_eq!(e.math.as_ref().unwrap().live_count(), 0);
    }
}

#[test]
fn left_operand_failure_releases_the_temporary_boundary_before_restore() {
    for (operand, error) in [
        (r"\delimiter-1", Error::Number),
        (r"\delimiter134217728", Error::Number),
        (r"\undefined", Error::Undefined),
        ("", Error::Eof),
    ] {
        let source = input(&format!(r"\noindent$a\left {operand}"));
        let mut e = engine(source.as_bytes());
        let checkpoint = e.checkpoint().unwrap();
        assert_eq!(e.advance(usize::MAX), Err(error), "{operand}");
        assert_eq!(e.current_group(), Some(GroupKind::MathShift));
        assert_eq!(e.document.math_frames.len(), 1);
        assert_eq!(e.math.as_ref().unwrap().live_count(), 1);
        e.restore(&checkpoint).unwrap();
        assert_eq!(e.math.as_ref().unwrap().live_count(), 0);
    }
}

#[test]
fn left_group_rejects_other_closers_and_restores_its_disjoint_owner_forest() {
    for closer in ["$", "}", r"\endgroup"] {
        let source = input(&format!(
            r"{DELCODES}\setbox0=\hbox{{z}}\noindent$a\left(\hbox{{b}}{closer}"
        ));
        let mut e = engine(source.as_bytes());
        let initial = e.checkpoint().unwrap();
        assert_eq!(e.advance(usize::MAX), Err(Error::Group));
        assert_eq!(e.current_group(), Some(GroupKind::MathLeft));
        assert_eq!(e.document.math_frames.len(), 2);
        assert_eq!(e.math.as_ref().unwrap().live_count(), 3);
        e.restore(&initial).unwrap();
        assert_eq!(e.document.nodes.live_count(), 0);
        assert_eq!(e.math.as_ref().unwrap().live_count(), 0);
    }
}

#[test]
fn nested_left_right_boxes_and_failed_conversion_preserve_outer_ids() {
    let source = input(&format!(
        r#"{DELCODES}\setbox0=\hbox{{z}}\noindent$a\hbox{{$\left(\hbox{{b}}^c\mathchar"0F63\right)$}}$\end"#
    ));
    let mut e = engine(source.as_bytes());
    let initial = e.checkpoint().unwrap();
    assert_eq!(e.advance(usize::MAX), Err(Error::Font));
    assert_eq!(e.document.math_frames.len(), 1);
    assert_eq!(e.math.as_ref().unwrap().live_count(), 1);
    assert_eq!(
        e.document.nodes.live_count(),
        2,
        "unrelated register box survives"
    );
    assert!(e.math_scratch.as_ref().unwrap().is_empty());
    e.restore(&initial).unwrap();
    assert_eq!(e.document.nodes.live_count(), 0);
    assert_eq!(e.math.as_ref().unwrap().live_count(), 0);
}

#[test]
fn left_right_restoration_precedes_aftergroup_scripts_and_quiescent_replay() {
    let source = input(&format!(
        r"{DELCODES}\def\after{{^b}}\vsize=100pt\hsize=70pt\parfillskip=0pt plus1fil\noindent$a\left(\fam=0\aftergroup\after\left(b\right)\right)\message{{\the\fam}}c$\par\relax\end"
    ));
    let mut e = engine(source.as_bytes());
    while e.document.text.prev_graf == 0 {
        e.advance(1).unwrap();
    }
    assert!(e.can_checkpoint());
    assert_eq!(e.outcome().message_bytes(), [b"-1".to_vec()]);
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

#[path = "math_delimiter_frontend_oracle_tests.rs"]
mod oracle;
