use super::*;
use crate::{Progress, scanner::Scanner, text::Mode};
const BOOT: &str = "\\catcode123=1 \\catcode125=2 \\catcode35=6\\relax\n";
struct Bundle<'a>(&'a [(&'a [u8], &'a [u8])]);
impl InputResolver for Bundle<'_> {
    fn resolve(&self, requested: &[u8], _: &[u8]) -> Option<InputFile<'_>> {
        self.0
            .iter()
            .find(|(name, _)| *name == requested || name.strip_suffix(b".tex") == Some(requested))
            .map(|&(name, bytes)| InputFile { name, bytes })
    }
}
struct Case {
    main: &'static str,
    child_name: &'static str,
    child: &'static str,
    expected: &'static [&'static str],
}
fn cases() -> Vec<Case> {
    vec![
        Case {
            main: r"\def\run#1{\message{CHECK:BEFORE:END}\input child.tex \message{CHECK:AFTER:END}#1}\run{\message{CHECK:ARG:END}}",
            child_name: "child.tex",
            child: "\\message{CHECK:CHILD:END}\n",
            expected: &["BEFORE", "CHILD", "AFTER", "ARG"],
        },
        Case {
            main: "\\message{CHECK:PARENT-FIRST:END}\n\\endinput \\input child.tex \\message{CHECK:PARENT-SAME:END}\n\\message{CHECK:PARENT-LATER:END}",
            child_name: "child.tex",
            child: "\\message{CHECK:CHILD-FIRST:END}\n\\message{CHECK:CHILD-LATER:END}\n",
            expected: &["PARENT-FIRST", "CHILD-FIRST", "PARENT-SAME", "PARENT-LATER"],
        },
        Case {
            main: r"\input child.tex \message{CHECK:PARENT:END}\endgroup\fi",
            child_name: "child.tex",
            child: "\\iftrue\\begingroup\\message{CHECK:CHILD:END}\n",
            expected: &["CHILD", "PARENT"],
        },
        Case {
            main: r"\def\name{child}\input\relax{\name}\message{CHECK:AFTER:END}",
            child_name: "child.tex",
            child: "\\message{CHECK:BRACED:END}\n",
            expected: &["BRACED", "AFTER"],
        },
        Case {
            main: r#"\input "child space.tex" \message{CHECK:AFTER:END}"#,
            child_name: "child space.tex",
            child: "\\message{CHECK:QUOTED:END}\n",
            expected: &["QUOTED", "AFTER"],
        },
        Case {
            main: r"\outer\def\name{child.tex}\input\name \message{CHECK:AFTER:END}",
            child_name: "child.tex",
            child: "\\message{CHECK:OUTER-NAME:END}\n",
            expected: &["OUTER-NAME", "AFTER"],
        },
        Case {
            main: r"\let\X=c\input\X hild.tex \message{CHECK:AFTER:END}",
            child_name: "child.tex",
            child: "\\message{CHECK:ALIAS:END}\n",
            expected: &["ALIAS", "AFTER"],
        },
        Case {
            main: r"\input child.tex \message{CHECK:EMPTY:END}",
            child_name: "child.tex",
            child: "",
            expected: &["EMPTY"],
        },
        Case {
            main: r"\input child.tex \message{CHECK:AFTER:END}",
            child_name: "child.tex",
            child: "\\iffalse\\never\\else\\ifdefined\\never\\else\\message{CHECK:LOOKUP:END}\\fi\\fi\n",
            expected: &["LOOKUP", "AFTER"],
        },
        Case {
            main: r"\immediate\write16{CHECK:WRITE-\input child.tex AFTER:END}\message{CHECK:PARENT:END}",
            child_name: "child.tex",
            child: "CHILD-\\string\n",
            expected: &["WRITE-CHILD-AFTER", "PARENT"],
        },
    ]
}
fn source(body: &str) -> String {
    format!("{BOOT}{body}\\end\n")
}
fn payloads(out: &crate::Outcome) -> Vec<&[u8]> {
    out.diagnostics
        .iter()
        .map(|d| {
            d.bytes()
                .strip_prefix(b"CHECK:")
                .unwrap()
                .strip_suffix(b":END")
                .unwrap()
        })
        .collect()
}
#[test]
fn nested_files_preserve_macros_names_lookup_eof_and_write_order() {
    for case in cases() {
        let files = [(case.child_name.as_bytes(), case.child.as_bytes())];
        let bundle = Bundle(&files);
        let text = source(case.main);
        let out = Engine::with_inputs(b"main.tex", text.as_bytes(), &bundle)
            .run()
            .unwrap();
        assert_eq!(
            payloads(&out),
            case.expected
                .iter()
                .map(|s| s.as_bytes())
                .collect::<Vec<_>>(),
            "{}",
            case.main
        );
    }
}
#[test]
fn resolver_receives_exact_bytes_and_current_including_name() {
    struct Observed(std::cell::RefCell<Vec<(Vec<u8>, Vec<u8>)>>);
    impl InputResolver for Observed {
        fn resolve(&self, requested: &[u8], including: &[u8]) -> Option<InputFile<'_>> {
            self.0
                .borrow_mut()
                .push((requested.to_vec(), including.to_vec()));
            match requested {
                b"child" => Some(InputFile {
                    name: b"directory/child.tex",
                    bytes: b"\\input \xff.tex \\relax\n",
                }),
                b"\xff.tex" => Some(InputFile {
                    name: b"directory/\xff.tex",
                    bytes: b"\\count0=7\\relax\n",
                }),
                _ => None,
            }
        }
    }
    let bundle = Observed(std::cell::RefCell::new(Vec::new()));
    let out = Engine::with_inputs(b"directory/main.tex", b"\\input child \\end", &bundle)
        .run()
        .unwrap();
    assert_eq!(out.counts[0], 7);
    assert_eq!(
        *bundle.0.borrow(),
        [
            (b"child".to_vec(), b"directory/main.tex".to_vec()),
            (b"\xff.tex".to_vec(), b"directory/child.tex".to_vec())
        ]
    );
}
#[test]
fn file_input_waits_until_font_size_scanning_has_finished() {
    let files = [(b"child.tex".as_slice(), b"\\count0=9\\relax\n".as_slice())];
    let bundle = Bundle(&files);
    for separator in ["", " "] {
        let text = format!("\\font\\f=test{separator}\\input child.tex \\end");
        let mut engine = Engine::with_inputs(b"main.tex", text.as_bytes(), &bundle);
        engine
            .register_tfm(b"test", &crate::font_test_data::basic())
            .unwrap();
        engine.advance(1).unwrap();
        assert!(engine.document.suspended_files.is_empty());
        assert_eq!(engine.outcome().counts[0], 0);
        engine.run_input().unwrap();
        assert_eq!(engine.outcome().counts[0], 9);
        assert!(!engine.name_in_progress);
    }
}

