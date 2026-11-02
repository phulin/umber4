use crate::{Engine, Error, Progress, scalars::Target, token::cmd};

const BOOT: &str = r"\catcode123=1\catcode125=2 ";

#[test]
fn late_magnification_failure_preserves_pages_diagnostics_and_replays() {
    let source = format!(
        r"{BOOT}\mag=1200\shipout\hbox{{}}\mag=1300\shipout\hbox{{}}\immediate\write16{{DONE}}\end\count0=9"
    );
    let mut e = Engine::new(source.as_bytes());
    while e.outcome().pages.is_empty() {
        e.advance(1).unwrap();
    }
    let saved = e.checkpoint().unwrap();
    let retained = e.outcome();
    for _ in 0..2 {
        e.restore(&saved).unwrap();
        assert_eq!(e.advance(1), Ok(Progress::Boundary)); // MAG assignment.
        assert_eq!(e.document.scalars[Target::MAG].value, 1300);
        assert_eq!(e.advance(0), Ok(Progress::Boundary));
        assert_eq!(e.advance(1), Ok(Progress::Boundary)); // Second inkless page.
        assert_eq!(e.outcome().pages.len(), 2);
        assert!(e.outcome().pages.iter().all(|p| p.magnification() == 1200));
        assert_eq!(e.advance(usize::MAX), Err(Error::Number));
        let failed = e.outcome();
        assert_eq!(failed.pages.len(), 2);
        assert_eq!(failed.diagnostics.len(), 1);
        assert_eq!(failed.diagnostics[0].bytes(), b"DONE");
        assert_eq!(failed.counts[0], 0);
        assert_eq!(
            failed.pages.get(0).unwrap().boxes(),
            retained.pages.get(0).unwrap().boxes()
        );
        assert!(!e.document.complete);
        assert!(!e.can_checkpoint());
        assert_eq!(e.advance(0), Err(Error::Checkpoint));
        // END was consumed; no following command ran or was read by finalization.
        assert_eq!(e.next_expanded().unwrap().meaning.command, cmd::COUNT);
        e.restore(&saved).unwrap();
        assert_eq!(failed.pages.len(), 2);
        assert_eq!(failed.diagnostics.len(), 1);
        assert_eq!(failed.diagnostics[0].bytes(), b"DONE");
        assert_eq!(retained.pages.len(), 1);
    }
}

#[test]
fn restoring_magnification_before_end_succeeds_without_rechecking_later_pages() {
    let source = format!(r"{BOOT}\mag=1200\shipout\hbox{{}}\mag=0\shipout\hbox{{}}\mag=1200\end");
    let mut e = Engine::new(source.as_bytes());
    e.run_input().unwrap();
    assert_eq!(e.outcome().pages.len(), 2);
    assert!(e.outcome().pages.iter().all(|p| p.magnification() == 1200));
    assert_eq!(e.advance(0), Ok(Progress::Complete));
}

#[test]
fn zero_page_jobs_do_not_revalidate_unused_or_true_unit_magnification() {
    for body in [
        r"\mag=0\end",
        r"\mag=32769\shipout\box0\end",
        r"\mag=1200\dimen0=1truept\mag=1300\end",
        r"\mag=1200\dimen0=1truept\mag=0\shipout\box0\end",
    ] {
        let mut e = Engine::new(body.as_bytes());
        e.run_input().unwrap();
        assert!(e.outcome().pages.is_empty());
    }
}

#[test]
fn final_page_drain_precedes_validation_and_prior_errors_keep_precedence() {
    let source = format!(
        r"{BOOT}\mag=1200\shipout\hbox{{}}\output={{\shipout\box255\global\mag=1300\immediate\write16{{DRAINED}}}}\hrule height1pt\end"
    );
    let mut e = Engine::new(source.as_bytes());
    assert_eq!(e.advance(usize::MAX), Err(Error::Number));
    assert_eq!(e.outcome().pages.len(), 2);
    assert_eq!(e.outcome().diagnostics.len(), 1);
    assert_eq!(e.outcome().diagnostics[0].bytes(), b"DRAINED");
    assert!(e.document.output_frame.is_none());
    for (suffix, error) in [
        (r"\begingroup\end", Error::Group),
        (r"\iftrue\end", Error::Conditional),
        (r"\kern\undefined", Error::Undefined),
        (r"\dump", Error::Checkpoint),
        ("", Error::Eof),
    ] {
        let source = format!(r"{BOOT}\mag=1200\shipout\hbox{{}}\mag=1300{suffix}");
        let mut e = Engine::new(source.as_bytes());
        assert_eq!(e.advance(usize::MAX), Err(error), "{suffix}");
        assert_eq!(e.outcome().pages.len(), 1);
    }
}

#[test]
#[ignore = "native final magnification: set UMBER_PDFTEX"]
fn upstream_magnification_checks_postamble_and_exempts_zero_page_jobs() {
    let dir = std::env::temp_dir().join(format!("umber-final-mag-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("texmf.cnf"), "TEXINPUTS=.\nshell_escape=f\n").unwrap();
    for (index, (body, mismatch)) in [
        (
            r"\mag=1200\shipout\hbox{}\mag=1300\shipout\hbox{}\immediate\write16{DONE}\end",
            true,
        ),
        (
            r"\mag=1200\shipout\hbox{}\mag=0\shipout\hbox{}\mag=1200\end",
            false,
        ),
        (r"\mag=0\end", false),
        (r"\mag=32769\shipout\box0\end", false),
        (r"\mag=1200\dimen0=1truept\mag=1300\end", false),
    ]
    .into_iter()
    .enumerate()
    {
        let source = format!("{BOOT}{body}");
        let mut e = Engine::new(source.as_bytes());
        let result = e.advance(usize::MAX);
        assert_eq!(
            result,
            if mismatch {
                Err(Error::Number)
            } else {
                Ok(Progress::Complete)
            }
        );
        let stem = format!("case{index}");
        std::fs::write(dir.join(format!("{stem}.tex")), &source).unwrap();
        let native = std::process::Command::new(std::env::var_os("UMBER_PDFTEX").unwrap())
            .args([
                "-ini",
                "-etex",
                "-progname=pdftex",
                "-output-format=dvi",
                "-interaction=nonstopmode",
                "-halt-on-error",
            ])
            .arg(format!("{stem}.tex"))
            .env("TEXMFCNF", &dir)
            .current_dir(&dir)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&native.stdout);
        assert_eq!(!native.status.success(), mismatch, "{text}");
        assert_eq!(
            text.contains("Incompatible magnification"),
            mismatch,
            "{text}"
        );
        if mismatch {
            assert!(text.find("DONE").unwrap() < text.find("Incompatible magnification").unwrap());
        } else if !e.outcome().pages.is_empty() {
            let bytes = std::fs::read(dir.join(format!("{stem}.dvi"))).unwrap();
            assert_eq!(i32::from_be_bytes(bytes[10..14].try_into().unwrap()), 1200);
            let end = bytes.iter().rposition(|&b| b != 223).unwrap() + 1;
            let post = u32::from_be_bytes(bytes[end - 5..end - 1].try_into().unwrap()) as usize;
            assert_eq!(bytes[post], 248);
            assert_eq!(
                i32::from_be_bytes(bytes[post + 13..post + 17].try_into().unwrap()),
                1200
            );
            assert_eq!(
                crate::page_dvi_tests::read(&bytes, &e.outcome().pages).len(),
                2
            );
        } else {
            assert!(!dir.join(format!("{stem}.dvi")).exists());
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
