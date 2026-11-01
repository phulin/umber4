//! Native paragraph lists exercise reconstruction independently of pure lookup.
use super::Oracle;
use crate::font_test_data::{basic, tfm, tfm_range};
use std::path::PathBuf;

fn settings() -> String {
    let mut s = String::from(
        r"\hsize=400pt\pretolerance=-1\tolerance=-1\lefthyphenmin=1\righthyphenmin=1\uchyph=1 ",
    );
    for c in b'a'..=b'z' {
        s.push_str(&format!("\\lccode{c}={c}\\lccode{}={c}", c - 32));
    }
    s.push(' ');
    s
}
#[test]
#[ignore = "native hyphenation rendering: set UMBER_PDFTEX and UMBER_TFM_DIR"]
fn upstream_automatic_hyphenation_nodes() {
    let oracle = Oracle::new();
    let common = settings();
    for (extra, body) in [
        (
            r"\hyphenation{ab-cd-ef af-fi-ne dif-fi-cult}",
            r"\noindent\ abcdef affine difficult",
        ),
        (r"\hyphenation{ab-cd-ef}", r"\noindent abcdef abcdef abcdef"),
        (
            r"\hyphenation{ab-cd-ef}",
            r"\noindent\ abcdef\lefthyphenmin=5\ abcdef\setlanguage0\ abcdef",
        ),
        (
            r"\hyphenation{ab-cd-ef}",
            r"\noindent\ abcdef\setlanguage1\ abcdef\language=0\ abcdef",
        ),
        (
            r"\hyphenation{ab-cd-ef}\uchyph=0",
            r"\noindent\ Abcdef abcdef",
        ),
        (r"\hyphenation{ab-cd-ef}", r"\noindent\ Abcdef ABCDEF"),
        (
            r"\hyphenation{ab-cd-ef}",
            r"\noindent\ .abcdef, abcdef\hbox{}\ abcdef\kern0pt\hbox{}",
        ),
        (
            r"\hyphenation{ab-cd-ef}",
            r"\noindent\ abcdef\penalty0 abcdef\ abcdef-abcdef",
        ),
        (
            r"\hyphenation{ab-cd-ef}\hyphenchar\f=-1",
            r"\noindent\ abcdef abcdef",
        ),
        (
            r"\hyphenation{ab-cd-ef}\hyphenchar\f=200",
            r"\noindent\ abcdef abcdef",
        ),
        (
            r"\hyphenation{ab-cd-ef}\hyphenchar\f=256",
            r"\noindent\ abcdef abcdef",
        ),
        (
            r"\hyphenation{ab-cd-ef}\pretolerance=10000",
            r"\noindent\ abcdef abcdef",
        ),
        (
            r"\hyphenation{ab-cd-ef}\hsize=25pt\parfillskip=0pt plus1fil\tolerance=10000",
            r"\noindent\ abcdef abcdef abcdef abcdef",
        ),
        (
            r"\hyphenation{ab-cd-ef}\hsize=35pt\parfillskip=0pt plus1fil\pretolerance=0\tolerance=0\emergencystretch=30pt",
            r"\noindent\ abcdef abcdef abcdef abcdef abcdef abcdef",
        ),
        (
            r"\hyphenation{ab-c ab-cd-ef}\font\g=test at12pt\hyphenchar\g=45",
            r"\noindent\ abc\g def\f\ abcdef",
        ),
        (
            r"\hyphenation{ab-cd-ef}",
            r"\noindent\ abcdef\lccode99=0\ abcdef",
        ),
    ] {
        oracle.compare(&basic(), &format!("{common}{extra}"), body);
    }
    for op in [0, 1, 2, 3, 4, 5, 6, 7, 8, 11, 12] {
        let bytes = tfm(&[[128, b'c', op, b'X']], &[(b'b', 1, 0)]);
        oracle.compare(
            &bytes,
            &format!(r"{common}\hyphenation{{a-b-c-d-e-f}}"),
            r"\noindent\ abcdef abcdef",
        );
        let bytes = tfm(&[[128, b'-', op, b'X']], &[(b'b', 1, 0)]);
        oracle.compare(
            &bytes,
            &format!(r"{common}\hyphenation{{ab-cdef}}"),
            r"\noindent\ abcdef abcdef",
        );
    }
    // Punctuation and both boundary programs can affect either discretionary branch.
    for op in [0, 1, 2, 3, 5, 6, 7, 11, 128] {
        let bytes = tfm(
            &[
                [255, b'|', 0, 1],
                [128, b'a', op, if op == 128 { 0 } else { b'X' }],
                [128, b'|', 0, b'Y'],
                [128, b'a', 0, b'Z'],
                [255, 0, 0, 1],
            ],
            &[(b'f', 1, 2), (b'.', 1, 3)],
        );
        oracle.compare(
            &bytes,
            &format!(r"{common}\hyphenation{{a-b-c-d-e-f b-c-d-e-f}}"),
            r"\noindent\ abcdef .abcdef\ abcdef\noboundary\relax",
        );
    }
    let bytes = tfm_range(
        0,
        127,
        &[[255, b'a', 0, 1], [128, b'a', 0, b'X']],
        &[(b'f', 1, 1)],
    );
    oracle.compare(
        &bytes,
        &format!(r"{common}\hyphenation{{a-b-c-d-e-f}}"),
        r"\noindent\ abcdefa abcdefa",
    );
    // Native candidate length is a prefix limit, including when the 64th
    // character lies inside a ligature which cannot be partly collected.
    for length in [62, 63, 64, 65, 80] {
        let word = "a".repeat(length);
        oracle.compare(
            &tfm(&[], &[]),
            &format!(r"{common}\patterns{{a1a}}"),
            &format!(r"\noindent\ {word}"),
        );
    }
    // A ligature straddling the limit must roll back its entire component group.
    for length in [60, 61, 62, 63] {
        let word = format!("{}ffi", "a".repeat(length));
        oracle.compare(
            &basic(),
            &format!(r"{common}\patterns{{a1a}}"),
            &format!(r"\noindent\ {word}"),
        );
    }
    // Three emitted nodes per consumed letter make a single cut prefix exceed
    // the signed seven-bit replacement count despite the bounded source word.
    let growing = tfm(
        &[
            [0, b'a', 3, b'b'],
            [128, b'b', 7, b'c'],
            [128, b'b', 7, b'd'],
            [128, b'b', 7, b'e'],
            [128, b'b', 0, b'x'],
            [128, b'a', 0, b'a'],
        ],
        &[
            (b'a', 1, 0),
            (b'c', 1, 2),
            (b'd', 1, 3),
            (b'e', 1, 4),
            (b'x', 1, 5),
        ],
    );
    for length in [42, 43, 44, 63] {
        let word = "a".repeat(length);
        let log = oracle.compare(
            &growing,
            &format!(r"{common}\patterns{{a1a}}\hsize=4000pt"),
            &format!(r"\noindent\ {word}"),
        );
        if length == 43 {
            assert!(log.contains("\\discretionary replacing 127"));
        }
        if length >= 44 {
            assert!(!log.contains("\\discretionary"));
        }
    }
    let root = PathBuf::from(std::env::var_os("UMBER_TFM_DIR").expect("real TFM corpus directory"));
    let cmr = std::fs::read(root.join("cm/cmr10.tfm")).unwrap();
    let patterns =
        std::fs::read_to_string(root.join("../../../tex/generic/hyphen/hyphen.tex")).unwrap();
    for width in [27, 55, 90, 180, 400] {
        oracle.compare(&cmr, &format!(r"{common}{patterns}\hsize={width}pt\parfillskip=0pt plus1fil\tolerance=200"), r"\noindent\ Difficult efficient affirmation office affinity shelfful hyphenation representation mathematical mathematics extraordinarily unprecedented.");
    }
}
