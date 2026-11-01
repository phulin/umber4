use crate::{Engine, Error, font_test_data::basic, scalars::Target, token::*};

const BOOT: &str = "\\catcode123=1\\catcode125=2\\catcode35=6\\relax\n";

fn engine(source: &[u8]) -> Engine<'_> {
    let mut engine = Engine::new(source);
    engine.register_tfm(b"test", &basic()).unwrap();
    engine
}

fn cases() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        (
            r"\font\a=test\a\edef\saved{\the\font}
{\font\local=test }\let\alias=\a
\message{[12345[\saved/\expandafter\string\saved/\ifdefined\local Y\else N\fi]54321]}
\escapechar=33\message{[12345[\saved/\expandafter\string\saved/\expandafter\meaning\saved]54321]}",
            vec!["\\local /\\local/N", "!local /!local/select font test"],
        ),
        (
            r"\message{[12345[\the\fam/\meaning\textfont/\meaning\scriptfont/\meaning\scriptscriptfont/\the\textfont15]54321]}
\let\family=\fam\family=-2147483647\advance\family by1
\message{[12345[\number\family]54321]}",
            vec![
                "0/\\textfont/\\scriptfont/\\scriptscriptfont/\\nullfont ",
                "-2147483646",
            ],
        ),
        (
            r"\font\original=test \original
\edef\frozen{\the\font}\edef\ordinary{\original}\let\copy=\original
\font\original=test at12pt
\expandafter\let\expandafter\captured\frozen
\message{[12345[\expandafter\string\frozen/\expandafter\meaning\frozen/\meaning\original/\ifx\captured\copy Y\else N\fi/\ifx\frozen\ordinary Y\else N\fi]54321]}
\message{[12345[\meaning\frozen/\expandafter\ifdefined\frozen Y\else N\fi]54321]}
\font\later=test \let\ignored=\later
\message{[12345[\frozen/\expandafter\string\frozen]54321]}
\frozen\edef\now{\the\font}
\message{[12345[\ifx\now\frozen Y\else N\fi]54321]}",
            vec![
                "\\original/select font test/select font test at 12.0pt/Y/N",
                "macro:->\\original /Y",
                "\\later /\\later",
                "Y",
            ],
        ),
        (
            r"\font\a=test \font\b=test at12pt
\let\family=\textfont\chardef\index=15
\family0=\a\scriptfont\index=\b\scriptscriptfont7=\family0
\fontdimen2\scriptfont15=3pt\hyphenchar\scriptscriptfont7=42
\message{[12345[\the\textfont0/\the\scriptfont15/\the\scriptscriptfont7/\the\fontdimen2\scriptfont15/\number\hyphenchar\textfont0]54321]}
\b\textfont1=\font\edef\saved{\the\textfont1}\toks0=\expandafter{\saved}
\message{[12345[\the\toks0/\expandafter\meaning\saved]54321]}",
            vec!["\\a /\\b /\\a /3.0pt/42", "\\b /select font test at 12.0pt"],
        ),
        (
            r"\font\a=test \font\b=test at12pt\textfont0=\a\fam=-1
{\textfont0=\b\fam=17\message{[12345[\the\textfont0/\the\fam]54321]}}
\message{[12345[\the\textfont0/\the\fam]54321]}
{\textfont0=\b{\global\textfont0=\b}\global\fam=-2}
{\globaldefs=-1\global\textfont0=\a\global\fam=19}
\message{[12345[\the\textfont0/\the\fam]54321]}
{\globaldefs=1\textfont0=\a\fam=-3}\globaldefs=0
\def\report{\message{[12345[\the\textfont0/\the\fam]54321]}}
\afterassignment\report\textfont0=\b",
            vec!["\\b /17", "\\a /-1", "\\b /-2", "\\b /-3"],
        ),
        (
            r"\edef\null{\the\nullfont}\let\nullfont=\relax
\font\!=test \!\edef\saved{\the\font}
\message{[12345[\saved/\expandafter\string\saved]54321]}
\catcode126=13\font~=test
\message{[12345[\saved/\expandafter\string\saved]54321]}
\expandafter\font\csname\endcsname=test
\message{[12345[\saved/\expandafter\string\saved/\null/\expandafter\meaning\null]54321]}
\escapechar=-1\message{[12345[\saved/\expandafter\string\saved]54321]}",
            vec![
                "\\! /\\!",
                "\\FONT~ /\\FONT~",
                "\\FONT /\\FONT/\\nullfont /select font nullfont",
                "FONT /FONT",
            ],
        ),
        (
            r"\font\a=test\a\edef\saved{\the\font}
\edef\quoted{\noexpand\a}\edef\protected{\unexpanded{\a}}
\message{[12345[\ifx\quoted\protected Y\else N\fi/\ifx\saved\quoted Y\else N\fi/\expandafter\detokenize\expandafter{\saved}]54321]}
\edef\terminate{\noexpand\count0=12\the\font}\terminate
\message{[12345[\number\count0/\the\font]54321]}",
            vec!["Y/N/\\a ", "12/\\a "],
        ),
    ]
}

