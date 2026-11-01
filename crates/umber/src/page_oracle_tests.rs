//! Native INITEX page structures and absolute DVI placements, independent of a backend.
use crate::{
    Engine, PackedBox, Placement,
    font_test_data::basic,
    numeric::{Scaled, print_scaled},
};
use std::{path::PathBuf, process::Command};
fn scaled(v: i32) -> String {
    let mut b = Vec::new();
    print_scaled(Scaled(v), |c| b.push(c));
    String::from_utf8(b).unwrap()
}
fn displayed(b: &PackedBox, boxes: &[PackedBox]) -> String {
    let mut s = format!(
        "\\{}box({}+{})x{}",
        if b.vertical { "v" } else { "h" },
        scaled(b.height),
        scaled(b.depth),
        scaled(b.width)
    );
    if b.glue.sign != 0 && b.glue.ratio != 0.0 {
        s.push_str(", glue set ");
        if b.glue.sign < 0 {
            s.push_str("- ");
        }
        s.push_str(&scaled((b.glue.ratio * 65536.0).round() as i32));
        s.push_str(["", "fil", "fill", "filll"][b.glue.order as usize]);
    }
    if let Some(parent) = b.parent {
        let parent = boxes[parent as usize];
        let shift = if parent.vertical {
            b.x - parent.x
        } else {
            b.y - parent.y
        };
        if shift != 0 {
            s.push_str(&format!(", shifted {}", scaled(shift)));
        }
    }
    s
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
        let dir = std::env::temp_dir().join(format!("umber-page-oracle-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("texmf.cnf"),
            b"TEXINPUTS = .\nTFMFONTS = .\nshell_escape = f\nmax_print_line = 10000\n",
        )
        .unwrap();
        Self {
            dir,
            binary: std::env::var_os("UMBER_PDFTEX")
                .expect("explicit native oracle")
                .into(),
        }
    }
    fn compare(&self, tfm: &[u8], settings: &str, body: &str) {
        let boot = r"\catcode123=1\catcode125=2\font\f=test\f\hsize=70pt\vsize=45pt\topskip=10pt\maxdepth=3pt\baselineskip=12pt\lineskip=1pt\parfillskip=0pt plus1fil ";
        let source = format!("{boot}{settings}{body}\\end");
        let mut e = Engine::new(source.as_bytes());
        e.register_tfm(b"test", tfm).unwrap();
        e.advance(usize::MAX).unwrap();
        let outcome = e.outcome();
        // The native-only routine observes outputpenalty and consumes box255 just
        // like default output; it adds no material or assignments to the job.
        let native = format!(
            r"\catcode123=1\catcode125=2\tracingoutput=1\showboxbreadth=10000\showboxdepth=10000\hbadness=10000\hfuzz=16383pt\output{{\message{{PAGEPENALTY:\the\outputpenalty:END}}\shipout\box255}}{source}"
        );
        std::fs::write(self.dir.join("test.tfm"), tfm).unwrap();
        std::fs::write(self.dir.join("probe.tex"), native).unwrap();
        let _ = std::fs::remove_file(self.dir.join("probe.dvi"));
        let output = Command::new(&self.binary)
            .current_dir(&self.dir)
            .env("TEXMFCNF", &self.dir)
            .env("TFMFONTS", &self.dir)
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
        let log = String::from_utf8_lossy(&std::fs::read(self.dir.join("probe.log")).unwrap())
            .into_owned();
        assert!(output.status.success(), "{source}\n{log}");
        let expected: Vec<_> = outcome
            .pages
            .iter()
            .flat_map(|p| p.boxes().iter().map(|b| displayed(b, p.boxes())))
            .collect();
        let actual: Vec<_> = log
            .lines()
            .map(|l| l.trim_start_matches('.'))
            .filter(|l| l.starts_with("\\vbox(") || l.starts_with("\\hbox("))
            .map(str::to_owned)
            .collect();
        assert_eq!(expected, actual, "box structures: {source}\n{log}");
        let messages: Vec<_> = log
            .split("UMBER:")
            .skip(1)
            .map(|s| s.split(":END").next().unwrap().to_owned())
            .collect();
        let expected_messages: Vec<_> = outcome
            .messages
            .iter()
            .filter_map(|b| {
                std::str::from_utf8(b)
                    .unwrap()
                    .strip_prefix("UMBER:")
                    .and_then(|s| s.strip_suffix(":END"))
                    .map(str::to_owned)
            })
            .collect();
        assert_eq!(messages, expected_messages, "visible quantities: {source}");
        let penalties: Vec<i32> = log
            .split("PAGEPENALTY:")
            .skip(1)
            .map(|s| s.split(":END").next().unwrap().trim().parse().unwrap())
            .collect();
        assert_eq!(
            outcome
                .pages
                .iter()
                .map(|p| p.output_penalty)
                .collect::<Vec<_>>(),
            penalties,
            "penalties: {source}"
        );
        if outcome.pages.is_empty() {
            assert!(!self.dir.join("probe.dvi").exists());
            return;
        }
        let bytes = std::fs::read(self.dir.join("probe.dvi")).unwrap();
        let actual = crate::page_dvi_tests::read(&bytes, &outcome.pages);
        let expected: Vec<Vec<Placement>> = outcome
            .pages
            .iter()
            .map(|p| p.placements().collect::<Vec<_>>())
            .collect();
        assert_eq!(expected, actual, "DVI placements: {source}");
    }
}
#[test]
#[ignore = "native page oracle: set UMBER_PDFTEX and UMBER_TFM_DIR"]
fn upstream_page_breaks_boxes_glue_and_absolute_placements() {
    let oracle = Oracle::new();
    let tfm = basic();
    let body = r"\noindent abc def ghi abc def ghi abc def ghi abc def ghi\par\noindent def ghi abc def ghi abc def ghi abc def ghi\par";
    for settings in [
        "",
        r"\vsize=24pt ",
        r"\vsize=18pt\maxdepth=0pt ",
        r"\topskip=1pt ",
        r"\topskip=11pt plus1pt minus1pt ",
        r"\parskip=2pt plus3pt minus1pt ",
        r"\baselineskip=12pt plus1pt minus1pt ",
        r"\hsize=55.123pt\spaceskip=3pt plus7pt minus1pt\parfillskip=0pt ",
        r"\leftskip=3pt plus1fil\rightskip=0pt plus2fil ",
        r"\hangindent=3pt\hangafter=1 ",
    ] {
        oracle.compare(&tfm, settings, body);
    }
    for body in [
        r"\hrule width20pt height4pt depth3pt\penalty-10000 ",
        r"\vskip8pt\kern4pt\penalty-10000\hrule width20pt height4pt\par ",
        r"\hrule width10pt height5pt\par\vsize=80pt\maxdepth=9pt\penalty-12345 ",
        r"\hrule width10pt height80pt\penalty0 ",
        r"\hrule width10pt height5pt depth8pt\penalty-10000 ",
        r"\vsize=10pt\hrule width4pt height2pt\vfil\penalty0\kern0pt\penalty0\pagegoal=1pt\penalty1 ",
        r"\hrule width4pt height3pt\kern7pt\par\vskip2pt\penalty-10000 ",
        r"\noindent abc\vrule width2pt def\par\hrule height1pt\noindent ghi\par ",
        r"\vskip3pt ",
        r"\noindent\par ",
        r"\noindent abc def ghi\vskip7pt\noindent abc def ghi ",
        r"\vsize=20pt\hrule width10pt height2pt\vskip0pt plus3pt\hrule width10pt height2pt\vskip0pt plus3pt\hrule width10pt height2pt\vskip0pt plus3pt\penalty-10000 ",
        r"\vsize=4pt\hrule width10pt height2pt\vskip0pt minus3pt\hrule width10pt height2pt\vskip0pt minus3pt\hrule width10pt height2pt\vskip0pt minus3pt\penalty-10000 ",
    ] {
        oracle.compare(&tfm, "", body);
    }
    oracle.compare(&tfm,"",r"\message{UMBER:\number\pagegoal/\the\pagetotal:END}\hrule width1pt height2pt\par\count0=\pagegoal\count1=\pagetotal\prevdepth=7pt\count2=\prevdepth\message{UMBER:\the\count0/\the\count1/\the\count2:END}\penalty-12345\relax\ifnum\lastpenalty=10000\message{UMBER:\number\outputpenalty/\number\pagegoal:END}\fi\vskip3pt plus2fil\par\skip0=\lastskip\message{UMBER:\the\skip0:END}");
    for (goal, component) in [(5, "plus"), (3, "minus")] {
        oracle.compare(&tfm,"",&format!(r"\topskip=0pt\maxdepth=0pt\vsize={goal}sp\hrule width1sp height2sp\vskip0pt {component}1sp\hrule width1sp height2sp\vskip0pt {component}1sp\penalty-10000"));
    }
    let root = PathBuf::from(std::env::var_os("UMBER_TFM_DIR").expect("real font corpus"));
    for name in [
        "ae/aer10.tfm",
        "latex-fonts/lcmss8.tfm",
        "latex-fonts/icmtt8.tfm",
    ] {
        oracle.compare(&std::fs::read(root.join(name)).unwrap(),r"\hsize=80pt\vsize=50pt ",r"\noindent Office affine AV quick brown fox jumps over the lazy dog. Office affine AV quick brown fox jumps over the lazy dog.\par\noindent Office affine AV quick brown fox jumps.\par");
    }
}
