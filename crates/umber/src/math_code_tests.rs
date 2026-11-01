use crate::{Engine, Error, Progress, scalars::Target, token::*};

const GROUPS: &str = r"\catcode123=1\catcode125=2 ";

fn meaning(engine: &Engine<'_>, name: &[u8]) -> Meaning {
    engine.document.bindings[engine.names.lookup(name).unwrap() as usize].meaning()
}

#[test]
fn initex_math_and_delimiter_codes_cover_every_byte() {
    let engine = Engine::new(b"");
    for byte in 0..=255u8 {
        let expected = i32::from(byte)
            + if byte.is_ascii_alphabetic() {
                0x7100
            } else if byte.is_ascii_digit() {
                0x7000
            } else {
                0
            };
        assert_eq!(
            engine.document.scalars[Target::mathcode(byte as usize)].value,
            expected
        );
        assert_eq!(
            engine.document.scalars[Target::delcode(byte as usize)].value,
            if byte == b'.' { 0 } else { -1 }
        );
        assert_eq!(
            engine.document.scalars[Target::sfcode(byte as usize)].value,
            if byte.is_ascii_uppercase() { 999 } else { 1000 }
        );
    }
}

fn valid_state_cases() -> Vec<String> {
    vec![
        r#"\mathcode0=0 \mathcode255="8000 \delcode0=-2147483647 \delcode255="FFFFFF
\mathchardef\zero=0 \mathchardef\last="7FFF
\count0=\mathcode255 \count1=\delcode0 \dimen0=\last sp
\message{UMBER-MATH:LIMITS:\the\mathcode0/\number\mathcode255/\the\delcode0/\number\delcode255/\the\zero/\number\last/\number\count0/\number\count1/\number\dimen0:END}
\message{UMBER-MATH:MEANING:\meaning\zero/\meaning\last:END}"#.into(),
        r#"\outer\def\value{42}\mathchardef\old=7
\mathchardef\old=\ifx\old\relax\value\else 9\fi
\let\alias=\old \mathchardef\same=42 \chardef\text=42
\message{UMBER-MATH:IDENTITY:\number\old/\ifx\old\alias Y\else N\fi/\ifx\old\same Y\else N\fi/\ifx\old\text Y\else N\fi/\meaning\alias:END}"#.into(),
        r#"\mathcode65=1 \delcode65=-2 \mathchardef\saved=3
{\mathcode65=4 \delcode65=-5 \mathchardef\saved=6
\message{UMBER-MATH:LOCAL:\the\mathcode65/\the\delcode65/\number\saved:END}}
\message{UMBER-MATH:RESTORED:\the\mathcode65/\the\delcode65/\number\saved:END}
{\mathcode65=4 \delcode65=-5 \mathchardef\saved=6
{\global\mathcode65=7 \global\delcode65=-8 \global\mathchardef\saved=9}}
\message{UMBER-MATH:GLOBAL:\the\mathcode65/\the\delcode65/\number\saved:END}
{\globaldefs=-1 \global\mathcode65=10 \global\delcode65=-11 \global\mathchardef\saved=12}
\message{UMBER-MATH:FORCEDLOCAL:\the\mathcode65/\the\delcode65/\number\saved:END}
{\globaldefs=1 \mathcode65=13 \delcode65=-14 \mathchardef\saved=15}
\globaldefs=0
\message{UMBER-MATH:FORCEDGLOBAL:\the\mathcode65/\the\delcode65/\number\saved:END}"#.into(),
    ]
}

