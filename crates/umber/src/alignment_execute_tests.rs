use super::*;
use crate::{Progress, font_test_data::basic};
const BOOT: &str = r"\catcode123=1\catcode125=2\catcode35=6\catcode38=4 ";
const SETTINGS: &str = r"\hsize=70pt\vsize=200pt\baselineskip=12pt\lineskip=1pt\topskip=10pt\parfillskip=0pt plus1fil\vbadness=10000\vfuzz=16383pt ";
fn engine(source: &[u8]) -> Engine<'_> {
    let mut e = Engine::new(source);
    e.register_tfm(b"test", &basic()).unwrap();
    e
}
const CASES: &[&str] = &[
    r"\halign{#&#\cr ab&cd\cr ef&g\cr}",
    r"\tabskip=1pt plus1fil\halign to60pt{#\hfil&\hfil#\cr ab&cd\cr e&fg\cr}",
    r"\halign spread-2pt{#\hfil&\hfil#\cr ab&cd\cr}",
    r"\halign{#&#&#\cr a\span b&cd\cr e&f&gh\cr ij\span\omit kl\span m\cr}",
    r"\halign{[#]&&(#)\cr a&b&c&d\cr e&f\cr}",
    r"\halign{&\hfil#\hfil\cr a&bb&ccc\cr d&ee\cr}",
    r"\halign{[#]&(#)\cr\omit a&b\cr c&\omit d\cr}",
    r"\halign{#&#\cr a\cr\crcr\cr b&c\crcr}",
    r"\halign{#\cr\crcr}",
    r"\halign{#\cr\noalign{\hrule height2pt\kern3pt}a\cr\noalign{\kern4pt\prevdepth=5pt}b\cr}",
    r"\halign{#\cr\noalign{\noindent ab cd\par}ef\cr}",
    r"\noindent ab\halign{#\cr cd\cr}ef\par",
    r"\setbox0=\vbox{\prevdepth=3pt\halign{#\cr a\cr}\message{UMBER:DEPTH:\the\prevdepth:END}}\noindent\box0\par",
    r"\halign{#&#\cr\vbox{\halign{#\cr ab\cr c\cr}}&d\cr}",
];
const SCOPES: &[&str] = &[
    r"\let\egroup=}\halign{#\cr a\aftergroup\egroup\cr",
    r"\def\deferred{\message{UMBER:T:END}}\halign{\deferred#\cr a\cr b\cr}",
    r"\def\param{##}\halign{\span\param\message{UMBER:V:END}\cr a\cr}",
    r"\tabskip=2pt\halign{\tabskip=3pt#&#\cr a&b\cr}\message{UMBER:TAB:\the\tabskip:END}",
    r"\globaldefs=1\tabskip=2pt\halign{\tabskip=3pt#\cr a\cr}\message{UMBER:TAB:\the\tabskip:END}",
    r"\globaldefs=-1\tabskip=2pt\count1=1\halign{\global\tabskip=3pt\count1=4 #\cr a\cr}\message{UMBER:LOCAL:\the\tabskip/\the\count1:END}",
    r"\tabskip=2pt\count1=1\halign{\global\tabskip=3pt\count1=4 #\cr a\cr}\message{UMBER:GLOBAL:\the\tabskip/\the\count1:END}",
    r"\def\report{\message{UMBER:AFTER:\the\count1:END}}\afterassignment\report\halign{\tabskip=3pt#\cr\count1=8 a\cr}",
    r"\count0=1\halign{\message{UMBER:U:\the\count0:END}#&#\cr\count0=2 a\span\message{UMBER:SPAN:\the\count0:END}b\cr\message{UMBER:NEXT:\the\count0:END}c&d\cr}",
    r"\def\peek{\ifvmode\message{UMBER:BODY:END}\else\ifnum\spacefactor=1000 \ifdim\lastskip=0pt \message{UMBER:NEWCELL:END}\else\ifdim\lastskip=4pt \message{UMBER:SPAN:END}\else\undefined\fi\fi\else\undefined\fi\fi}\tabskip=3pt\halign{\message{UMBER:U:END}#&#&#\cr\peek a&\peek b\hskip4pt\span\peek c\cr}",
    r"\def\late{\ifhmode\ifnum\spacefactor=1000 \message{UMBER:CELL-AFTER:END}\else\undefined\fi\else\message{UMBER:BODY-AFTER:END}\fi}\halign{#&#\cr\aftergroup\late a&b\cr c\aftergroup\late&d\cr}",
    r"\everycr{\noalign{\message{UMBER:OLD:END}\global\everycr{\noalign{\message{UMBER:NEW:END}}}}}\halign{#\cr a\cr b\cr}",
    r"\setbox0=\vbox{\prevdepth=3pt\everycr{\noalign{\message{UMBER:DEPTH:\the\prevdepth:END}\setbox9=\lastbox\message{UMBER:VOID:\ifvoid9 Y\else N\fi:END}}}\halign{#\cr a\cr\noalign{\prevdepth=4pt}b\cr}\message{UMBER:FINAL:\the\prevdepth:END}}\noindent\box0\par",
];
#[test]
fn ordinary_alignment_sources_complete_with_released_construction_roots() {
    for body in CASES.iter().chain(SCOPES) {
        let source = format!(r"{BOOT}\font\f=test\f {SETTINGS}{body}\end");
        let mut e = engine(source.as_bytes());
        assert_eq!(e.advance(usize::MAX), Ok(Progress::Complete), "{body}");
        assert!(e.document.alignment_frames.is_empty());
        assert!(e.document.alignment_layouts.is_empty());
        assert!(!e.alignment_reader.active());
        assert_eq!(e.document.nodes.live_count(), 0);
    }
}

