use crate::{Diagnostic, DiagnosticDestination, Engine, Error, Progress, scalars::Target};

const BOOT: &str = "\\catcode123=1\\catcode125=2\\catcode35=6\\catcode94=7\\relax\n";
const INTEGERS: &[&str] = &[
    "hbadness",
    "vbadness",
    "binoppenalty",
    "relpenalty",
    "displaywidowpenalty",
    "predisplaypenalty",
    "tracinglostchars",
    "uchyph",
    "newlinechar",
    "delimiterfactor",
    "showboxbreadth",
    "showboxdepth",
    "errorcontextlines",
];
const DIMENSIONS: &[&str] = &[
    "overfullrule",
    "splitmaxdepth",
    "boxmaxdepth",
    "delimitershortfall",
    "nulldelimiterspace",
    "scriptspace",
];
const GLUES: &[&str] = &[
    "abovedisplayskip",
    "abovedisplayshortskip",
    "belowdisplayskip",
    "belowdisplayshortskip",
    "splittopskip",
];

fn state_cases() -> Vec<(String, Vec<&'static str>)> {
    let mut cases = Vec::new();
    for (names, initial, value, operand, unit) in [
        (INTEGERS, "0", "-7", "2", ""),
        (DIMENSIONS, "0.0pt", "-7sp", "2sp", "sp"),
        (GLUES, "0.0pt", "-7sp plus2fil minus4fill", "2sp", "sp"),
    ] {
        for name in names {
            let body = format!(
                r"\let\alias=\{name}
\message{{[12345[\the\alias]54321]}}
\alias={value}\advance\alias by{operand}\multiply\alias by3\divide\alias by2
\message{{[12345[\number\alias]54321]}}
{{\alias=8{unit}\relax}}\message{{[12345[\number\alias]54321]}}
{{\globaldefs=-1\global\alias=9{unit}\relax}}\message{{[12345[\number\alias]54321]}}
{{\globaldefs=1\alias=-3{unit}\relax}}\globaldefs=0
\message{{[12345[\number\alias]54321]}}
\def\report{{\message{{[12345[\number\alias]54321]}}}}
\afterassignment\report\alias=6{unit}\relax"
            );
            cases.push((body, vec![initial, "-7", "-7", "-7", "-3", "6"]));
        }
    }
    cases
}

fn payloads(diagnostics: &[Diagnostic]) -> Vec<Vec<u8>> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            let bytes = diagnostic.bytes();
            bytes
                .strip_prefix(b"[12345[")
                .unwrap()
                .strip_suffix(b"]54321]")
                .unwrap()
                .to_vec()
        })
        .collect()
}

#[test]
fn initialization_parameters_use_existing_assignment_and_quantity_paths() {
    for (body, expected) in state_cases() {
        let source = format!("{BOOT}{body}\\end");
        let outcome = Engine::new(source.as_bytes()).run().unwrap();
        assert_eq!(
            payloads(&outcome.diagnostics),
            expected.iter().map(|s| s.as_bytes()).collect::<Vec<_>>(),
            "{body}"
        );
    }
}