fn payloads(outcome: &crate::Outcome) -> Vec<Vec<u8>> {
    outcome
        .diagnostics
        .iter()
        .map(|diagnostic| {
            diagnostic
                .bytes()
                .strip_prefix(b"[12345[")
                .unwrap()
                .strip_suffix(b"]54321]")
                .unwrap()
                .to_vec()
        })
        .collect()
}

#[test]
fn font_identifiers_preserve_identity_spelling_and_assignment_semantics() {
    for (body, expected) in cases() {
        let source = format!("{BOOT}{body}\\end");
        let actual = engine(source.as_bytes()).run().unwrap();
        assert_eq!(
            payloads(&actual),
            expected.iter().map(|s| s.as_bytes()).collect::<Vec<_>>(),
            "{body}"
        );
    }
}

#[test]
fn all_family_slots_start_null_and_remain_distinct_font_bindings() {
    let mut source = format!("{BOOT}\\font\\a=test \\font\\b=test at12pt ");
    for name in ["textfont", "scriptfont", "scriptscriptfont"] {
        for family in 0..16 {
            source.push_str(&format!(
                r"\{name}{family}=\{} ",
                if family % 2 == 0 { "a" } else { "b" }
            ));
        }
    }
    source.push_str(r"\end");
    let mut engine = engine(source.as_bytes());
    assert_eq!(engine.document.scalars[Target::FAM].value, 0);
    for style in 0..3 {
        for family in 0..16 {
            assert_eq!(
                engine.document.scalars[Target::family(style, family)].value,
                0
            );
        }
    }
    engine.advance(usize::MAX).unwrap();
    for style in 0..3 {
        for family in 0..16 {
            assert_eq!(
                engine.document.scalars[Target::family(style, family)].value,
                1 + family as i32 % 2
            );
        }
    }
}

#[test]
fn frozen_namespace_has_no_name_binding_and_cannot_alias_sentinels() {
    let mut engine = engine(b"\\font\\a=test \\a \\font");
    engine.advance(2).unwrap();
    let names = engine.names.len();
    let bindings = engine.document.bindings.len();
    let crate::lists::Internal::Font(token) = engine.the_value().unwrap() else {
        panic!("font token")
    };
    assert_eq!(token.font_id(), Some(1));
    assert!(token.is_cs());
    assert!(!token.is_parameter());
    assert!(!token.is_plain_character());
    for ordinary in [
        FROZEN_RELAX,
        FROZEN_END_WRITE,
        Token(2 * CS),
        Token::cs(CS - 1),
    ] {
        assert_ne!(ordinary, token);
        assert_eq!(ordinary.font_id(), None);
    }
    assert_eq!(
        engine.resolve(token),
        Meaning {
            command: cmd::SELECT_FONT,
            value: 1
        }
    );
    assert_eq!(engine.names.len(), names);
    assert_eq!(engine.document.bindings.len(), bindings);
    assert_eq!(
        Token::font(i32::MAX as usize).unwrap().font_id(),
        Some(i32::MAX as usize)
    );
    assert_eq!(Token::font(i32::MAX as usize + 1), Err(Error::Capacity));
}

#[test]
fn checkpoint_roots_restore_font_spelling_families_and_open_groups() {
    let source = format!(
        r"{BOOT}\font\a=test \a\textfont0=\a\edef\saved{{\the\font}}\count0=1\relax
{{\font\alias=test \font\b=test at12pt\textfont0=\b\fam=-7\count0=2\relax
\message{{[12345[\saved/\the\textfont0/\the\fam]54321]}}}}
\message{{[12345[\saved/\the\textfont0/\the\fam]54321]}}\end"
    );
    let mut engine = engine(source.as_bytes());
    while engine.outcome().counts[0] != 1 {
        engine.advance(1).unwrap();
    }
    let before = engine.checkpoint().unwrap();
    while engine.outcome().counts[0] != 2 {
        engine.advance(1).unwrap();
    }
    let inside = engine.checkpoint().unwrap();
    for _ in 0..2 {
        assert_eq!(engine.document.level, 1);
        assert_eq!(engine.document.fonts.len, 3);
        engine.advance(usize::MAX).unwrap();
        assert_eq!(
            payloads(&engine.outcome()),
            [
                b"\\alias /\\b /-7".to_vec(),
                b"\\alias /\\alias /0".to_vec()
            ]
        );
        engine.restore(&inside).unwrap();
    }
    engine.restore(&before).unwrap();
    assert_eq!(engine.document.fonts.len, 2);
    assert_eq!(engine.document.scalars[Target::family(0, 0)].value, 1);
    assert_eq!(
        engine
            .names
            .spelling(engine.document.fonts.descriptors[1].identifier),
        b"a"
    );
    engine.advance(usize::MAX).unwrap();
    assert_eq!(engine.outcome().diagnostics.len(), 2);
    drop(engine);
    drop(inside);
    drop(before);
}