#[test]
fn math_code_limits_meanings_aliases_and_group_levels() {
    let expected = [
        &[
            "UMBER-MATH:LIMITS:0/32768/-2147483647/16777215/0/32767/32768/-2147483647/32767:END",
            "UMBER-MATH:MEANING:\\mathchar\"0/\\mathchar\"7FFF:END",
        ][..],
        &["UMBER-MATH:IDENTITY:42/Y/Y/N/\\mathchar\"2A:END"][..],
        &[
            "UMBER-MATH:LOCAL:4/-5/6:END",
            "UMBER-MATH:RESTORED:1/-2/3:END",
            "UMBER-MATH:GLOBAL:7/-8/9:END",
            "UMBER-MATH:FORCEDLOCAL:7/-8/9:END",
            "UMBER-MATH:FORCEDGLOBAL:13/-14/15:END",
        ][..],
    ];
    for (body, expected) in valid_state_cases().iter().zip(expected) {
        let source = format!("{GROUPS}{body}\\end");
        let out = Engine::new(source.as_bytes()).run().unwrap();
        assert_eq!(
            out.messages,
            expected.iter().map(|s| s.as_bytes()).collect::<Vec<_>>()
        );
    }
}

#[test]
fn math_code_checkpoints_retain_open_group_and_meaning_roots() {
    let source = format!(
        r"{GROUPS}\mathcode65=100\delcode65=-100\mathchardef\saved=300\relax
{{\mathcode65=101\delcode65=-101\mathchardef\saved=301\relax}}\relax
\mathcode65=102\delcode65=-102\mathchardef\saved=302\end"
    );
    let mut engine = Engine::new(source.as_bytes());
    assert_eq!(engine.advance(6).unwrap(), Progress::Boundary);
    let before = engine.checkpoint().unwrap();
    assert_eq!(engine.advance(5).unwrap(), Progress::Boundary);
    let inside = engine.checkpoint().unwrap();
    for _ in 0..2 {
        assert_eq!(engine.document.scalars[Target::mathcode(65)].value, 101);
        assert_eq!(engine.document.scalars[Target::delcode(65)].value, -101);
        assert_eq!(
            meaning(&engine, b"saved"),
            Meaning {
                command: cmd::MATH_GIVEN,
                value: 301
            }
        );
        assert_eq!(engine.advance(2).unwrap(), Progress::Boundary);
        assert_eq!(engine.document.scalars[Target::mathcode(65)].value, 100);
        assert_eq!(engine.document.scalars[Target::delcode(65)].value, -100);
        assert_eq!(meaning(&engine, b"saved").value, 300);
        assert_eq!(engine.advance(usize::MAX).unwrap(), Progress::Complete);
        assert_eq!(engine.document.scalars[Target::mathcode(65)].value, 102);
        assert_eq!(meaning(&engine, b"saved").value, 302);
        engine.restore(&inside).unwrap();
    }
    engine.restore(&before).unwrap();
    assert_eq!(engine.document.scalars[Target::mathcode(65)].value, 100);
    assert_eq!(engine.advance(usize::MAX).unwrap(), Progress::Complete);
}

#[test]
fn invalid_math_codes_preserve_assignment_and_scanning_failure_order() {
    for (command, target, value) in [
        ("mathcode", Target::mathcode(65), "-1"),
        ("mathcode", Target::mathcode(65), "32769"),
        ("delcode", Target::delcode(65), "16777216"),
    ] {
        let source = format!("\\{command}65=7\\relax\\{command}65={value} \\count0=9\\end");
        let mut engine = Engine::new(source.as_bytes());
        engine.advance(2).unwrap();
        let saved = engine.checkpoint().unwrap();
        assert_eq!(engine.advance(1), Err(Error::Number));
        assert_eq!(engine.document.scalars[target].value, 7);
        assert_eq!(engine.outcome().counts[0], 0);
        assert!(!engine.can_checkpoint());
        engine.restore(&saved).unwrap();
        assert_eq!(engine.document.scalars[target].value, 7);
        assert!(engine.can_checkpoint());
    }
    for command in ["mathcode", "delcode"] {
        let source = format!("\\{command}256=\\unread\\end");
        let mut engine = Engine::new(source.as_bytes());
        assert_eq!(engine.advance(1), Err(Error::Number));
        assert!(engine.names.lookup(b"unread").is_none());
        assert_eq!(engine.next_token().unwrap().token, Token::other(b'='));
    }
    for value in ["-1", "32768", "\\undefinedvalue"] {
        let source = format!(
            r"{GROUPS}\outer\def\victim{{OLD}}\relax\mathchardef\victim={value} \count0=9\end"
        );
        let mut engine = Engine::new(source.as_bytes());
        engine.advance(4).unwrap();
        let saved = engine.checkpoint().unwrap();
        let expected = if value.starts_with('\\') {
            Error::Undefined
        } else {
            Error::Number
        };
        assert_eq!(engine.advance(1), Err(expected));
        assert_eq!(meaning(&engine, b"victim"), Meaning::RELAX);
        assert_eq!(engine.outcome().counts[0], 0);
        engine.restore(&saved).unwrap();
        assert!(meaning(&engine, b"victim").is_outer_macro());
    }
}

