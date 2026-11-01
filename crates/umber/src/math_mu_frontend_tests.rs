use super::*;

const CASES: &[&str] = &[
    r"$a\mskip1.5mu plus2mu minus3mu b\scriptstyle\mskip1.5mu plus2mu minus3mu c\scriptscriptstyle\mskip1.5mu d$",
    r"$a_{b\mskip-2mu}^{c\mskip1mu plus2fil minus3fill}d$",
    r"\muskip0=-2mu plus3filll minus4mu$\mskip-\muskip0 a\hskip1pt plus2fil\mskip\thinmuskip b$",
    r"$a\mathbin b\mskip2mu\mathrel c d\mskip0mu\mathbin e f$",
    r"$a\nonscript\mskip2mu b\scriptstyle c\nonscript\mskip2mu d$",
    r"$\scriptstyle a\nonscript\kern2pt b\nonscript\mkern3mu c\nonscript\hskip4pt d$",
    r"$\scriptstyle a\nonscript\nonscript\mskip2mu b\nonscript\textstyle\mskip3mu c$",
    r"$\scriptstyle a\nonscript\mathchoice{\mskip1mu}{\mskip2mu}{\mskip3mu}{\mskip4mu}b$",
    r"$\scriptstyle a\mathchoice{b}{c}{\nonscript}{d}\mskip3mu e$",
    r"$\scriptstyle a\nonscript\leaders\hbox{b}\hskip2pt c\nonscript\penalty17\mskip1mu d$",
    r"$a\nonscript_{b}\mskip1mu c$",
    r"$\mskip1mu a\textfont2=\ss\mskip2mu b\thinmuskip=4mu$",
    r"$a{\mskip1mu\scriptfont2=\ss}_{b\mskip2mu}c$",
    r"$a\mskip1mu\nonscript$",
];

#[test]
fn pending_mu_glue_is_frame_owned_and_replays_with_scanned_sources() {
    let source = input(r"\setbox0=\hbox{\kern1pt}\noindent$\mskip2mu plus3fil minus4mu\relax");
    let mut e = engine(source.as_bytes());
    let checkpoint = e.checkpoint().unwrap();
    for _ in 0..3 {
        loop {
            let read = e.next_expanded().unwrap();
            if read.meaning.command == cmd::RELAX && !e.document.math_frames.is_empty() {
                break;
            }
            e.execute(read).unwrap();
        }
        assert_eq!(e.math.as_ref().unwrap().live_count(), 1);
        assert_eq!(e.document.nodes.live_count(), 2);
        assert!(!e.can_checkpoint());
        assert!(!e.document.boxes.head(0).is_null());
        e.restore(&checkpoint).unwrap();
        assert_eq!(e.math.as_ref().unwrap().live_count(), 0);
        assert_eq!(e.document.nodes.live_count(), 0);
    }
    for body in CASES {
        let source = input(&format!(r"\noindent\hbox{{{body}}}\par\end"));
        let mut e = engine(source.as_bytes());
        e.advance(usize::MAX).unwrap();
        assert_eq!(e.math.as_ref().unwrap().live_count(), 0);
    }
}

#[test]
fn mskip_rejects_ordinary_units_and_nonmath_without_consuming_later_material() {
    for (body, error) in [
        (r"\noindent\mskip\message{UNREAD}", Error::Syntax),
        (r"\nonscript\message{UNREAD}", Error::Syntax),
        (r"\noindent$\mskip\skip0\message{UNREAD}", Error::Number),
    ] {
        let source = input(body);
        let mut e = engine(source.as_bytes());
        let checkpoint = e.checkpoint().unwrap();
        assert_eq!(e.advance(usize::MAX), Err(error));
        assert_eq!(e.next_expanded().unwrap().meaning.command, cmd::MESSAGE);
        e.restore(&checkpoint).unwrap();
        assert_eq!(e.document.nodes.live_count(), 0);
    }
}

#[test]
#[ignore = "native math glue and nonscript: set UMBER_PDFTEX"]
fn upstream_mu_glue_nonscript_style_context_and_signed_scaling() {
    let oracle = crate::page_oracle_tests::Oracle::new();
    let tfm = metrics();
    for body in CASES {
        oracle.compare(&tfm, SETUP, &format!(r"\noindent\hbox{{{body}}}\par"));
    }
    for quad in [-589824, -1179630, 1179630] {
        let settings = format!(r"{SETUP}\fontdimen6\f={quad}sp");
        oracle.compare(&tfm, &settings, r"\noindent\hbox{$a\mskip.0000152587890625mu plus-.0000152587890625mu minus.0000457763671875mu b\mskip1mu plus2fil minus3fill$}\par");
    }
    oracle.compare(
        &tfm,
        &format!(r"{SETUP}\hsize=45pt\mathsurround=2pt"),
        r"\noindent ab$a\mathbin b\mskip3mu plus2mu c\mathrel d e$abc def\par",
    );
}