#[path = "alignment_execute_oracle_tests.rs"]
mod oracle;
#[path = "alignment_execute_ownership_tests.rs"]
mod ownership;

#[test]
fn malformed_preambles_closers_and_modes_preserve_cursor_and_owners() {
    for (body, error, next) in [
        (
            r"\halign{a\cr\message{UNREAD}",
            Error::Syntax,
            cmd::ALIGN_CR,
        ),
        (r"\halign{##\message{UNREAD}", Error::Syntax, cmd::MESSAGE),
        (r"\halign{&&\message{UNREAD}", Error::Syntax, 4),
        (
            r"\def\once{\next}\def\next{##}\halign{\span\once\cr\message{UNREAD}",
            Error::Syntax,
            cmd::ALIGN_CR,
        ),
        (
            r"\halign{#\cr a&\message{UNREAD}",
            Error::Syntax,
            cmd::MESSAGE,
        ),
        (
            r"\halign{#\cr a}\message{UNREAD}",
            Error::Syntax,
            cmd::END_GROUP_CHAR,
        ),
        (
            r"\halign{#\cr\begingroup a\cr\message{UNREAD}",
            Error::Group,
            cmd::MESSAGE,
        ),
        (
            r"\catcode36=3\halign{#\cr$a\cr\message{UNREAD}",
            Error::Syntax,
            cmd::MESSAGE,
        ),
        (
            r"\setbox1=\hbox{\halign\message{UNREAD}",
            Error::Syntax,
            cmd::MESSAGE,
        ),
        (
            r"\catcode36=3\noindent$$\halign\message{UNREAD}",
            Error::Unsupported,
            cmd::MESSAGE,
        ),
        (r"\valign\message{UNREAD}", Error::Unsupported, cmd::MESSAGE),
    ] {
        let source = format!(r"{BOOT}\font\f=test\f {SETTINGS}\setbox0=\hbox{{z}}{body}");
        let mut e = engine(source.as_bytes());
        let before = e.checkpoint().unwrap();
        assert_eq!(e.advance(usize::MAX), Err(error), "{body}");
        assert_eq!(e.next_token().unwrap().meaning.command, next, "{body}");
        assert!(!e.document.boxes.head(0).is_null());
        assert!(!e.can_checkpoint());
        e.restore(&before).unwrap();
        assert!(e.document.alignment_frames.is_empty() && e.document.alignment_layouts.is_empty());
        assert!(!e.alignment_reader.active());
        assert_eq!(e.document.nodes.live_count(), 0);
    }
}
