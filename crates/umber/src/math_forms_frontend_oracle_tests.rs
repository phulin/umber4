//! Scanned forms, field folding, and vcenter construction against native TeX.
use super::*;
use crate::page_oracle_tests::Oracle;

#[test]
#[ignore = "native scanned forms: set UMBER_PDFTEX"]
fn upstream_scanned_forms_geometry_and_dvi() {
    let oracle = Oracle::new();
    let tfm = metrics();
    let settings = format!(r"{SETUP}\vbadness=10000\vfuzz=16383pt ");
    for form in [
        r"\overline",
        r"\underline",
        r#"\radical"061000"#,
        r#"\mathaccent"705E"#,
    ] {
        for body in [format!(r"${form} a_b^c$"), format!(r"${form} {{ab}}_c^d$")] {
            oracle.compare(&tfm, &settings, &format!(r"\noindent\hbox{{{body}}}\par"));
        }
    }
    for body in [
        r"$\mathaccent94 a_b^c{\mathaccent94 a}_b^c$",
        r"${{\mathaccent94 a_b}}^c\mathord{\mathaccent94 b^c}_d$",
        r"$a^{\mathaccent94 b_c}\mathop{\mathaccent94 c}_d$",
        r#"$\fam=1\textfont1=\s\mathaccent"705E a\mathaccent"005E a$"#,
        r"$\fam=0\mathaccent32767 a$",
        r#"$\scriptstyle\overline a^b\scriptscriptstyle\radical"061000 {bc}_d$"#,
        r"$\vcenter{\hbox{ab}}_c^d\vcenter to10pt{\kern1pt}\vcenter spread2pt{\kern1pt}$",
        r"\boxmaxdepth=-1pt$\vcenter to10pt{\hbox{\vrule width1pt height1pt depth8pt}}$",
        r"\hangindent=3pt\everyvbox{\ifvmode\ifinner\kern1pt\fi\fi\kern\hangindent}\def\after{^b}$\vcenter{\aftergroup\after\kern1pt}a$",
        r"\hsize=70pt$\vcenter{\hsize=20pt\noindent ab cd ef gh ij kl}$",
        r#"$\overline{\hbox{$\mathaccent94 a$}}\vcenter{\hbox{$\radical"061000 {b\over c}$}}_d$"#,
        r"$\underline{ab}\fontdimen8\f=.75pt\overline{cd}$",
    ] {
        oracle.compare(&tfm, &settings, &format!(r"\noindent\hbox{{{body}}}\par"));
    }
}
