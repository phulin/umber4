//! Scanned delimiters and unchanged plain size macros through native layout/DVI.
use super::*;
use crate::page_oracle_tests::Oracle;

#[test]
#[ignore = "native scanned delimiters: set UMBER_PDFTEX and UMBER_TFM_DIR"]
fn upstream_scanned_delimiters_and_plain_big_macros() {
    let oracle = Oracle::new();
    let tfm = metrics();
    let settings = format!(
        r"{SETUP}{DELCODES}\delimiterfactor=901\delimitershortfall=5pt\nulldelimiterspace=1.2pt "
    );
    for body in [
        r"$a\left(b\right)c$",
        r"$\left(\left(a\right)b\right)_c^{de}$",
        r"$\left(^a\scriptstyle b\right)c$",
        r"$a^{\left(b\right)}_{\left(c\right)}$",
        r#"$\fam=1\textfont1=\s\delimiter"7061000 a^\delimiter"4061000\mathop\delimiter"7062000$"#,
        r#"$\left\delimiter"7061000 a\right\delimiter"5062000$"#,
        r"$\left(\vbox to20pt{}\right.\nulldelimiterspace=-2pt$",
        r"\def\after{^b}$\left(\aftergroup\after a\right)c$",
        r"$\left(\textfont0=\s\fam=0 a\right)b$",
        r"$\left(\hbox{$\left(b\right)$}\right)$",
        r#"\catcode40=13\def({\undefined}\catcode40=12\mathcode40="8000$\left(a\right)$"#,
        r"\catcode126=13\def~{(}$\left~a\right)$",
    ] {
        oracle.compare(&tfm, &settings, &format!(r"\noindent\hbox{{{body}}}\par"));
    }
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
        .map(|name| std::fs::read(root.join(format!("cm/{name}.tfm"))).unwrap())
        .collect();
    let fonts: Vec<_> = names
        .iter()
        .zip(&bytes)
        .map(|(name, bytes)| (name.as_bytes(), bytes.as_slice()))
        .collect();
    let mut cm =
        String::from(r"\catcode36=3\catcode94=7\catcode95=8\catcode35=6\catcode64=11\let\tenrm\f ");
    for name in &names[1..] {
        cm.push_str(&format!(r"\font\{name}={name} "));
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
        for (command, font) in ["textfont", "scriptfont", "scriptscriptfont"]
            .iter()
            .zip(sizes)
        {
            cm.push_str(&format!(r"\{command}{family}=\{font} "));
        }
    }
    cm.push_str(
        r#"\delimiterfactor=901\delimitershortfall=5pt\nulldelimiterspace=1.2pt\scriptspace=.5pt
\thinmuskip=3mu\medmuskip=4mu plus2mu minus4mu\thickmuskip=5mu plus5mu
\dimen0=1pt\dimendef\p@=0\chardef\z@=0\def\m@th{\mathsurround\z@}
\def\{{\delimiter"4266308 }\def\}{\delimiter"5267309 }
\def\bigl{\mathopen\big}\def\bigr{\mathclose\big}
\def\Bigl{\mathopen\Big}\def\Bigr{\mathclose\Big}
\def\biggl{\mathopen\bigg}\def\biggr{\mathclose\bigg}
\def\Biggl{\mathopen\Bigg}\def\Biggr{\mathclose\Bigg}
\def\big#1{{\hbox{$\left#1\vbox to8.5\p@{}\right.\n@space$}}}
\def\Big#1{{\hbox{$\left#1\vbox to11.5\p@{}\right.\n@space$}}}
\def\bigg#1{{\hbox{$\left#1\vbox to14.5\p@{}\right.\n@space$}}}
\def\Bigg#1{{\hbox{$\left#1\vbox to17.5\p@{}\right.\n@space$}}}
\def\n@space{\nulldelimiterspace\z@ \m@th} "#,
    );
    for body in [
        r"$\bigl\{a\bigr\}$",
        r"$\Bigl\{a\Bigr\}$",
        r"$\biggl\{a\biggr\}$",
        r"$\Biggl\{a\Biggr\}$",
        r"$a+\Bigl\{b\Bigr\}_c$",
        r"\setbox0=\hbox{$\biggl\{$}\vrule height\ht0 depth\dp0 width0pt$\Bigl\{$Groups$\Bigr\}$",
    ] {
        oracle.compare_fonts(&fonts, &cm, &format!(r"\noindent\hbox{{{body}}}\par"));
    }
}
