use crate::{
    Engine, Error, Progress,
    numeric::{Glue, GlueOrder, Scaled},
    token::Token,
};

const BOOT: &str = "\\catcode123=1\\catcode125=2\\catcode35=6\\relax\n";

fn cases() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        (
            r"\message{MU:\the\thinmuskip/\the\medmuskip/\the\thickmuskip:END}
\muskipdef\m=255\let\alias=\m\m=1.25mu plus2fill minus-3mu
\message{MU:\the\alias/\meaning\alias:END}
\let\same=\m\ifx\same\alias\message{MU:SAME:END}\fi
\skipdef\ordinary=255\ifx\ordinary\m\else\message{MU:DISTINCT:END}\fi",
            vec![
                "0.0mu/0.0mu/0.0mu",
                "1.25mu plus 2.0fill minus -3.0mu/\\muskip255",
                "SAME",
                "DISTINCT",
            ],
        ),
        (
            r#"\muskip0=2mu plus3mu minus4fil\muskip1=-\muskip0
\muskip2=1.5\muskip0 plus.25\muskip0 minus1filll
\count0=-3\muskip3=\count0 mu
\message{MU:\the\muskip1/\the\muskip2/\the\muskip3:END}
\muskip4=0mu plus\muskip0 minus\muskip1
\message{MU:\the\muskip4:END}
\muskip5=--'10mu plus"10fil minus`A mu
\message{MU:\the\muskip5:END}
\muskip6=-.5\muskip0 plus1fil \message{MU:\the\muskip6:END}"#,
            vec![
                "-2.0mu plus -3.0mu minus -4.0fil/3.0mu plus 0.5mu minus 1.0filll/-3.0mu",
                "0.0mu plus 2.0mu minus -2.0mu",
                "8.0mu plus 16.0fil minus 65.0mu",
                "-1.0mu plus 1.0fil",
            ],
        ),
        (
            r"\thinmuskip=.5mu plus2fil minus3fill
\advance\thinmuskip by1.5mu plus-2fil minus4mu
\message{MU:\the\thinmuskip:END}
\multiply\thinmuskip by-3\divide\thinmuskip by2
\message{MU:\the\thinmuskip:END}
\medmuskip=2mu plus-2fil\advance\medmuskip0mu plus2fil
\advance\medmuskip0mu plus1mu\message{MU:\the\medmuskip:END}
\thickmuskip=-.00000762939453125mu plus1.5filll minus.5mu
\message{MU:\the\thickmuskip:END}",
            vec![
                "2.0mu minus 3.0fill",
                "-3.0mu minus -4.5fill",
                "2.0mu plus 1.0mu",
                "-0.00002mu plus 1.5filll minus 0.5mu",
            ],
        ),
        (
            r"\thinmuskip=3mu plus1mu minus2fil
\edef\saved{\the\thinmuskip}\thinmuskip=4mu
\message{MU:\saved/\the\thinmuskip:END}
\immediate\write16{MU:\saved/\the\thinmuskip:END}
\def\report{\message{MU:AFTER:END}}\afterassignment\report\muskipdef\m=255
\afterassignment\report\m=7mu
\message{MU:\the\m:END}",
            vec![
                "3.0mu plus 1.0mu minus 2.0fil/4.0mu",
                "3.0mu plus 1.0mu minus 2.0fil/4.0mu",
                "AFTER",
                "AFTER",
                "7.0mu",
            ],
        ),
        (
            r"\muskip0=8192mu\advance\muskip0 by8192mu\muskip1=-\muskip0
\message{MU:\the\muskip0/\the\muskip1:END}
\divide\muskip0 by1\message{MU:\the\muskip0:END}
\thinmuskip=1mu{\globaldefs=-1\global\thinmuskip=2mu}
\message{MU:\the\thinmuskip:END}
{\globaldefs=1\thinmuskip=3mu}\globaldefs=0
\message{MU:\the\thinmuskip:END}",
            vec!["16384.0mu/-16384.0mu", "16384.0mu", "1.0mu", "3.0mu"],
        ),
    ]
}

fn marked(bytes: &[u8]) -> &[u8] {
    bytes
        .strip_prefix(b"MU:")
        .unwrap()
        .strip_suffix(b":END")
        .unwrap()
}

#[test]
fn math_glue_quantities_arithmetic_and_expansion_keep_their_kind() {
    for (body, expected) in cases() {
        let source = format!("{BOOT}{body}\\end");
        let outcome = Engine::new(source.as_bytes()).run().unwrap();
        assert_eq!(
            outcome
                .diagnostics
                .iter()
                .map(|d| marked(d.bytes()))
                .collect::<Vec<_>>(),
            expected.iter().map(|s| s.as_bytes()).collect::<Vec<_>>(),
            "{body}"
        );
    }
}

