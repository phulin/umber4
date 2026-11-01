use super::*;
use crate::page_oracle_tests::Oracle;

#[test]
#[ignore = "native ordinary alignment execution: set UMBER_PDFTEX"]
fn upstream_ordinary_halign_execution_geometry_and_dvi() {
    let oracle = Oracle::new();
    for body in CASES.iter().chain(SCOPES) {
        oracle.compare(&basic(), &format!("{BOOT}{SETTINGS}"), body);
    }
}

#[test]
#[ignore = "native Gentle table/plain alignment macros: set UMBER_PDFTEX and UMBER_TFM_DIR"]
fn upstream_gentle_table_plain_tabs_and_math_alignment_templates() {
    let oracle = Oracle::new();
    let root = std::path::PathBuf::from(std::env::var_os("UMBER_TFM_DIR").expect("CM corpus"));
    let names = [
        "test", "sevenrm", "fiverm", "teni", "seveni", "fivei", "tensy", "sevensy", "fivesy",
        "tenex", "tenbf", "tentt",
    ];
    let files = [
        "cmr10", "cmr7", "cmr5", "cmmi10", "cmmi7", "cmmi5", "cmsy10", "cmsy7", "cmsy5", "cmex10",
        "cmbx10", "cmtt10",
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
    let mut settings =
        format!(r"{BOOT}{SETTINGS}\catcode36=3\catcode94=7\catcode95=8\catcode64=11\let\tenrm\f ");
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
    settings.push_str(r#"\hsize=450pt\vsize=700pt\delimiterfactor=901\delimitershortfall=5pt\nulldelimiterspace=1.2pt\scriptspace=.5pt
\thinmuskip=3mu\medmuskip=4mu plus2mu minus4mu\thickmuskip=5mu plus5mu
\dimendef\z@=10\z@=0pt\dimendef\p@=11\p@=1pt\chardef\@ne=1\chardef\tw@=2
\countdef\count@=255\dimendef\dimen@=0\dimendef\dimen@ii=2\def\m@ne{-1}
\chardef\tabs=10\chardef\tabsyet=11\chardef\tabsdone=12\chardef\voidb@x=9\chardef\strutbox=5
\skipdef\z@skip=0\z@skip=0pt\skipdef\smallskipamount=4\smallskipamount=3pt plus1pt minus1pt
\setbox\strutbox=\hbox{\vrule height8.5pt depth3.5pt width\z@}
\def\strut{\relax\ifmmode\copy\strutbox\else\unhcopy\strutbox\fi}
\let\bgroup={ \let\egroup=}\def\ialign{\everycr{}\tabskip\z@skip\halign}\def\null{\hbox{}}
\def\normalbaselines{\baselineskip=12pt\lineskip=1pt\lineskiplimit=0pt}
\def\m@th{\mathsurround\z@}\def\quad{\hskip1em\relax}\def\smallskip{\vskip\smallskipamount}
\def\bf{\fam6\tenbf}\def\tt{\fam7\tentt}\mathchardef\backslash="26E
\def\{{\delimiter"4266308 }\def\}{\delimiter"5267309 }
\chardef\%=37\chardef\&=38\chardef\$=36\chardef\#=35
\def\leavevmode{\unhbox\voidb@x}\def\_{\leavevmode \kern.06em \vbox{\hrule width.3em}}
\def\^#1{{\accent94 #1}}\def\~#1{{\accent"7E #1}}
\def\loop#1\repeat{\def\body{#1}\iterate}
\def\iterate{\body \let\next\iterate \else\let\next\relax\fi \next}\let\repeat=\fi
\def\us@true{\let\ifus@\iftrue}\def\us@false{\let\ifus@\iffalse}\us@false
\def\@crtrue{\let\if@cr\iftrue}\def\@crfalse{\let\if@cr\iffalse}\@crfalse
"#);
    let plain = std::fs::read_to_string(
        root.ancestors()
            .nth(3)
            .unwrap()
            .join("tex/plain/base/plain.tex"),
    )
    .unwrap();
    let tabs_start = plain.find(r"\def\cleartabs").unwrap();
    let tabs_end = plain[tabs_start..]
        .find(r"\outer\def\beginsection")
        .unwrap()
        + tabs_start;
    settings.push_str(&plain[tabs_start..tabs_end]);
    for body in [
        r"\settabs3\columns\+a&bb&ccc\cr\+d&ee&f\cr",
        r"\settabs\+aa&bbb&c\cr\+d&e&fff\cr",
    ] {
        oracle.compare_fonts(&fonts, &settings, body);
    }
    let gentle = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../.tools/corpus/gentle.tex"
    ))
    .unwrap();
    let defs = gentle.find(r"\def\\").unwrap();
    let defs_end = gentle[defs..].find(r"\def\beginliteral").unwrap() + defs;
    settings.push_str(&gentle[defs..defs_end]);
    let make = gentle.find(r"\def\maketable").unwrap();
    let make_end = gentle[make..].find(r"\def\makebox").unwrap() + make;
    settings.push_str(&gentle[make..make_end]);
    let start = gentle
        .find(r"\maketable [Characters requiring special input]")
        .unwrap();
    let end = gentle[start..].find(r"\toindex").unwrap() + start;
    oracle.compare_fonts(&fonts, &settings, &gentle[start..end]);
    // These are the ordinary alignment templates inside plain matrix/eqalign's
    // vcenter. Outer mu-glue wrappers are outside ordinary alignment execution.
    for body in [
        r"\noindent$\vcenter{\normalbaselines\m@th\ialign{\hfil$#$\hfil&&\quad\hfil$#$\hfil\crcr a&b&c\cr d&e\crcr}}$\par",
        r"\noindent$\vcenter{\normalbaselines\m@th\ialign{\strut\hfil$\displaystyle{#}$&$\displaystyle{{}#}$\hfil\crcr a&=b\cr c&=d\crcr}}$\par",
    ] {
        oracle.compare_fonts(&fonts, &settings, body);
    }
}
