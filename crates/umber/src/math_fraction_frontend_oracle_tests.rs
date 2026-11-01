//! Six generalized-fraction commands through actual scanning, layout, and DVI.
use super::*;
use crate::page_oracle_tests::Oracle;

#[test]
#[ignore = "native generalized fractions: set UMBER_PDFTEX"]
fn upstream_scanned_generalized_fractions_geometry_and_dvi() {
    let oracle = Oracle::new();
    let tfm = metrics();
    let settings = format!(
        r"{SETUP}{DELCODES}\delimiterfactor=901\delimitershortfall=5pt\nulldelimiterspace=1.2pt "
    );
    for command in [
        r"\over",
        r"\atop",
        r"\above-.5pt",
        r"\overwithdelims()",
        r"\atopwithdelims()",
        r"\abovewithdelims().125pt",
    ] {
        for body in [
            format!(r"$a{command} bc$"),
            format!(r"$a^{{b{command} c}}\left(\hbox{{a}}{command}\hbox{{b}}\right)_d$"),
        ] {
            oracle.compare(&tfm, &settings, &format!(r"\noindent\hbox{{{body}}}\par"));
        }
    }
    for body in [
        r"${{a\over b}\atop{c\above1sp d}}$",
        r"$\displaystyle{a\over b}\scriptstyle{c\atop d}\scriptscriptstyle{e\above0pt f}$",
        r"$\left(\over b\right)\left(a\atop\right)\left(\above0pt\right)$",
        r"$\over b$\ $a\atop$\ $\above0pt$",
        r"$\left(\left(a\over b\right)\over c\right.)\nulldelimiterspace=-2pt$",
        r"$a\over\scriptfont1=\ss b$",
        r"${a\over\scriptfont1=\ss b}\left(a\atop\scriptfont1=\ss b\right)$",
        r"$a\over\fontdimen8\f=.75pt b$",
        r"\def\after{^c}$\left(\aftergroup\after a\over b\right)d$",
        r"${\hbox{edit text}\atop\mathrel{ab}}\lower2pt\vbox{\hbox{input}\hbox{file}}$",
    ] {
        oracle.compare(&tfm, &settings, &format!(r"\noindent\hbox{{{body}}}\par"));
    }
    oracle.compare(
        &tfm,
        &format!(r"{settings}\hsize=40pt\mathsurround=2pt"),
        r"\noindent ab${a\over b}$cd ef${c\atop d}$gh\par",
    );
}
