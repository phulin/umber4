//! Explicit native adapter; ordinary tests need neither files nor a TeX install.
use crate::{
    Engine,
    font_test_data::{basic, tfm, tfm_range},
    nodes::{Kind, List},
    numeric::{Scaled, print_scaled},
    token::*,
};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
fn scaled(s: Scaled) -> String {
    let mut b = Vec::new();
    print_scaled(s, |c| b.push(c));
    String::from_utf8(b).unwrap()
}
fn byte(c: u8) -> String {
    if (32..127).contains(&c) {
        (c as char).to_string()
    } else if c < 128 {
        format!("^^{}", (c ^ 64) as char)
    } else {
        format!("^^{c:02x}")
    }
}
fn components(e: &Engine<'_>, list: &List) -> String {
    let mut s = String::new();
    let mut p = list.head();
    while !p.is_null() {
        let n = e.document.nodes.get(p);
        if let Kind::Character { character, .. } = n.kind {
            s.push_str(&byte(character));
        }
        p = n.next;
    }
    s
}
fn shape(e: &Engine<'_>) -> Vec<String> {
    let mut lines = Vec::new();
    let mut p = e.document.text.horizontal.head();
    while !p.is_null() {
        let n = e.document.nodes.get(p);
        match &n.kind {
            Kind::Character { character, .. } => lines.push(format!(".\\f {}", byte(*character))),
            Kind::Ligature {
                character,
                components: list,
                left_boundary,
                right_boundary,
                ..
            } => lines.push(format!(
                ".\\f {} (ligature {}{}{})",
                byte(*character),
                if *left_boundary { "|" } else { "" },
                components(e, list),
                if *right_boundary { "|" } else { "" }
            )),
            Kind::Kern { width, explicit } => lines.push(format!(
                ".\\kern{}{}",
                if *explicit { " " } else { "" },
                scaled(*width)
            )),
            Kind::Glue { spec, subtype } => {
                let label = match subtype {
                    crate::nodes::GlueKind::SpaceSkip => "(\\spaceskip)",
                    crate::nodes::GlueKind::XSpaceSkip => "(\\xspaceskip)",
                    _ => "",
                };
                let mut s = format!(".\\glue{label} {}", scaled(spec.width));
                for (v, label, order) in [
                    (spec.stretch, " plus ", spec.stretch_order),
                    (spec.shrink, " minus ", spec.shrink_order),
                ] {
                    if v.0 != 0 {
                        s.push_str(label);
                        s.push_str(&scaled(v));
                        if order != crate::numeric::GlueOrder::Normal {
                            s.push_str(std::str::from_utf8(order.suffix()).unwrap());
                        }
                    }
                }
                lines.push(s);
            }
            Kind::Discretionary { .. } => {} // hbox oracle suppresses explicit-hyphen discs
            other => panic!("unexpected test node: {other:?}"),
        }
        p = n.next;
    }
    lines
}
struct Oracle {
    dir: PathBuf,
    binary: PathBuf,
}
impl Drop for Oracle {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
impl Oracle {
    fn new() -> Self {
        let binary = std::env::var_os("UMBER_PDFTEX")
            .expect("set UMBER_PDFTEX to an explicit native oracle")
            .into();
        let dir = std::env::temp_dir().join(format!("umber-font-oracle-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("texmf.cnf"),
            b"TEXINPUTS = .\nTFMFONTS = .\nshell_escape = f\nmax_print_line = 10000\n",
        )
        .unwrap();
        Self { dir, binary }
    }
    fn run(&self, bytes: &[u8], source: &str) -> String {
        std::fs::write(self.dir.join("test.tfm"), bytes).unwrap();
        std::fs::write(self.dir.join("probe.tex"), source).unwrap();
        let output = Command::new(&self.binary)
            .current_dir(&self.dir)
            .env("TEXMFCNF", &self.dir)
            .env("TFMFONTS", &self.dir)
            .args([
                "-ini",
                "-etex",
                "-no-shell-escape",
                "-interaction=nonstopmode",
                "probe.tex",
            ])
            .output()
            .unwrap();
        let log = std::fs::read_to_string(self.dir.join("probe.log")).unwrap();
        assert!(
            log.contains("No pages of output."),
            "{}\n{log}",
            String::from_utf8_lossy(&output.stdout)
        );
        log
    }
    fn compare_shape(&self, bytes: &[u8], body: &str) {
        let source = format!(
            "\\catcode123=1 \\catcode125=2 \\font\\f=test \\f\\noindent {body}\\relax\\end"
        );
        let mut e = Engine::new(source.as_bytes());
        e.register_tfm(b"test", bytes).unwrap();
        loop {
            let r = e.next_expanded().unwrap();
            if r.meaning.command == cmd::END {
                break;
            }
            e.execute(r).unwrap();
        }
        let expected = shape(&e);
        let source = format!(
            "\\catcode123=1 \\catcode125=2 \\newlinechar=-1 \\font\\f=test \\f\\showboxbreadth=10000\\showboxdepth=10000\\setbox0=\\hbox{{{body}}}\\showbox0\\end\n"
        );
        let log = self.run(bytes, &source);
        let observed: Vec<String> = log
            .lines()
            .filter(|l| {
                l.starts_with(".\\f ") || l.starts_with(".\\kern") || l.starts_with(".\\glue")
            })
            .map(str::to_owned)
            .collect();
        assert_eq!(expected, observed, "body {body}\n{log}");
    }
    fn compare_metrics(&self, bytes: &[u8]) {
        let mut source = String::from("\\catcode123=1 \\catcode125=2 ");
        for size in [
            "",
            "at 3.14159pt",
            "at 127.99999pt",
            "at 2047.99998pt",
            "scaled 1200",
        ] {
            source.push_str(&format!("\\font\\f=test {size}\\f "));
            for c in [0, 1, 12, 32, 65, 86, 97, 102, 105, 127, 200, 255] {
                source.push_str(&format!("\\message{{UMBER:{c}:\\number\\fontcharwd\\f{c}/\\number\\fontcharht\\f{c}/\\number\\fontchardp\\f{c}/\\number\\fontcharic\\f{c}:END}}\n"));
            }
            for n in 1..=7 {
                source.push_str(&format!(
                    "\\message{{UMBER:p{n}:\\number\\fontdimen{n}\\f:END}}\n"
                ));
            }
        }
        source.push_str("\\end\n");
        let mut e = Engine::new(source.as_bytes());
        e.register_tfm(b"test", bytes).unwrap();
        let expected = e.run().unwrap().messages;
        let log = self.run(bytes, &source);
        let observed: Vec<Vec<u8>> = log
            .split("UMBER:")
            .skip(1)
            .map(|s| format!("UMBER:{}:END", s.split(":END").next().unwrap()).into_bytes())
            .collect();
        assert_eq!(expected.len(), observed.len());
        for (a, b) in expected.iter().zip(&observed) {
            assert_eq!(String::from_utf8_lossy(a), String::from_utf8_lossy(b));
        }
    }
}
#[test]
#[ignore = "native oracle adapter: set UMBER_PDFTEX and UMBER_TFM_DIR explicitly"]
fn upstream_font_metrics_and_horizontal_nodes() {
    let oracle = Oracle::new();
    let bytes = basic();
    oracle.compare_metrics(&bytes);
    oracle.compare_shape(&bytes, "fi ffi AV a-b");
    for body in [
        r"\sfcode`\.=3000 a. A. ",
        r"\spacefactor=2000\spaceskip=5pt plus 3pt minus 2pt\fontdimen7\f=1pt\ ",
        r"\xspaceskip=9pt\spacefactor=2000\kern0pt{} ",
    ] {
        oracle.compare_shape(&bytes, body);
    }
    for op in [0, 1, 2, 3, 5, 6, 7, 11] {
        let bytes = tfm(&[[128, b'b', op, b'c']], &[(b'a', 1, 0)]);
        oracle.compare_shape(&bytes, "ab");
    }
    let bytes = tfm(
        &[
            [255, b'|', 0, 1],
            [128, b'a', 0, b'c'],
            [128, b'|', 0, b'd'],
            [255, 0, 0, 1],
        ],
        &[(b'a', 1, 2)],
    );
    for body in ["a", r"\noboundary a\noboundary", r"a\noboundary"] {
        oracle.compare_shape(&bytes, body);
    }
    let bytes = tfm(&[[255, 200, 0, 1], [128, 200, 0, b'd']], &[(b'a', 1, 1)]);
    for body in ["a", r"a\char200a"] {
        oracle.compare_shape(&bytes, body);
    }
    for (first, character) in [(64, 32), (0, 200), (0, 0)] {
        for op in [0, 1, 2, 3, 5, 6, 7, 11, 128] {
            let bytes = tfm_range(
                first,
                127,
                &[
                    [255, character, 0, 1],
                    [128, character, op, if op == 128 { 0 } else { b'c' }],
                    [255, 0, 0, 1],
                ],
                &[],
            );
            oracle.compare_shape(&bytes, &format!("\\char{character}\\relax\\count0=7"));
        }
    }
    let root = std::env::var_os("UMBER_TFM_DIR")
        .expect("set UMBER_TFM_DIR to the explicit corpus public/ directory");
    for name in [
        "ae/aer10.tfm",
        "latex-fonts/lcmss8.tfm",
        "latex-fonts/icmtt8.tfm",
    ] {
        let bytes = std::fs::read(Path::new(&root).join(name)).unwrap();
        oracle.compare_metrics(&bytes);
        oracle.compare_shape(&bytes, "fi ffi fl ff AV To a-b");
    }
}