#[test]
fn all_math_glue_registers_start_at_zero_separate_from_ordinary_glue() {
    let mut source = BOOT.to_owned();
    for i in 0..256 {
        source.push_str(&format!(r"\message{{MU:\the\muskip{i}:END}}"));
    }
    source.push_str(r"\muskip255=1mu\skip255=2pt\message{MU:\the\muskip255/\the\skip255:END}\end");
    let out = Engine::new(source.as_bytes()).run().unwrap();
    assert!(
        out.diagnostics[..256]
            .iter()
            .all(|d| marked(d.bytes()) == b"0.0mu")
    );
    assert_eq!(marked(out.diagnostics[256].bytes()), b"1.0mu/2.0pt");
}

#[test]
fn math_glue_scans_preserve_direct_spec_and_internal_width_boundaries() {
    let value = Glue {
        width: Scaled(2 * 65536),
        stretch: Scaled(3 * 65536),
        shrink: Scaled(4 * 65536),
        stretch_order: GlueOrder::Fill,
        shrink_order: GlueOrder::Fil,
    };
    for (operand, want, next) in [
        (
            r"-\muskip1 plus1mu!",
            value.negate(),
            Token::character(11, b'p'),
        ),
        (
            r"1.5\muskip1 !",
            Glue {
                width: Scaled(3 * 65536),
                ..Glue::default()
            },
            Token::other(b'!'),
        ),
    ] {
        for stored in [false, true] {
            let source = if stored {
                format!("{BOOT}\\def\\operand{{{operand}}}\\operand")
            } else {
                operand.to_owned()
            };
            let mut engine = Engine::new(source.as_bytes());
            if stored {
                engine.advance(5).unwrap();
            }
            engine.assign_mu_glue(1, value, false);
            assert_eq!(
                engine.scan_mu_glue(),
                Ok(want),
                "{operand}, stored={stored}"
            );
            assert_eq!(engine.next_raw().unwrap().token, next);
        }
    }
    let mut engine = Engine::new(br"\muskip0!");
    engine.assign_mu_glue(
        0,
        Glue {
            width: Scaled(1 << 30),
            ..Glue::default()
        },
        false,
    );
    assert_eq!(engine.scan_mu_glue().unwrap().width, Scaled(1 << 30));
    assert_eq!(engine.next_raw().unwrap().token, Token::other(b'!'));
    let mut engine = Engine::new(br"1\muskip0!");
    engine.assign_mu_glue(
        0,
        Glue {
            width: Scaled(1 << 30),
            ..Glue::default()
        },
        false,
    );
    assert_eq!(engine.scan_mu_glue(), Err(Error::Overflow));
}

fn incompatible_cases() -> Vec<&'static str> {
    vec![
        r"\muskip0=\skip0",
        r"\muskip0=\dimen0",
        r"\skip0=\muskip0",
        r"\dimen0=\muskip0",
        r"\count0=\muskip0",
        r"\message{\number\muskip0}",
        r"\muskip0=2\count0",
        r"\muskip0=2\dimen0",
        r"\muskip0=2\skip0",
        r"\skip0=2\muskip0",
        r"\dimen0=2\muskip0",
        r"\muskip0=1mu plus\skip0",
        r"\muskip0=1mu minus\dimen0",
        r"\muskip0=1mu plus2\count0",
        r"\skip0=1pt plus\muskip0",
        r"\advance\muskip0 by\skip0",
        r"\multiply\muskip0 by\muskip0",
        r"\divide\muskip0 by\muskip0",
    ]
}

#[test]
fn mixed_glue_kinds_and_wrong_units_fail_before_assignment() {
    for body in incompatible_cases()
        .into_iter()
        .map(str::to_owned)
        .chain(["pt", "sp", "em", "ex", "truemu", "fil"].map(|unit| format!(r"\muskip0=0{unit}")))
    {
        let source = format!("{BOOT}{body}\\end");
        let mut engine = Engine::new(source.as_bytes());
        assert_eq!(engine.advance(usize::MAX), Err(Error::Number), "{body}");
        assert_eq!(
            engine.document.layout_values.mu_glues[0].value,
            Glue::default()
        );
        assert!(engine.document.layout_values.prepared_mag.is_none());
    }
}

