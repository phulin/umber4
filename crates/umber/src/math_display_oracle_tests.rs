//! Ordinary scanned displays and equation numbers through native geometry/DVI.
use super::*;
use crate::page_oracle_tests::Oracle;

#[test]
#[ignore = "native scanned displays: set UMBER_PDFTEX"]
fn upstream_scanned_display_entry_layout_and_equation_numbers() {
    let oracle = Oracle::new();
    let tfm = metrics();
    let settings = format!(r"{SETUP}{DISPLAY}\vbadness=10000\vfuzz=16383pt ");
    for body in [
        r"\noindent$$a$$\par",
        r"$$a$$\par",
        r"\noindent abc$$a+b$$def ghi\par",
        r"\noindent abc def ghi jkl mno pqr stu vwx yz$$a\over b$$next\par",
        r"\noindent$$\mathchoice{a}{b}{c}{d}$$\par",
        r"\noindent$$$$\par",
        r"\setbox0=\hbox{$$}\noindent\box0\par",
        r"\noindent$$a\eqno b$$\par",
        r"\noindent$$a\leqno b$$\par",
        r"\noindent$$abcdefghij\eqno klmnop$$\par",
        r"\noindent$$abcdefghij\leqno klmnop$$\par",
        r"\noindent$$a\eqno\hbox to0pt{b\hss}$$\par",
        r"\noindent$$a\leqno\hbox to0pt{b\hss}$$\par",
        r"\noindent$$\hskip0pt plus1fil abcdefghij\hskip0pt minus20pt\eqno abc$$\par",
        r"\noindent$$abcdefghij\hskip0pt minus1fil\leqno abc$$\par",
        r"\noindent$$abcdefghij\hskip0pt minus1fill\eqno abc$$\par",
        r"\noindent$$\displaywidth=11sp\displayindent=-3sp\kern2sp$$\par",
        r"\noindent$$\displaywidth=-11sp\kern-20sp$$\par",
        r"\noindent$$\vcenter{\hbox{$\mathchoice{a\over b}{c}{d}{e}$}}\lower2pt\hbox{ab}$$\par",
        r"\setbox0=\vbox{\noindent abc$$a\over b$$def\par}\noindent\box0\par",
    ] {
        oracle.compare(&tfm, &settings, body);
    }
    let diagnostics = r"\everydisplay{\message{UMBER:DISPLAY:\the\predisplaysize/\the\displaywidth/\the\displayindent/\ifinner I\else O\fi:END}} ";
    for (shape, body) in [
        ("", r"\noindent abc$$a$$next\par"),
        ("", r"\noindent\kern2pt$$a$$\par"),
        ("", r"\noindent\hbox{}\kern2pt$$a$$\par"),
        ("", r"\noindent a\hskip0pt plus1fil b$$a$$\par"),
        ("", r"\noindent a\hskip0pt plus1fil $$a$$\par"),
        (
            "",
            r"\noindent a\cleaders\hbox{b}\hskip0pt plus1fil $$a$$\par",
        ),
        (
            r"\hsize=12pt\rightskip=0pt minus20pt ",
            r"\noindent abc$$a$$\par",
        ),
        (
            r"\hsize=12pt ",
            r"\noindent a\hskip0pt minus20pt bc$$a$$\par",
        ),
        (
            r"\hangindent=7pt\hangafter=-2 ",
            r"\noindent ab$$a$$cd ef\par",
        ),
        (
            r"\hangindent=7pt\hangafter=-3 ",
            r"\noindent ab$$a$$cd ef\par",
        ),
        (
            r"\hangindent=-7pt\hangafter=2 ",
            r"\noindent ab$$a$$cd ef\par",
        ),
        (
            r"\parshape3 1pt50pt 2pt40pt 3pt30pt ",
            r"\noindent ab$$a$$cd ef\par",
        ),
        (r"\parshape1 -3pt40pt ", r"\noindent$$a$$cd ef\par"),
        (
            r"\globaldefs=1\fam=6\predisplaysize=9pt ",
            r"\noindent$$a$$\message{UMBER:RESTORED:\the\fam/\the\predisplaysize:END}\par",
        ),
    ] {
        oracle.compare(&tfm, &format!("{settings}{shape}{diagnostics}"), body);
    }
    for body in [
        r"\def\probe{\message{UMBER:EXPANDED:END}a}\noindent\expandafter$\noexpand\probe$\par",
        r"\def\probe{\message{UMBER:EXPANDED:END}a}\noindent$$a$$\noexpand\probe\par",
        r"\def\space{ }\noindent$$a$$\space\space b\par",
        r"\def\close{\ifvmode$\else\undefined\fi}\noindent$$a$\close b\par",
        r"\def\close{\ifmmode\ifinner\undefined\else$\fi\else\undefined\fi}\noindent$$a\eqno b$\close c\par",
        r"\everymath{\message{UMBER:I:\the\fam/\ifinner Y\else N\fi:END}}\everydisplay{\message{UMBER:D:\the\fam/\ifinner Y\else N\fi:END}}\noindent$$a\fam=2\eqno\fam=3 b$$\par",
        r"\noindent$$a\vadjust{\kern4pt}\eqno\hbox to0pt{b\hss}$$\par",
        r"\noindent$$a\vadjust{\kern4pt}\leqno\hbox to0pt{b\hss}$$\par",
        r"\count5=1000\dimen5=100pt\noindent$$a\insert5{\hrule width1pt height2pt}\eqno b$$\par",
        r"\vsize=8pt\topskip=0pt\fam=6\predisplaysize=9pt\output{\message{PAGEPENALTY:\the\outputpenalty:END}\message{UMBER:OUTPUT:\the\fam/\the\predisplaysize/\the\displaywidth:END}\shipout\box255}\everydisplay{\message{UMBER:HOOK:\the\fam/\the\predisplaysize/\the\displaywidth:END}}\noindent ab\penalty-10000 cd$$a$$ef\par",
    ] {
        oracle.compare(&tfm, &settings, body);
    }
}