fn invalid_cases() -> Vec<(&'static str, Error)> {
    vec![
        (r"\textfont-1=\nullfont", Error::Number),
        (r"\scriptfont16=\nullfont", Error::Number),
        (r"\message{\the\scriptscriptfont16}", Error::Number),
        (r"\textfont0=7", Error::Syntax),
        (r"\textfont0=\relax\nullfont", Error::Syntax),
        (r"\fontdimen1\relax\nullfont=0pt", Error::Syntax),
        (r"\hyphenchar\relax\nullfont=0", Error::Syntax),
        (r"\count0=\textfont0", Error::Number),
        (r"\message{\number\textfont0}", Error::Number),
        (r"\advance\textfont0 by1", Error::Unsupported),
        (r"\multiply\scriptfont0 by2", Error::Unsupported),
        (r"\dimen0=\textfont0", Error::Number),
        (
            r"\edef\saved{\the\font}\expandafter\def\saved{}",
            Error::Syntax,
        ),
        (
            r"\edef\saved{\the\font}\expandafter\let\saved=\nullfont",
            Error::Syntax,
        ),
        (
            r"\edef\saved{\the\font}\expandafter\font\saved=test",
            Error::Syntax,
        ),
    ]
}

#[test]
fn indices_numeric_coercion_and_frozen_assignment_targets_fail() {
    for (body, error) in invalid_cases() {
        let source = format!("{BOOT}\\textfont0=\\nullfont\\count1=1\\relax{body}\\end");
        let mut engine = engine(source.as_bytes());
        while engine.outcome().counts[1] != 1 {
            engine.advance(1).unwrap();
        }
        let before = engine.checkpoint().unwrap();
        assert_eq!(engine.advance(usize::MAX), Err(error), "{body}");
        assert_eq!(engine.document.scalars[Target::family(0, 0)].value, 0);
        assert!(!engine.can_checkpoint());
        engine.restore(&before).unwrap();
        assert!(engine.can_checkpoint());
    }
}

#[test]
fn failed_family_scans_keep_old_value_and_check_index_before_operand_expansion() {
    for (body, error) in [
        (
            r"\textfont16=\csname operandcreated\endcsname",
            Error::Number,
        ),
        (r"\textfont0=\iftrue 7\fi", Error::Syntax),
        (
            r"\textfont0=\relax\csname operandcreated\endcsname",
            Error::Syntax,
        ),
    ] {
        let source = format!("{BOOT}\\font\\a=test \\textfont0=\\a\\count0=1\\relax{body}\\end");
        let mut engine = engine(source.as_bytes());
        while engine.outcome().counts[0] == 0 {
            engine.advance(1).unwrap();
        }
        let saved = engine.checkpoint().unwrap();
        assert_eq!(engine.advance(usize::MAX), Err(error));
        assert_eq!(engine.document.scalars[Target::family(0, 0)].value, 1);
        assert_eq!(engine.names.lookup(b"operandcreated"), None);
        engine.restore(&saved).unwrap();
        assert!(engine.document.conditions.is_empty());
        assert!(engine.can_checkpoint());
    }
}

#[test]
fn font_identifier_relax_error_consumes_only_the_invalid_identifier() {
    let mut engine = engine(br"\relax\nullfont");
    assert_eq!(engine.scan_font_ident(), Err(Error::Syntax));
    let next = engine.next_token().unwrap();
    assert_eq!(next.meaning.command, cmd::SELECT_FONT);
    assert_eq!(next.meaning.value, 0);
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
        let output =
            std::process::Command::new(std::env::var_os("UMBER_PDFTEX").expect("explicit oracle"))
                .current_dir(&self.0)
                .env("TEXMFCNF", &self.0)
                .env("TFMFONTS", &self.0)
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
#[ignore = "native font-identity/family oracle: set UMBER_PDFTEX explicitly"]
fn upstream_font_identifiers_and_families() {
    let oracle = Oracle(
        std::env::temp_dir().join(format!("umber-font-state-oracle-{}", std::process::id())),
    );
    std::fs::create_dir_all(&oracle.0).unwrap();
    std::fs::write(oracle.0.join("test.tfm"), basic()).unwrap();
    std::fs::write(
        oracle.0.join("texmf.cnf"),
        b"TEXINPUTS = .\nTFMFONTS = .\nshell_escape = f\nmax_print_line = 10000\n",
    )
    .unwrap();
    for (body, _) in cases() {
        let (success, log) = oracle.run(body);
        assert!(success, "{body}\n{log}");
        let observed = log
            .split("[12345[")
            .skip(1)
            .map(|s| s.split_once("]54321]").unwrap().0.as_bytes().to_vec())
            .collect::<Vec<_>>();
        let source = format!("{BOOT}{body}\\end");
        assert_eq!(
            observed,
            payloads(&engine(source.as_bytes()).run().unwrap()),
            "{body}\n{log}"
        );
    }
    for (body, _) in invalid_cases() {
        let (success, log) = oracle.run(body);
        assert!(!success, "native accepted {body}\n{log}");
    }
}