#[test]
fn math_glue_binding_failures_restore_without_partial_arithmetic() {
    for (bad, error) in [
        (r"\muskip0=16384mu", Error::Overflow),
        (r"\muskip0=1mu plus16384fil", Error::Overflow),
        (r"\multiply\muskip0 by2147483647", Error::Overflow),
        (r"\divide\muskip0 by0", Error::Overflow),
        (r"\muskip0=1mu plus1fillll", Error::Number),
        (r"\muskip256=1mu", Error::Number),
    ] {
        let source = format!(r"{BOOT}\muskip0=7mu\count0=1\relax{bad}\end");
        let mut engine = Engine::new(source.as_bytes());
        while engine.outcome().counts[0] == 0 {
            engine.advance(1).unwrap();
        }
        engine.advance(1).unwrap();
        let saved = engine.checkpoint().unwrap();
        assert_eq!(engine.advance(1), Err(error), "{bad}");
        assert_eq!(
            engine.document.layout_values.mu_glues[0].value.width,
            Scaled(7 * 65536)
        );
        engine.restore(&saved).unwrap();
        assert!(engine.can_checkpoint());
    }
}

#[test]
fn math_glue_open_group_and_pending_argument_checkpoints_branch_independently() {
    let source = format!(
        r"{BOOT}\muskipdef\m=255\m=1mu\skip255=9pt
\def\work#1{{{{\m=2mu\thinmuskip=3mu\count0=1\relax
#1\global\m=4mu\m=5mu\medmuskip=6mu}}}}
\work{{\advance\thinmuskip by1mu}}
\message{{MU:\the\m/\the\thinmuskip/\the\medmuskip/\the\skip255:END}}\end"
    );
    let mut engine = Engine::new(source.as_bytes());
    while engine.outcome().counts[0] == 0 {
        engine.advance(1).unwrap();
    }
    let saved = engine.checkpoint().unwrap();
    let mut expected = None;
    for _ in 0..3 {
        while engine.advance(32).unwrap() != Progress::Complete {}
        let out = engine.outcome().message_bytes();
        assert_eq!(out, [b"MU:4.0mu/0.0mu/0.0mu/9.0pt:END".to_vec()]);
        if let Some(before) = &expected {
            assert_eq!(&out, before);
        }
        expected = Some(out);
        engine.restore(&saved).unwrap();
        assert_eq!(
            engine.document.layout_values.mu_glues[255].value.width,
            Scaled(2 * 65536)
        );
        engine.assign_mu_glue(
            255,
            Glue {
                width: Scaled(99),
                ..Glue::default()
            },
            true,
        );
        engine.restore(&saved).unwrap();
    }
    drop(engine);
    drop(saved);
}

struct Oracle(std::path::PathBuf);
impl Drop for Oracle {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
impl Oracle {
    fn run(&self, body: &str) -> (bool, String) {
        std::fs::write(self.0.join("probe.tex"), format!("{BOOT}{body}\\end\n")).unwrap();
        let output = std::process::Command::new(
            std::env::var_os("UMBER_PDFTEX").expect("explicit native oracle"),
        )
        .current_dir(&self.0)
        .env("TEXMFCNF", &self.0)
        .args([
            "-ini",
            "-etex",
            "-no-shell-escape",
            "-interaction=nonstopmode",
            "-halt-on-error",
            "probe.tex",
        ])
        .output()
        .unwrap();
        (
            output.status.success(),
            std::fs::read_to_string(self.0.join("probe.log")).unwrap(),
        )
    }
}

#[test]
#[ignore = "native math-glue oracle: set UMBER_PDFTEX explicitly"]
fn upstream_math_glue_quantities_and_unit_errors() {
    let oracle =
        Oracle(std::env::temp_dir().join(format!("umber-math-glue-oracle-{}", std::process::id())));
    std::fs::create_dir_all(&oracle.0).unwrap();
    std::fs::write(
        oracle.0.join("texmf.cnf"),
        b"TEXINPUTS = .\nshell_escape = f\nmax_print_line = 10000\n",
    )
    .unwrap();
    for (body, expected) in cases() {
        let (success, log) = oracle.run(body);
        assert!(success, "{log}");
        let actual = log
            .split("MU:")
            .skip(1)
            .map(|tail| tail.split_once(":END").unwrap().0)
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{log}");
    }
    for body in incompatible_cases() {
        let (success, log) = oracle.run(body);
        assert!(
            !success && log.contains("Incompatible glue units"),
            "{body}: {log}"
        );
    }
    for unit in ["pt", "sp", "em", "ex", "truemu", "fil"] {
        let (success, log) = oracle.run(&format!(r"\muskip0=0{unit}"));
        assert!(
            !success && log.contains("Illegal unit of measure"),
            "{unit}: {log}"
        );
    }
}