fn errors() -> [(&'static str, &'static str); 4] {
    [
        (
            "\\def\\unfinished{CHILD\n",
            "File ended while scanning definition",
        ),
        (
            "\\long\\def\\grab#1{#1}\\grab{CHILD\n",
            "File ended while scanning use",
        ),
        ("\\message{CHILD\n", "File ended while scanning text"),
        ("\\iffalse\n", "Incomplete \\iffalse"),
    ]
}
#[test]
fn included_eof_restores_parent_before_rejecting_scanner_context() {
    for (child, _) in errors() {
        let files = [(b"child.tex".as_slice(), child.as_bytes())];
        let bundle = Bundle(&files);
        let text = source(r"\input child.tex \count0=99");
        let mut engine = Engine::with_inputs(b"main.tex", text.as_bytes(), &bundle);
        assert_eq!(engine.run_input(), Err(Error::Eof), "{child}");
        assert_eq!(engine.document.file.name, b"main.tex");
        assert!(engine.document.suspended_files.is_empty());
        assert_eq!(engine.scanner, Scanner::Normal);
        assert_eq!(engine.document.scalars.counts()[0].value, 0);
        assert!(!engine.can_checkpoint());
    }
}
#[test]
fn write_failures_unwind_active_children_without_replacing_the_error() {
    for (child, error) in [
        ("CHILD\n", Error::Eof),
        ("CHILD}TAIL\n", Error::Syntax),
        ("\\undefined\n", Error::Undefined),
    ] {
        let files = [(b"child.tex".as_slice(), child.as_bytes())];
        let bundle = Bundle(&files);
        let text = source(r"\immediate\write16{CHECK:\input child.tex AFTER:END}\count0=99");
        let mut engine = Engine::with_inputs(b"main.tex", text.as_bytes(), &bundle);
        let saved = engine.checkpoint().unwrap();
        for _ in 0..2 {
            assert_eq!(engine.run_input(), Err(error), "{child}");
            assert_eq!(engine.document.file.name, b"main.tex");
            assert!(engine.document.suspended_files.is_empty());
            assert!(
                !engine
                    .document
                    .sources
                    .iter()
                    .any(|s| matches!(s, Source::FileBoundary | Source::WriteBoundary { .. }))
            );
            assert_eq!(engine.document.text.mode, Mode::Vertical);
            assert_eq!(engine.scanner, Scanner::Normal);
            assert!(engine.outcome().diagnostics.is_empty());
            engine.restore(&saved).unwrap();
        }
    }
}
#[test]
fn snapshots_retain_nested_cursors_pending_arguments_and_global_force_eof() {
    let files = [(b"child.tex".as_slice(), b"\\def\\stop{\\endinput\\message{CHECK:MID:END}\\count0=1\\relax}\\stop\n\\message{CHECK:SKIPPED:END}\n".as_slice())];
    let bundle = Bundle(&files);
    let text = source(r"\def\run#1{\input child.tex #1}\run{\message{CHECK:PARENT:END}}");
    let mut engine = Engine::with_inputs(b"main.tex", text.as_bytes(), &bundle);
    while engine.outcome().counts[0] == 0 {
        engine.advance(1).unwrap();
    }
    assert!(engine.document.force_eof);
    assert_eq!(engine.document.file.name, b"child.tex");
    let saved = engine.checkpoint().unwrap();
    let prefix = engine.outcome();
    for _ in 0..3 {
        while engine.advance(16).unwrap() != Progress::Complete {}
        let out = engine.outcome();
        assert_eq!(payloads(&out), [b"MID".as_slice(), b"PARENT".as_slice()]);
        assert_eq!(engine.document.file.name, b"main.tex");
        engine.restore(&saved).unwrap();
        assert_eq!(engine.outcome().diagnostics, prefix.diagnostics);
    }
    drop(engine);
    drop(saved);
    assert_eq!(payloads(&prefix), [b"MID".as_slice()]);
}
#[test]
fn nested_prepared_lines_replay_committed_translation_and_catcode_changes() {
    let files = [(
        b"child.tex".as_slice(),
        b"\\catcode94=12\\count0=1\\relax\n".as_slice(),
    )];
    let bundle = Bundle(&files);
    let text = source(
        r"\catcode94=7\catcode43=13\def+{\message{CHECK:PLUS:END}}\def\load{\input child.tex }\load^^2b",
    );
    let mut engine = Engine::with_inputs(b"main.tex", text.as_bytes(), &bundle);
    while engine.outcome().counts[0] == 0 {
        engine.advance(1).unwrap();
    }
    let saved = engine.checkpoint().unwrap();
    for _ in 0..2 {
        engine.run_input().unwrap();
        assert_eq!(payloads(&engine.outcome()), [b"PLUS".as_slice()]);
        engine.restore(&saved).unwrap();
    }
}
fn forbidden_names() -> [&'static str; 2] {
    [
        r"\outer\def\name{child.tex}\input{\name}",
        r"\outer\def\name{child.tex}\edef\result{\input\name }",
    ]
}
#[test]
fn filename_scanning_inherits_context_and_braced_names_absorb() {
    let files = [(b"child.tex".as_slice(), b"\\relax\n".as_slice())];
    let bundle = Bundle(&files);
    for body in forbidden_names() {
        let text = source(body);
        let mut engine = Engine::with_inputs(b"main.tex", text.as_bytes(), &bundle);
        assert_eq!(engine.run_input(), Err(Error::Outer));
        assert_eq!(engine.scanner, Scanner::Normal);
        assert!(!engine.name_in_progress);
        assert!(engine.document.suspended_files.is_empty());
        assert!(engine.name_spills.iter().all(|name| name.is_empty()));
    }
}

