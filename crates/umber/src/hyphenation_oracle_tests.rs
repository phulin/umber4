//! Native readers and word positions, independently rendered by pdfTeX.
use super::tests::{BOOT, loaded};
use crate::{Engine, font_test_data};
use std::{path::PathBuf, process::Command};

struct Oracle(PathBuf);
impl Drop for Oracle {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
impl Oracle {
    fn new() -> Self {
        let oracle = Self(
            std::env::temp_dir().join(format!("umber-hyphenation-oracle-{}", std::process::id())),
        );
        std::fs::create_dir_all(&oracle.0).unwrap();
        std::fs::write(oracle.0.join("test.tfm"), font_test_data::tfm(&[], &[])).unwrap();
        std::fs::write(
            oracle.0.join("texmf.cnf"),
            b"TEXINPUTS = .\nTFMFONTS = .\nshell_escape = f\nmax_print_line = 10000\n",
        )
        .unwrap();
        oracle
    }
    fn run(&self, body: &str) -> (bool, String) {
        std::fs::write(self.0.join("probe.tex"), format!("{BOOT}{body}\\end\n")).unwrap();
        let output =
            Command::new(std::env::var_os("UMBER_PDFTEX").expect("explicit native oracle"))
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
    fn compare_words(
        &self,
        setup: &str,
        tables: &super::Hyphenation,
        language: u8,
        words: &[&str],
        left: i32,
        right: i32,
    ) {
        let mut body = format!(
            r"{setup}\font\f=test \f\hyphenchar\f=45\newlinechar=-1
\hsize=16383pt\pretolerance=-1\tolerance=-1\hbadness=0\showboxdepth=0
\lefthyphenmin={left}\righthyphenmin={right}\language={language}"
        );
        for (i, word) in words.iter().enumerate() {
            // Like plain's showhyphens: leading glue admits the first word;
            // a forced final pass shows every inserted discretionary in the
            // underfull diagnostic, independently of Umber's pure lookup.
            body.push_str(&format!(
                r"\message{{WORD:{i}:}}\setbox0=\vbox{{\noindent\ {word}\par}}"
            ));
        }
        let (success, log) = self.run(&body);
        assert!(success, "{log}");
        for (i, word) in words.iter().enumerate() {
            let part = log.split_once(&format!("WORD:{i}:")).unwrap().1;
            let observed = part
                .split_once("\\f ")
                .unwrap()
                .1
                .lines()
                .next()
                .unwrap()
                .trim();
            let positions = tables.positions(language, word.as_bytes(), left, right);
            let mut expected = String::new();
            for (j, c) in word.chars().enumerate() {
                if positions & (1u64 << j) != 0 {
                    expected.push('-');
                }
                expected.push(c);
            }
            assert_eq!(observed, expected, "{word}, minima {left}/{right}\n{log}");
        }
    }
}

#[test]
#[ignore = "native hyphenation oracle: set UMBER_PDFTEX and UMBER_HYPHEN_TEX"]
fn upstream_hyphenation_loading_and_lookup() {
    let oracle = Oracle::new();
    for body in [
        r"\message{CHECK:\the\language/\the\lefthyphenmin/\the\righthyphenmin:END}
\language=-2147483647\lefthyphenmin=2147483647\righthyphenmin=-1
{\language=255\global\lefthyphenmin=-7}\advance\language by1\multiply\righthyphenmin by-2
\message{CHECK:\the\language/\the\lefthyphenmin/\the\righthyphenmin:END}",
        r"\outer\def\P{a1b}\outer\def\E{a-b}\def\S{ }
\let\letter=c\chardef\given=98\def\after{\message{CHECK:AFTER:END}}
{\globaldefs=-1\afterassignment\after\global\patterns\relax{\P\S\letter1d}
\afterassignment\after\hyphenation{\E\S a\given-c a\char98-d}}
\message{CHECK:\meaning\patterns/\meaning\hyphenation:END}",
        r"\patterns{ab ab a1b 1.cd.1 2.cd.2 .c1d.}\hyphenation{a-b ab a--bc -ab-c-}",
        r"\lccode`2=`x\patterns{a12b}\hyphenation{a-xb}",
    ] {
        let text = format!("{BOOT}{body}\\end");
        let outcome = Engine::new(text.as_bytes()).run().unwrap();
        let (success, log) = oracle.run(body);
        assert!(success, "{body}\n{log}");
        let observed = log
            .split("CHECK:")
            .skip(1)
            .map(|part| part.split_once(":END").unwrap().0.as_bytes())
            .collect::<Vec<_>>();
        let expected = outcome
            .diagnostics
            .iter()
            .map(|d| {
                d.bytes()
                    .strip_prefix(b"CHECK:")
                    .unwrap()
                    .strip_suffix(b":END")
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(observed, expected, "{body}\n{log}");
    }
    for body in [
        r"\patterns{a1b ab}",
        r"\patterns{a1b a1b}",
        r"\patterns{a12b}",
        r"\chardef\a=97\patterns{\a1b}",
        r"\patterns{\char97b}",
        r"\patterns{{a1b}}",
        r"\patterns{a1b\relax}",
        r"\hyphenation{a1b}",
        r"\hyphenation{\relax}",
    ] {
        let text = format!("{BOOT}{body}\\end");
        assert!(Engine::new(text.as_bytes()).run().is_err());
        let (success, log) = oracle.run(body);
        assert!(!success, "native accepted {body}\n{log}");
    }
    let setup = r"\patterns{a1b ab2c b3c .ab4c a5bc.}\hyphenation{ab a-bc ab-c}";
    oracle.compare_words(
        setup,
        &loaded(setup),
        0,
        &["ab", "abc", "xabc", "abcx", "xabcy"],
        1,
        1,
    );
    let setup = r"\language=255\patterns{a1b}\language=0\patterns{b1c}\hyphenation{a-bc}";
    for language in [0, 1, 255] {
        oracle.compare_words(setup, &loaded(setup), language, &["ab", "bc", "abc"], 1, 1);
    }
    // Native paragraph entry, not a direct test-only freeze call.
    for pretolerance in [-1, 10000] {
        let body = format!(
            r"\font\f=test \f\hsize100pt\parfillskip0pt plus1fil\pretolerance={pretolerance}\noindent ab\par\patterns{{a1b}}"
        );
        let (success, log) = oracle.run(&body);
        assert_eq!(success, pretolerance >= 0, "{log}");
    }

    let original =
        std::fs::read(std::env::var_os("UMBER_HYPHEN_TEX").expect("original hyphen.tex path"))
            .unwrap();
    std::fs::write(oracle.0.join("hyphen.tex"), &original).unwrap();
    let mut source = BOOT.as_bytes().to_vec();
    source.extend_from_slice(&original);
    source.extend_from_slice(b"\\end");
    let mut engine = Engine::new(&source);
    engine.advance(usize::MAX).unwrap();
    let tables = engine.document.hyphenation.clone();
    assert_eq!(tables.exceptions.as_ref().unwrap().entries.len(), 15);
    assert_eq!(tables.patterns.as_ref().unwrap().nodes.len(), 7113);
    let words = [
        "hyphenation",
        "difficult",
        "associate",
        "associates",
        "declination",
        "obligatory",
        "philanthropic",
        "present",
        "presents",
        "project",
        "projects",
        "reciprocity",
        "recognizance",
        "reformation",
        "retribution",
        "table",
        "database",
        "exception",
        "implementation",
        "paragraph",
        "character",
        "mathematics",
        "initialization",
    ];
    for (left, right) in [(1, 1), (2, 3), (0, -7), (63, 63)] {
        oracle.compare_words(r"\input hyphen ", &tables, 0, &words, left, right);
    }
    drop(engine);
    assert!(tables.positions(0, b"hyphenation", 2, 3) != 0);
}
