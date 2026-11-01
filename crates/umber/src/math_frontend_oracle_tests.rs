//! Same-source inline construction is compared with native box geometry and DVI.
use super::{SETUP, metrics};
use crate::page_oracle_tests::Oracle;

#[test]
#[ignore = "native inline math: set UMBER_PDFTEX"]
fn upstream_inline_math_frontend_geometry_and_dvi() {
    let oracle = Oracle::new();
    let tfm = metrics();
    for body in [
        r"$^{1}$", // First Gentle footnote marker, with an empty nucleus.
        r"$a_b^c\mathop{ab}\limits_{}^{cd}$",
        r"${a}b{cd}e{}^{}\mathord{f}i$",
        r"$a\kern2pt^b\textstyle_c$",
        r"$a\mathop b\limits^c\mathop c\nolimits_d\mathop e\displaylimits_f$",
        r"$\displaystyle a_b^c\textstyle a_b^c\scriptstyle a_b^c\scriptscriptstyle a_b^c$",
        r"$\fam=1 a\fam=0 b\fam=16 c\fam=-2 d$",
        r#"$\mathcode97="0061 a\fam=0\mathcode98="7162 b\mathchar"7163\chardef\d=100\d\char101$"#,
        r"$a\textfont1=\s b\textfont1=\ss c$",
        r"$a{\textfont1=\s b}c$",
        r"$a^{\hbox{c$^{d}$}}\hbox{$ab$}$",
        r"$a\raise2pt\hbox{$b$}\lower1pt\vbox{\hbox{c}}_d$",
        r"\setbox0=\hbox{a}$\copy0\box0\box0^b\hbox{$$}$",
        r"$a\/b\kern1pt c\hskip2pt plus1fil\penalty50 d$",
        r"$a\vrule width3pt height4pt depth2pt b$",
        r"$\mkern1.5mu a\scriptstyle\mkern1.5mu b\scriptscriptstyle\mkern1.5mu c$",
        r"$a_{\mkern1.5mu b}^{\mkern-1.5mu c}\kern2pt\mkern2mu d$",
        r"\everymath{\ifmmode\message{UMBER:M:END}\fi\ifinner\message{UMBER:I:END}\fi}$\ifmmode a\else b\fi$",
        r"\mathsurround=1pt$\mathsurround=-2pt a$\message{UMBER:\the\mathsurround:END}",
        r#"\catcode126=13\outer\def~{\mathchar97 }\catcode126=12\mathcode126="8000$~^~\mathop~\limits_~$"#,
        r"\def\shift{$}$a\shift b\shift c$",
        r"\def\shift{$}$\shift",
        r"\def\after{\mathchar98 }$a\begingroup\aftergroup\after\endgroup c$",
        r"$a^{b\aftergroup c}d$",
    ] {
        oracle.compare(&tfm, SETUP, &format!(r"\noindent\hbox{{{body}}}\par"));
    }
    for class in [
        "mathord",
        "mathop",
        "mathbin",
        "mathrel",
        "mathopen",
        "mathclose",
        "mathpunct",
        "mathinner",
    ] {
        for surround in [-2, 0, 3] {
            oracle.compare(
                &tfm,
                SETUP,
                &format!(r"\noindent\hbox{{\mathsurround={surround}pt$a\{class} b^c d$}}\par"),
            );
        }
    }
    for (quad, kern) in [
        (-589824, 1),
        (-589824, -1),
        (-1179630, 65536),
        (1179630, -65536),
    ] {
        let settings = format!(r"{SETUP}\fontdimen6\f={quad}sp");
        let body = format!(
            r"\noindent\hbox{{$\mkern{}mu a\scriptstyle\mkern{}mu b$}}\par",
            f64::from(kern) / 65536.0,
            f64::from(kern) / 65536.0
        );
        oracle.compare(&tfm, &settings, &body);
    }
    for width in [25, 45, 70] {
        oracle.compare(
            &tfm,
            &format!(r"{SETUP}\hsize={width}pt\binoppenalty=37\relpenalty=53\mathsurround=2pt"),
            r"\noindent abc$a\mathbin b c\mathrel d e\hskip3pt f$\ abc def$^{1}$ghi\par",
        );
    }
}
