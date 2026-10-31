//! Explicit native test adapter: compare packed vertical lists, never engine I/O.
use crate::{
    Engine,
    font_test_data::basic,
    nodes::{GlueKind, GlueSign, Kind, List},
    numeric::{GlueOrder, Scaled, print_scaled},
};
use std::{path::PathBuf, process::Command};
fn scaled(v: Scaled) -> String {
    let mut b = Vec::new();
    print_scaled(v, |c| b.push(c));
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
fn list(e: &Engine<'_>, contents: &List, level: usize, out: &mut Vec<String>) {
    let prefix = ".".repeat(level);
    let mut p = contents.head();
    while !p.is_null() {
        let node = e.document.nodes.get(p);
        let mut children = None;
        let text = match &node.kind {
            Kind::Character { character, .. } => format!("\\f {}", byte(*character)),
            Kind::Ligature {
                character,
                components,
                left_boundary,
                right_boundary,
                ..
            } => {
                let mut q = components.head();
                let mut text = String::new();
                while !q.is_null() {
                    let n = e.document.nodes.get(q);
                    if let Kind::Character { character, .. } = n.kind {
                        text.push_str(&byte(character));
                    }
                    q = n.next;
                }
                format!(
                    "\\f {} (ligature {}{}{})",
                    byte(*character),
                    if *left_boundary { "|" } else { "" },
                    text,
                    if *right_boundary { "|" } else { "" }
                )
            }
            Kind::Box(b) => {
                let mut s = format!(
                    "\\hbox({}+{})x{}",
                    scaled(b.height),
                    scaled(b.depth),
                    scaled(b.width)
                );
                let g = b.glue_set;
                if g.sign != GlueSign::Normal && g.ratio != 0.0 {
                    s.push_str(", glue set ");
                    if g.sign == GlueSign::Shrinking {
                        s.push_str("- ");
                    }
                    s.push_str(&scaled(Scaled((g.ratio * 65536.0).round() as i32)));
                    if g.order != GlueOrder::Normal {
                        s.push_str(std::str::from_utf8(g.order.suffix()).unwrap());
                    }
                }
                if b.shift.0 != 0 {
                    s.push_str(&format!(", shifted {}", scaled(b.shift)));
                }
                children = Some(&b.list);
                s
            }
            Kind::Glue { spec, subtype } => {
                let label = match subtype {
                    GlueKind::ParSkip => "(\\parskip)",
                    GlueKind::LeftSkip => "(\\leftskip)",
                    GlueKind::RightSkip => "(\\rightskip)",
                    GlueKind::ParFillSkip => "(\\parfillskip)",
                    GlueKind::BaselineSkip => "(\\baselineskip)",
                    GlueKind::LineSkip => "(\\lineskip)",
                    GlueKind::SpaceSkip => "(\\spaceskip)",
                    GlueKind::XSpaceSkip => "(\\xspaceskip)",
                    _ => "",
                };
                let mut s = format!("\\glue{label} {}", scaled(spec.width));
                for (v, label, o) in [
                    (spec.stretch, " plus ", spec.stretch_order),
                    (spec.shrink, " minus ", spec.shrink_order),
                ] {
                    if v.0 != 0 {
                        s.push_str(label);
                        s.push_str(&scaled(v));
                        if o != GlueOrder::Normal {
                            s.push_str(std::str::from_utf8(o.suffix()).unwrap());
                        }
                    }
                }
                s
            }
            Kind::Kern { width, explicit } => format!(
                "\\kern{}{}",
                if *explicit { " " } else { "" },
                scaled(*width)
            ),
            Kind::Penalty(p) => format!("\\penalty {p}"),
            Kind::Discretionary {
                pre,
                post,
                replace_count,
            } => {
                assert!(pre.is_empty() && post.is_empty());
                assert_eq!(*replace_count, 0);
                "\\discretionary".into()
            }
            other => panic!("unexpected oracle node {other:?}"),
        };
        out.push(format!("{prefix}{text}"));
        if let Some(child) = children {
            list(e, child, level + 1, out);
        }
        p = node.next;
    }
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
            .expect("explicit native oracle")
            .into();
        let dir =
            std::env::temp_dir().join(format!("umber-paragraph-oracle-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("texmf.cnf"),
            b"TEXINPUTS = .\nTFMFONTS = .\nshell_escape = f\nmax_print_line = 10000\n",
        )
        .unwrap();
        Self { dir, binary }
    }
    fn compare(&self, tfm: &[u8], settings: &str, body: &str) -> String {
        let boot =
            r"\catcode123=1 \catcode125=2 \font\f=test \f\defaulthyphenchar=45\hyphenchar\f=45 ";
        let source = format!("{boot}{settings}{body}\\par\\end");
        let mut e = Engine::new(source.as_bytes());
        e.register_tfm(b"test", tfm).unwrap();
        // Isolate paragraph construction before its synchronous page handoff.
        loop {
            let read = e.next_expanded().unwrap();
            if read.meaning.command == crate::token::cmd::PAR {
                e.break_paragraph().unwrap();
                break;
            }
            e.execute(read).unwrap();
        }
        let mut expected = Vec::new();
        list(&e, &e.document.text.vertical, 1, &mut expected);
        let source = format!(
            "{boot}\\tracingparagraphs=1\\showboxbreadth=10000\\showboxdepth=10000\\setbox0=\\vbox{{{settings}{body}\\par}}\\showbox0\\end\n"
        );
        std::fs::write(self.dir.join("test.tfm"), tfm).unwrap();
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
        let observed: Vec<String> = log
            .split("> \\box0=")
            .last()
            .unwrap()
            .lines()
            .filter(|line| line.starts_with('.'))
            .map(str::to_owned)
            .collect();
        assert_eq!(
            expected, observed,
            "settings {settings}\nbody {body}\n{log}"
        );
        log
    }
}
#[test]
#[ignore = "native paragraph oracle: set UMBER_PDFTEX and UMBER_TFM_DIR"]
fn upstream_paragraph_lines_and_glue() {
    let oracle = Oracle::new();
    let test = basic();
    let common = r"\hsize=70pt\parindent=8pt\parfillskip=0pt plus1fil\baselineskip=12pt\lineskip=1pt\lineskiplimit=0pt\linepenalty=10\hyphenpenalty=50\exhyphenpenalty=50\adjdemerits=10000\doublehyphendemerits=10000\finalhyphendemerits=5000 ";
    for (settings, body) in [
        ("", r"\noindent abc def ghi abc def ghi abc def ghi"),
        (
            r"\leftskip=2pt plus1pt\rightskip=3pt plus2pt\parskip=2pt plus1pt ",
            r"abc def ghi abc def ghi abc def ghi",
        ),
        (
            r"\interlinepenalty=3\clubpenalty=5\widowpenalty=7\brokenpenalty=11 ",
            r"\noindent abc\penalty-10000 def\penalty-10000 ghi",
        ),
        (
            r"\hsize=18pt\tolerance=0\pretolerance=0 ",
            r"\noindent abcdefghijklmnopqrstuvwxyz",
        ),
        (
            r"\hsize=56pt\pretolerance=0\tolerance=0\emergencystretch=30pt ",
            r"\noindent ab cd ef gh ij kl mn op qr st uv wx yz ab cd ef gh ij kl mn op",
        ),
        (
            r"\hsize=50pt\looseness=1 ",
            r"\noindent ab cd ef gh ij kl mn op qr st uv wx yz ab cd ef gh ij kl mn op",
        ),
        (
            r"\hsize=60pt\looseness=-1 ",
            r"\noindent ab cd ef gh ij kl mn op qr st uv wx yz ab cd ef gh ij kl mn op",
        ),
        (
            r"\hangindent=12pt\hangafter=-2 ",
            r"abc def ghi abc def ghi abc def ghi abc def ghi",
        ),
        (
            r"\hangindent=-12pt\hangafter=1 ",
            r"abc def ghi abc def ghi abc def ghi abc def ghi",
        ),
        (
            r"\parshape3 2pt 35pt 5pt 50pt 8pt 60pt ",
            r"abc def ghi abc def ghi abc def ghi abc def ghi",
        ),
        (
            r"\hsize=35pt\pretolerance=10000 ",
            r"\noindent ab-cd-ef-gh ij-kl-mn op-qr-st-uv",
        ),
        (
            r"\hsize=40pt\baselineskip=1pt\lineskip=3pt ",
            r"\noindent abc def ghi abc def ghi",
        ),
        (
            r"\hsize=50pt\spaceskip=4pt plus2fill minus1pt ",
            r"\noindent abc def ghi abc def ghi",
        ),
        (
            r"\hsize=50pt\leftskip=0pt plus1fil\rightskip=0pt plus-1fil ",
            r"\noindent ab cd ef gh ij kl mn",
        ),
        (
            r"\hsize=45pt ",
            r"\noindent fi ffi AV a-b fi ffi AV a-b\kern2pt \penalty10000\hskip4pt def",
        ),
    ] {
        let log = oracle.compare(&test, &format!("{common}{settings}"), body);
        if settings.contains("emergencystretch") {
            assert!(log.contains("@emergencypass"), "{log}");
        }
    }
    for width in [25, 35, 50, 65, 90] {
        for (pretolerance, tolerance, looseness) in [
            (0, 10000, 0),
            (-1, 300, 0),
            (0, 50, 1),
            (0, 100, -1),
            (0, 0, 100),
            (0, -1, -100),
        ] {
            oracle.compare(&test,&format!("{common}\\hsize={width}pt\\pretolerance={pretolerance}\\tolerance={tolerance}\\looseness={looseness}"),r"\noindent ab cd ef gh ij kl mn op qr st uv wx yz ab cd ef gh ij kl mn op");
        }
    }
    oracle.compare(
        &test,
        &format!(
            "{common}\\hsize=40pt\\linepenalty=0\\adjdemerits=0\\spaceskip=5pt plus5pt minus1pt"
        ),
        r"\noindent a a a a a a a a a a a a a a a a a a a a a a a a",
    );
    oracle.compare(&test, common, r"\noindent a\hskip0pt minus1fil");
    oracle.compare(
        &test,
        common,
        r"\noindent a\penalty-10000\hskip2pt\penalty-10000 b",
    );
    let root = PathBuf::from(std::env::var_os("UMBER_TFM_DIR").expect("real TFM corpus directory"));
    for name in [
        "ae/aer10.tfm",
        "latex-fonts/lcmss8.tfm",
        "latex-fonts/icmtt8.tfm",
    ] {
        let bytes = std::fs::read(root.join(name)).unwrap();
        oracle.compare(&bytes,&format!("{common}\\hsize=80pt"),r"\noindent The quick brown fox jumps over the lazy dog. Office affine AV a-b-c. The quick brown fox jumps.");
    }
}