#[test]
#[ignore = "native unchanged Gentle first display: set UMBER_PDFTEX and UMBER_TFM_DIR"]
fn upstream_unchanged_gentle_first_display_and_plain_macros() {
    let oracle = Oracle::new();
    let root = std::path::PathBuf::from(std::env::var_os("UMBER_TFM_DIR").expect("real CM corpus"));
    let names = [
        "test", "sevenrm", "fiverm", "teni", "seveni", "fivei", "tensy", "sevensy", "fivesy",
        "tenex",
    ];
    let files = [
        "cmr10", "cmr7", "cmr5", "cmmi10", "cmmi7", "cmmi5", "cmsy10", "cmsy7", "cmsy5", "cmex10",
    ];
    let bytes: Vec<_> = files
        .iter()
        .map(|n| std::fs::read(root.join(format!("cm/{n}.tfm"))).unwrap())
        .collect();
    let fonts: Vec<_> = names
        .iter()
        .zip(&bytes)
        .map(|(n, b)| (n.as_bytes(), b.as_slice()))
        .collect();
    let mut settings = String::from(r"\catcode36=3\catcode35=6\catcode64=11\let\tenrm\f ");
    for name in &names[1..] {
        settings.push_str(&format!(r"\font\{name}={name} "));
    }
    for (family, sizes) in [
        ["tenrm", "sevenrm", "fiverm"],
        ["teni", "seveni", "fivei"],
        ["tensy", "sevensy", "fivesy"],
        ["tenex", "tenex", "tenex"],
    ]
    .iter()
    .enumerate()
    {
        for (cmd, font) in ["textfont", "scriptfont", "scriptscriptfont"]
            .iter()
            .zip(sizes)
        {
            settings.push_str(&format!(r"\{cmd}{family}=\{font} "));
        }
    }
    settings.push_str(r#"\hsize=450pt\vsize=600pt\baselineskip=12pt\topskip=10pt\parfillskip=0pt plus1fil\vbadness=10000\vfuzz=16383pt
\dimendef\z@=0\z@=0pt\def\m@th{\mathsurround\z@}\mathcode45="2200
\mathchardef\rightarrow="3221\def\quad{\hskip1em\relax}
\def\TeX{T\kern-.1667em\lower.5ex\hbox{E}\kern-.125emX}
\def\relbar{\mathrel{\smash-}}\def\joinrel{\mathrel{\mkern-3mu}}
\def\longrightarrow{\relbar\joinrel\rightarrow}
\def\mathpalette#1#2{\mathchoice{#1\displaystyle{#2}}%
  {#1\textstyle{#2}}{#1\scriptstyle{#2}}{#1\scriptscriptstyle{#2}}}
\def\smash{\relax % \relax, in case this comes first in \halign
  \ifmmode\def\next{\mathpalette\mathsm@sh}\else\let\next\makesm@sh
  \fi\next}
\def\makesm@sh#1{\setbox\z@\hbox{#1}\finsm@sh}
\def\mathsm@sh#1#2{\setbox\z@\hbox{$\m@th#1{#2}$}\finsm@sh}
\def\finsm@sh{\ht\z@\z@ \dp\z@\z@ \box\z@}
"#);
    let gentle = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../.tools/corpus/gentle.tex"
    ))
    .unwrap();
    let makebox = gentle.find(r"\def\makebox#1#2#3").unwrap();
    let macro_end = gentle[makebox..].find(r"\def\displaytext").unwrap() + makebox;
    settings.push_str(&gentle[makebox..macro_end]);
    let nucleus = gentle.find(r"{\hbox{edit text\quad}").unwrap();
    let start = gentle[..nucleus].rfind("$$").unwrap();
    let end = gentle[nucleus..].find("$$").unwrap() + nucleus + 2;
    oracle.compare_fonts(
        &fonts,
        &settings,
        &format!("\\noindent{}\\par", &gentle[start..end]),
    );
}