#[test]
fn missing_input_and_main_eof_fail_without_prompting_or_new_checkpoint() {
    let bundle = Bundle(&[]);
    let mut engine = Engine::with_inputs(b"main", b"\\input absent \\end", &bundle);
    let saved = engine.checkpoint().unwrap();
    assert_eq!(engine.run_input(), Err(Error::Resource));
    assert!(engine.document.suspended_files.is_empty());
    assert!(!engine.name_in_progress);
    engine.restore(&saved).unwrap();
    assert!(engine.can_checkpoint());
    assert_eq!(
        Engine::new(b"\\endinput\n\\end").run().err(),
        Some(Error::Eof)
    );
}
struct Oracle(std::path::PathBuf);
impl Drop for Oracle {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
impl Oracle {
    fn run(&self, main: &str, name: &str, child: &str) -> (bool, String) {
        std::fs::write(self.0.join("main.tex"), source(main)).unwrap();
        std::fs::write(self.0.join(name), child).unwrap();
        let out = std::process::Command::new(
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
            "main.tex",
        ])
        .output()
        .unwrap();
        (
            out.status.success(),
            std::fs::read_to_string(self.0.join("main.log")).unwrap(),
        )
    }
}
#[test]
#[ignore = "native immutable-input oracle: set UMBER_PDFTEX explicitly"]
fn upstream_nested_input_force_eof_scanners_and_write_boundaries() {
    let oracle =
        Oracle(std::env::temp_dir().join(format!("umber-input-oracle-{}", std::process::id())));
    std::fs::create_dir_all(&oracle.0).unwrap();
    std::fs::write(
        oracle.0.join("texmf.cnf"),
        b"TEXINPUTS=.\nTFMFONTS=.\nshell_escape=f\nmax_print_line=10000\n",
    )
    .unwrap();
    for case in cases() {
        let (success, log) = oracle.run(case.main, case.child_name, case.child);
        assert!(success, "{log}");
        assert_eq!(
            log.split("CHECK:")
                .skip(1)
                .map(|s| s.split_once(":END").unwrap().0)
                .collect::<Vec<_>>(),
            case.expected,
            "{log}"
        );
    }
    std::fs::write(oracle.0.join("test.tfm"), crate::font_test_data::basic()).unwrap();
    for separator in ["", " "] {
        let body = format!(r"\font\f=test{separator}\input child.tex \message{{CHECK:AFTER:END}}");
        let child = "X\\relax\\message{CHECK:CHILD:END}\n";
        let files = [(b"child.tex".as_slice(), child.as_bytes())];
        let bundle = Bundle(&files);
        let text = source(&body);
        let mut engine = Engine::with_inputs(b"main.tex", text.as_bytes(), &bundle);
        engine
            .register_tfm(b"test", &crate::font_test_data::basic())
            .unwrap();
        let expected = engine.run().unwrap();
        let (success, log) = oracle.run(&body, "child.tex", child);
        assert!(success, "{log}");
        assert_eq!(
            log.split("CHECK:")
                .skip(1)
                .map(|s| s.split_once(":END").unwrap().0.as_bytes())
                .collect::<Vec<_>>(),
            payloads(&expected),
            "{log}"
        );
    }
    for body in forbidden_names() {
        let (success, log) = oracle.run(body, "child.tex", "\\relax\n");
        assert!(
            !success && log.contains("Forbidden control sequence"),
            "{log}"
        );
    }
    for (child, diagnostic) in errors() {
        let (success, log) = oracle.run(r"\input child.tex \relax", "child.tex", child);
        assert!(
            !success && log.contains(diagnostic) && log.contains("l.2 \\input child.tex"),
            "{log}"
        );
    }
}
