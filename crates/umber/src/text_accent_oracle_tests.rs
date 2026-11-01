//! Complete text-accent node lists and DVI positions against native make_accent.
use super::Oracle;
use crate::font_test_data;
use std::path::PathBuf;

fn tiny(accent: i32, base: i32, height: i32) -> Vec<u8> {
    let mut b = font_test_data::tfm(&[], &[]);
    for c in 0..128 {
        b[32 + c * 4..36 + c * 4].copy_from_slice(&[1, 0, 0, 0]);
    }
    b[32 + 98 * 4..36 + 98 * 4].copy_from_slice(&[2, 16, 0, 0]);
    let dimensions = (6 + 2 + 128) * 4;
    b[dimensions + 4..dimensions + 8].copy_from_slice(&(accent * 16).to_be_bytes());
    b[dimensions + 8..dimensions + 12].copy_from_slice(&(base * 16).to_be_bytes());
    b[dimensions + 20..dimensions + 24].copy_from_slice(&(height * 16).to_be_bytes());
    b
}
#[test]
#[ignore = "native text accent: set UMBER_PDFTEX and UMBER_TFM_DIR"]
fn upstream_text_accent_nodes() {
    let oracle = Oracle::new();
    let common = r"\hsize=1000pt\parfillskip=0pt plus1fil\sfcode98=3000 ";
    for (extra, body) in [
        (
            "",
            r"\noindent\accent97 b \accent97 f \accent97 fi \accent97-",
        ),
        (r"\fontdimen5\f=\fontcharht\f98 ", r"\noindent\accent97 b"),
        (r"\fontdimen5\f=10pt ", r"\noindent\accent97 b"),
        (r"\fontdimen5\f=-2pt ", r"\noindent\accent97 b"),
        (
            "",
            r"\noindent\accent97\fontdimen5\f=20pt\fontdimen1\f=.7pt b",
        ),
        ("", r"\noindent\accent97\font\g=test at20pt\g b"),
        ("", r"\noindent\accent97\nullfont b\f c"),
        ("", r"\noindent\spacefactor=2000\accent0\count0=3 a"),
        ("", r"\noindent\spacefactor=2000\accent97\char0 b"),
        ("", r"\noindent\accent97\relax \def\base{b}\base c"),
        ("", r"\noindent\accent97\global\let\base=b\base c"),
        (r"\afterassignment b", r"\noindent\accent97\count0=3 a"),
        ("", r"\noindent\accent97\hbox{b}c"),
        ("", r"\noindent\accent97{b}c"),
        ("", r"\noindent\accent97\language=7 bc"),
        ("", r"\noindent\accent97\char98 c"),
        (r"\chardef\base=98 ", r"\noindent\accent97\base c"),
        ("", r"\noindent\hbox{\accent97 b \accent97 fi \accent97-}"),
        (r"\everypar{\fontdimen5\f=10pt}", r"\accent97 b"),
    ] {
        oracle.compare(&font_test_data::basic(), &format!("{common}{extra}"), body);
    }
    for (a, w, h) in [(2, 3, 0), (3, 2, 0), (1, 1, 1), (0, 0, 1)] {
        oracle.compare(
            &tiny(a, w, h),
            &format!(r"{common}\font\f=test at1pt\f\fontdimen1\f=0pt\fontdimen5\f=0pt"),
            r"\noindent\accent97 b",
        );
    }
    for slant in ["16383pt", "-16383pt"] {
        oracle.compare(
            &tiny(3, 2, 0),
            &format!(r"{common}\font\f=test at1pt\f\fontdimen1\f={slant}\fontdimen5\f=16383pt"),
            r"\noindent\accent97 b",
        );
    }
    let root = PathBuf::from(std::env::var_os("UMBER_TFM_DIR").expect("real TFM corpus directory"));
    for name in ["cm/cmr10.tfm", "cm/cmti10.tfm", "cm/cmbx10.tfm"] {
        oracle.compare(&std::fs::read(root.join(name)).unwrap(), common, r"\noindent Br\accent127 uggemann \accent19 a \accent94 o \accent18 e \accent126 n \accent23 c \accent22 d \accent19\font\g=test at14pt\g A");
    }
}
#[test]
#[ignore = "native text accent pages: set UMBER_PDFTEX and UMBER_TFM_DIR"]
fn upstream_text_accent_page_placements() {
    let oracle = crate::page_oracle_tests::Oracle::new();
    let settings = r"\hsize=100pt\vsize=200pt ";
    let body = r"\noindent\accent97 b \accent97 f \accent97 fi \hbox{\accent97 i}\par";
    oracle.compare(&font_test_data::basic(), settings, body);
    oracle.compare(
        &font_test_data::basic(),
        &format!(r"{settings}\fontdimen5\f=\fontcharht\f98"),
        body,
    );
    let root = PathBuf::from(std::env::var_os("UMBER_TFM_DIR").expect("real TFM corpus directory"));
    for font in ["cm/cmr10.tfm", "cm/cmti10.tfm"] {
        oracle.compare(&std::fs::read(root.join(font)).unwrap(), settings, r"\noindent Br\accent127 uggemann \accent19 a \accent94 o \accent18 e \accent126 n \accent23 c \accent22 d \accent19\font\g=test at14pt\g A\par");
    }
}
