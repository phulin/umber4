//! Explicit native comparisons reuse the existing independent DVI reader.
use super::*;
use crate::{InputFile, InputResolver, Outcome};
use std::{path::PathBuf, process::Command};
struct Oracle {
    dir: PathBuf,
}
impl Oracle {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "umber-leaders-oracle-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("texmf.cnf"),
            "TEXINPUTS=.\nTFMFONTS=.\nopenin_any=a\nshell_escape=f\nmax_print_line=10000\n",
        )
        .unwrap();
        Self { dir }
    }
    fn compare(&self, source: &str, out: &Outcome) {
        std::fs::write(self.dir.join("probe.tex"), source).unwrap();
        let result =
            Command::new(std::env::var_os("UMBER_PDFTEX").expect("explicit native oracle"))
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
        let log = std::fs::read_to_string(self.dir.join("probe.log")).unwrap();
        assert!(result.status.success(), "{source}\n{log}");
        let messages: Vec<_> = log
            .split("CHECK:")
            .skip(1)
            .map(|s| s.split_once(":END").unwrap().0)
            .collect();
        let ours: Vec<_> = out
            .diagnostics
            .iter()
            .filter_map(|d| {
                let s = std::str::from_utf8(d.bytes()).unwrap();
                s.strip_prefix("CHECK:")
                    .and_then(|s| s.strip_suffix(":END"))
            })
            .collect();
        assert_eq!(messages, ours, "{source}\n{log}");
        let native = crate::page_dvi_tests::read(
            &std::fs::read(self.dir.join("probe.dvi")).unwrap(),
            &out.pages,
        );
        let ours: Vec<Vec<_>> = out.pages.iter().map(|p| p.placements().collect()).collect();
        assert_eq!(native, ours, "{source}\n{log}");
    }
}
impl Drop for Oracle {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
#[test]
#[ignore = "native leaders: set UMBER_PDFTEX"]
fn upstream_leaders_axes_origins_rules_and_glue_rounding() {
    let oracle = Oracle::new();
    for body in cases() {
        let source = source(&body);
        let out = Engine::new(source.as_bytes()).run().unwrap();
        oracle.compare(&source, &out);
    }
    for body in [
        r"\setbox0=\hbox{\leaders\hbox{\kern1pt}\vskip2pt}",
        r"\setbox0=\hbox{\leaders\hrule\penalty0}",
        r"\leaders\hbox{\kern1pt}\hskip2pt",
    ] {
        let source = format!("{BOOT}{body}");
        let mut e = Engine::new(source.as_bytes());
        assert_eq!(e.advance(usize::MAX), Err(Error::Syntax));
        std::fs::write(oracle.dir.join("probe.tex"), &source).unwrap();
        let result = Command::new(std::env::var_os("UMBER_PDFTEX").unwrap())
            .current_dir(&oracle.dir)
            .env("TEXMFCNF", &oracle.dir)
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
        assert!(!result.status.success());
        assert!(
            String::from_utf8_lossy(&result.stdout).contains("Leaders not followed by proper glue")
        );
    }
    for body in [
        r"\noindent\vrule width7pt height2pt\cleaders\hbox{\vrule width2pt height1pt}\hskip5pt plus1fil\vrule width7pt height2pt\par",
        r"\noindent\vrule width10pt height2pt\cleaders\hbox{\vrule width2pt height1pt}\hskip2pt\vrule width10pt height2pt\par",
        r"\noindent\vrule width2pt height2pt\leaders\hbox{\vrule width2pt height1pt}\hskip0pt plus1fil\par",
        r"\leaders\hbox{\vrule width2pt height1pt}\vskip4pt\hrule width3pt height2pt\vskip3pt\par",
        r"\hsize=12pt\baselineskip=12pt\noindent\vrule width10pt height2pt\leaders\hbox{\vrule width2pt height9pt}\hskip1pt\vrule width10pt height2pt\par",
    ] {
        let source = format!(r"{BOOT}\hsize=20pt{body}\end");
        let out = Engine::new(source.as_bytes()).run().unwrap();
        oracle.compare(&source, &out);
    }
}
struct Inputs {
    plain: Vec<u8>,
    hyphen: Vec<u8>,
}
impl InputResolver for Inputs {
    fn resolve(&self, requested: &[u8], _: &[u8]) -> Option<InputFile<'_>> {
        match requested {
            b"plain" | b"plain.tex" => Some(InputFile {
                name: b"plain.tex",
                bytes: &self.plain,
            }),
            b"hyphen" | b"hyphen.tex" => Some(InputFile {
                name: b"hyphen.tex",
                bytes: &self.hyphen,
            }),
            _ => None,
        }
    }
}
#[test]
#[ignore = "native plain leaders: set UMBER_PDFTEX and UMBER_TFM_DIR"]
fn upstream_actual_plain_dotfill_and_hrulefill_with_cm() {
    let oracle = Oracle::new();
    let root = PathBuf::from(std::env::var_os("UMBER_TFM_DIR").expect("fonts/tfm/public"));
    let inputs = Inputs {
        plain: std::fs::read(root.join("../../../tex/plain/base/plain.tex")).unwrap(),
        hyphen: std::fs::read(root.join("../../../tex/generic/hyphen/hyphen.tex")).unwrap(),
    };
    std::fs::write(oracle.dir.join("plain.tex"), &inputs.plain).unwrap();
    std::fs::write(oracle.dir.join("hyphen.tex"), &inputs.hyphen).unwrap();
    let mut fonts: Vec<_> = std::fs::read_dir(root.join("cm"))
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "tfm"))
        .collect();
    fonts.push(root.join("knuth-lib/manfnt.tfm"));
    for body in [
        r"\hbox to123pt{Introduction \dotfill{} i}",
        r"\hbox to5in{Getting Started\dotfill 1}",
        r"\hbox to5in{All Characters Great and Small\hrulefill 9}",
        r"\hbox to121pt{\kern-1pt\hbox to70pt{\dotfill}\dotfill\kern2pt}",
        r"\hbox{$a\cleaders\hbox{$\mathsurround=0pt\mkern1.5mu.\mkern1.5mu$}\hskip20pt b$}",
    ] {
        let source = format!(
            r"\input plain \setbox0={body}\message{{CHECK:BOX:\number\wd0,\number\ht0,\number\dp0:END}}\shipout\box0\end"
        );
        let mut e = Engine::with_inputs(b"probe.tex", source.as_bytes(), &inputs);
        for path in &fonts {
            let bytes = std::fs::read(path).unwrap();
            let name = path.file_stem().unwrap().as_encoded_bytes();
            e.register_tfm(name, &bytes).unwrap();
            std::fs::write(oracle.dir.join(path.file_name().unwrap()), bytes).unwrap();
        }
        e.run_input()
            .unwrap_or_else(|error| panic!("{body}: {error:?}"));
        oracle.compare(&source, &e.outcome());
    }
}
