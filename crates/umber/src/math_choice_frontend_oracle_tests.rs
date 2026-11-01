//! Eager branch diagnostics and selected scanned-source geometry/DVI.
use super::*;
use crate::page_oracle_tests::Oracle;

// The unchanged plain.tex mathpalette/smash definitions; only their bootstrap
// constants are supplied here instead of loading the entire format.
const PLAIN: &str = r"\catcode64=11\dimendef\z@=0\z@=0pt\def\m@th{\mathsurround\z@}
\def\mathpalette#1#2{\mathchoice{#1\displaystyle{#2}}%
  {#1\textstyle{#2}}{#1\scriptstyle{#2}}{#1\scriptscriptstyle{#2}}}
\def\smash{\relax % \relax, in case this comes first in \halign
  \ifmmode\def\next{\mathpalette\mathsm@sh}\else\let\next\makesm@sh
  \fi\next}
\def\makesm@sh#1{\setbox\z@\hbox{#1}\finsm@sh}
\def\mathsm@sh#1#2{\setbox\z@\hbox{$\m@th#1{#2}$}\finsm@sh}
\def\finsm@sh{\ht\z@\z@ \dp\z@\z@ \box\z@}
";

#[test]
#[ignore = "native eager mathchoice: set UMBER_PDFTEX"]
fn upstream_scanned_choices_effects_selection_and_plain_mathpalette_smash() {
    let oracle = Oracle::new();
    let tfm = metrics();
    let settings = format!(r"{SETUP}{PLAIN}\vbadness=10000\vfuzz=16383pt ");
    oracle.compare(&tfm, &settings, EFFECTS);
    for style in [
        r"\displaystyle",
        r"\textstyle",
        r"\scriptstyle",
        r"\scriptscriptstyle",
    ] {
        for formula in [
            r"\mathchoice{a}{bb}{ccc}{dddd}^e",
            r"a+\mathchoice{\mathrel b}{\mathbin c}{\mathop d\limits}{e}f",
            r"\mathchoice{a\over b}{\vcenter{\hbox{bc}}}{\overline{cd}}{\mathaccent94 e}_f",
            r"\smash{a_b^c}\mathpalette\mathsm@sh{\smash{d^e}_f}",
        ] {
            oracle.compare(
                &tfm,
                &settings,
                &format!(r"\noindent\hbox{{${style} {formula}$}}\par"),
            );
        }
    }
    for body in [
        r"$a^{\mathchoice{b}{c}{d}{e}}_{\mathchoice{f}{g}{h}{i}}$",
        r"$\mathchoice{\mathchoice{a}{b}{c}{d}}{\mathchoice{e}{f}{g}{h}}{i}{j}$",
        r"$\mathchoice{a}{\scriptstyle bc}{d}{e}f$",
        r"\def\after{\relax}\def\last{^a}$\mathchoice{\aftergroup\after b}{c}{d}{\aftergroup\last e}f$",
        r"$\smash{a\over b}+\smash{\smash c}$",
        r"\everymath{\global\advance\count0by1}$\smash{a^b}$\message{UMBER:HOOKS:\the\count0:END}",
    ] {
        oracle.compare(&tfm, &settings, &format!(r"\noindent\hbox{{{body}}}\par"));
    }
}