#[test]
fn math_character_execution_never_enters_text_or_creates_a_glyph() {
    for before in ["", "\\noindent"] {
        let source = format!("\\mathchardef\\m=65{before}\\m\\end");
        let mut engine = Engine::new(source.as_bytes());
        assert_eq!(engine.advance(usize::MAX), Err(Error::Unsupported));
        assert!(engine.document.text.horizontal.is_empty());
        assert!(engine.outcome().pages.is_empty());
    }
}

struct Oracle(std::path::PathBuf);
impl Drop for Oracle {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
impl Oracle {
    fn run(&self, body: &str) -> (std::process::ExitStatus, String) {
        let source = format!("{GROUPS}\\newlinechar=-1 {body}\\end\n");
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
            "probe.tex",
        ])
        .output()
        .unwrap();
        (
            output.status,
            std::fs::read_to_string(self.0.join("probe.log")).unwrap(),
        )
    }
}

#[test]
#[ignore = "native math-code oracle: set UMBER_PDFTEX explicitly"]
fn upstream_math_codes_defaults_limits_and_scoped_meanings() {
    let oracle =
        Oracle(std::env::temp_dir().join(format!("umber-math-code-oracle-{}", std::process::id())));
    std::fs::create_dir_all(&oracle.0).unwrap();
    std::fs::write(
        oracle.0.join("texmf.cnf"),
        b"TEXINPUTS = .\nshell_escape = f\nmax_print_line = 10000\n",
    )
    .unwrap();
    let mut defaults = String::new();
    for byte in 0..=255 {
        defaults.push_str(&format!(
            r"\message{{UMBER-MATH:DEFAULT{byte}:\the\mathcode{byte}/\the\delcode{byte}:END}}"
        ));
        defaults.push('\n');
    }
    for body in std::iter::once(defaults).chain(valid_state_cases()) {
        let source = format!("{GROUPS}{body}\\end");
        let expected = Engine::new(source.as_bytes()).run().unwrap().messages;
        let (status, log) = oracle.run(&body);
        assert!(status.success(), "{log}");
        let actual: Vec<_> = log
            .split("UMBER-MATH:")
            .skip(1)
            .map(|tail| {
                let text = tail.split_once(":END").unwrap().0;
                format!("UMBER-MATH:{text}:END").into_bytes()
            })
            .collect();
        assert_eq!(actual, expected, "{log}");
    }
    for (body, diagnostic) in [
        (
            r"\mathcode65=-1",
            "Invalid code (-1), should be in the range 0..32768",
        ),
        (
            r"\mathcode65=32769",
            "Invalid code (32769), should be in the range 0..32768",
        ),
        (
            r"\delcode65=16777216",
            "Invalid code (16777216), should be at most 16777215",
        ),
        (r"\mathchardef\m=-1", "Bad mathchar (-1)"),
        (r"\mathchardef\m=32768", "Bad mathchar (32768)"),
    ] {
        let (status, log) = oracle.run(body);
        assert!(!status.success() && log.contains(diagnostic), "{log}");
    }
}