fn newline_cases() -> Vec<(&'static str, &'static str, [&'static [u8]; 2])> {
    vec![
        (r"\newlinechar=-1", "a|b", [b"a|b", b"a|b"]),
        (r"\newlinechar=124", "a|b", [b"a\nb", b"a\nb"]),
        (r"\catcode0=12", "a^^@b", [b"a\nb", b"a\nb"]),
        (r"\catcode1=12\newlinechar=1", "a^^Ab", [b"a\nb", b"a\nb"]),
        (
            r"\catcode1=12\newlinechar=94",
            "a^^Ab",
            [b"a\n\nAb", b"a^^Ab"],
        ),
        (
            r"\catcode1=12\newlinechar=65",
            "a^^Ab",
            [b"a^^\nb", b"a^^Ab"],
        ),
        (r"\newlinechar=102", "a^^ffb", [b"a^^\n\nb", b"a^^ffb"]),
        (r"\newlinechar=32", "a b", [b"a\nb", b"a\nb"]),
        (
            r"\newlinechar=92",
            r"\string\sample",
            [b"\nsample", b"\nsample"],
        ),
        (
            r"\newlinechar=32",
            r"\noexpand\sample",
            [b"\\sample\n", b"\\sample\n"],
        ),
        (
            r"\catcode1=12\newlinechar=256",
            "a^^Ab",
            [b"a^^Ab", b"a^^Ab"],
        ),
        (
            r"\catcode1=12\newlinechar=-2",
            "a^^Ab",
            [b"a^^Ab", b"a^^Ab"],
        ),
    ]
}
fn newline_body(setup: &str, text: &str) -> String {
    format!(
        r"{setup}\message{{[12345[{text}]54321]}}
\immediate\write-1{{[12345[{text}]54321]}}
\immediate\write16{{[12345[{text}]54321]}}"
    )
}

#[test]
fn newline_matches_original_bytes_in_both_diagnostic_destinations() {
    for (setup, text, [message, write]) in newline_cases() {
        let source = format!("{BOOT}{}\\end", newline_body(setup, text));
        let out = Engine::new(source.as_bytes()).run().unwrap();
        assert_eq!(
            payloads(&out.diagnostics),
            [message, write, write],
            "{setup}, {text}"
        );
        assert_eq!(
            out.diagnostics
                .iter()
                .map(Diagnostic::destination)
                .collect::<Vec<_>>(),
            [
                DiagnosticDestination::TerminalAndLog,
                DiagnosticDestination::LogOnly,
                DiagnosticDestination::TerminalAndLog,
            ]
        );
    }
}

#[test]
fn newline_conversion_tokens_and_published_records_keep_their_own_lifetimes() {
    let source = format!(
        r"{BOOT}\newlinechar=92\edef\saved{{\string\sample}}
\newlinechar=-1\message{{\saved}}\relax
{{\newlinechar=92\message{{\saved}}\relax}}
\message{{\saved}}\end"
    );
    let mut engine = Engine::new(source.as_bytes());
    while engine.outcome().diagnostics.is_empty() {
        engine.advance(1).unwrap();
    }
    let before = engine.checkpoint().unwrap();
    let prefix = engine.outcome();
    let expected = [
        Diagnostic::Message(b"\\sample".to_vec()),
        Diagnostic::Message(b"\nsample".to_vec()),
        Diagnostic::Message(b"\\sample".to_vec()),
    ];
    for _ in 0..2 {
        assert_eq!(engine.advance(usize::MAX).unwrap(), Progress::Complete);
        assert_eq!(engine.outcome().diagnostics, expected);
        engine.restore(&before).unwrap();
        assert_eq!(engine.document.scalars[Target::NEWLINECHAR].value, -1);
        assert_eq!(engine.outcome().diagnostics, prefix.diagnostics);
    }
    drop(engine);
    drop(before);
    assert_eq!(prefix.diagnostics, expected[..1]);
}

#[test]
fn initialization_parameter_roots_and_open_group_saves_restore_together() {
    let source = format!(
        r"{BOOT}\hbadness=10\splitmaxdepth=20sp\abovedisplayskip=30sp\relax
{{\hbadness=11\splitmaxdepth=21sp\abovedisplayskip=31sp\relax}}
\message{{\number\hbadness/\number\splitmaxdepth/\number\abovedisplayskip}}\end"
    );
    let mut engine = Engine::new(source.as_bytes());
    engine.advance(9).unwrap();
    let before = engine.checkpoint().unwrap();
    engine.advance(5).unwrap();
    let inside = engine.checkpoint().unwrap();
    for _ in 0..2 {
        assert_eq!(engine.document.level, 1);
        engine.advance(usize::MAX).unwrap();
        assert_eq!(engine.outcome().message_bytes(), [b"10/20/30".to_vec()]);
        engine.restore(&inside).unwrap();
    }
    engine.restore(&before).unwrap();
    engine.advance(usize::MAX).unwrap();
    assert_eq!(engine.outcome().message_bytes(), [b"10/20/30".to_vec()]);
}

#[test]
fn parameter_scan_failures_leave_the_previous_binding() {
    for (name, old, bad, expected) in [
        ("hbadness", "7", "2147483648", Error::Overflow),
        ("splitmaxdepth", "7sp", "16384pt", Error::Overflow),
        ("abovedisplayskip", "7sp", "16384pt", Error::Overflow),
    ] {
        let source = format!("{BOOT}\\{name}={old}\\relax\\{name}={bad} \\count0=9\\end");
        let mut engine = Engine::new(source.as_bytes());
        engine.advance(7).unwrap();
        let checkpoint = engine.checkpoint().unwrap();
        assert_eq!(engine.advance(1), Err(expected));
        assert_eq!(engine.outcome().counts[0], 0);
        let token = crate::token::Token::cs(engine.names.lookup(name.as_bytes()).unwrap());
        let read = crate::token::Read {
            token,
            meaning: engine.resolve(token),
        };
        assert_eq!(engine.internal_int(read).unwrap(), 7);
        engine.restore(&checkpoint).unwrap();
        assert!(engine.can_checkpoint());
    }
}

struct Oracle(std::path::PathBuf);
impl Drop for Oracle {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
impl Oracle {
    fn compare(&self, body: &str) {
        let source = format!("{BOOT}{body}\\end\n");
        let expected = Engine::new(source.as_bytes()).run().unwrap();
        std::fs::write(self.0.join("probe.tex"), source).unwrap();
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
        let log = std::fs::read_to_string(self.0.join("probe.log")).unwrap();
        assert!(output.status.success(), "{log}");
        let actual: Vec<_> = log
            .split("[12345[")
            .skip(1)
            .map(|tail| tail.split_once("]54321]").unwrap().0.as_bytes().to_vec())
            .collect();
        assert_eq!(actual, payloads(&expected.diagnostics), "{log}");
    }
}

#[test]
#[ignore = "native initialization parameter oracle: set UMBER_PDFTEX explicitly"]
fn upstream_initialization_parameters_and_newline_diagnostics() {
    let oracle =
        Oracle(std::env::temp_dir().join(format!("umber-parameter-oracle-{}", std::process::id())));
    std::fs::create_dir_all(&oracle.0).unwrap();
    std::fs::write(
        oracle.0.join("texmf.cnf"),
        b"TEXINPUTS = .\nshell_escape = f\nmax_print_line = 10000\n",
    )
    .unwrap();
    for (body, _) in state_cases() {
        oracle.compare(&body);
    }
    for (setup, text, _) in newline_cases() {
        oracle.compare(&newline_body(setup, text));
    }
}
