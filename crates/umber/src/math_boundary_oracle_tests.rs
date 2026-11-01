//! Native math sources compared with directly constructed ordinary boundaries.
use super::Oracle;
use crate::{
    font_test_data::tfm,
    layout_values::DimenParam,
    nodes::{Kind, List},
    token::cmd,
};

fn metrics() -> Vec<u8> {
    let mut bytes = tfm(&[], &[]);
    // No italic corrections or ligature programs: ordinary source characters
    // independently match the glyph nodes produced by native ordinary noads.
    for c in 0..128 {
        bytes[32 + c * 4 + 2] = 0;
    }
    let words = u16::from_be_bytes([bytes[0], bytes[1]]) + 15;
    bytes[..2].copy_from_slice(&words.to_be_bytes());
    bytes[22..24].copy_from_slice(&22u16.to_be_bytes());
    bytes.extend_from_slice(&[0; 60]);
    bytes
}

#[test]
#[ignore = "native math boundary paragraphs: set UMBER_PDFTEX"]
fn upstream_math_boundary_paragraphs() {
    let oracle = Oracle::new();
    let tfm = metrics();
    let common = r"\catcode36=3\let\mathdisc\relax\textfont1=\f\textfont2=\f\textfont3=\f\scriptfont2=\f\scriptfont3=\f\scriptscriptfont2=\f\scriptscriptfont3=\f\parfillskip=0pt plus1fil\baselineskip=12pt\linepenalty=10\hyphenpenalty=50\exhyphenpenalty=50\adjdemerits=10000\doublehyphendemerits=10000\finalhyphendemerits=5000\lefthyphenmin=1\righthyphenmin=1\hyphenation{ab-cd-ef} ";
    for width in [10, 20, 27, 40, 70] {
        for surround in [-3, 0, 2] {
            for body in [
                r"\noindent abc$\hbox{def}$\hskip3pt ghi abc",
                r"\noindent abc$\hbox{def}\hskip2pt\hbox{ghi}$\hskip3pt abc def",
                r"\noindent abc$\hbox{def}\kern0pt\hskip2pt\hbox{ghi}$\hskip3pt abc def",
                r"\noindent abc\penalty-10000 $$def ghi",
                r"\noindent abc$\hbox{def}\penalty-10000\hbox{ghi}$abc def",
                r"\noindent abc$\hbox{def}\mathdisc\hbox{ghi}$abc def",
                r"\noindent\ abcdef$\hskip0pt abcdef\hskip0pt abcdef$\hskip0pt abcdef abcdef",
                r"\noindent\ abcdef$\hbox{abc}$abcdef\ abcdef",
                r"\noindent\ abcdef\kern0pt$\hbox{abc}$\kern0pt\hskip1pt abcdef",
            ] {
                for passes in [
                    r"\pretolerance=-1\tolerance=10000",
                    r"\pretolerance=0\tolerance=0\emergencystretch=30pt",
                ] {
                    let settings =
                        format!(r"{common}\hsize={width}pt\mathsurround={surround}pt{passes} ");
                    let native = body
                        .replace(r"\mathdisc", r"\discretionary{}{}{}")
                        .replace("$$", "$ $");
                    let mut inside = false;
                    oracle.compare_with(&tfm, &settings, body, &native, |e, read| {
                        if read.meaning.command == 3 {
                            let width = e.document.layout_values.dimens
                                [DimenParam::MathSurround.index()]
                            .value;
                            e.append_horizontal(Kind::MathBoundary {
                                width,
                                after: inside,
                            })
                            .unwrap();
                            inside = !inside;
                            true
                        } else if read.token.is_cs()
                            && e.names.spelling(read.token.id()) == b"mathdisc"
                        {
                            assert_eq!(read.meaning.command, cmd::RELAX);
                            e.append_horizontal(Kind::discretionary(
                                List::default(),
                                List::default(),
                                0,
                            ))
                            .unwrap();
                            true
                        } else {
                            false
                        }
                    });
                    assert!(!inside);
                }
            }
        }
    }
}
