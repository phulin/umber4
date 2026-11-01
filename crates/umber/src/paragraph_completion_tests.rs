use super::*;
use crate::{layout_values::DimenParam, token::cmd};

fn pending(source: &[u8]) -> Engine<'_> {
    let mut e = engine(source);
    loop {
        let read = e.next_expanded().unwrap();
        if read.meaning.command == cmd::PAR {
            return e;
        }
        e.execute(read).unwrap();
    }
}

#[test]
fn lower_completion_preserves_bindings_and_returns_the_line_before_adjustments_and_page_work() {
    let source=br"\catcode123=1\catcode125=2\font\f=test\f\vsize=5pt\hsize=50pt\parfillskip=0pt plus1fil\baselineskip=12pt\interlinepenalty=3\clubpenalty=5\widowpenalty=7\hangindent=5pt\parshape2 1pt40pt 2pt50pt\noindent ab\penalty-10000 cd\penalty-10000 ef\vadjust{\kern2pt}\par";
    let mut e = pending(source);
    let last = e.finish_paragraph(101).unwrap().unwrap();
    assert_eq!(e.document.text.mode, Mode::Vertical);
    assert_eq!(e.document.text.prev_graf, 3);
    assert_eq!(
        e.document.layout_values.dimens[DimenParam::HangIndent.index()].value,
        Scaled(5 * 65536)
    );
    assert!(e.document.layout_values.shape.lines.is_some());
    assert!(e.document.page.current.is_empty());
    assert!(e.outcome().pages.is_empty());
    let line_ids = lines(&e);
    assert_eq!(line_ids.last(), Some(&last));
    assert_ne!(e.document.text.vertical.tail(), last);
    assert!(matches!(
        e.document.nodes.get(e.document.text.vertical.tail()).kind,
        Kind::Kern { .. }
    ));
    assert_eq!(glyphs(&e), [b"ab".to_vec(), b"cd".to_vec(), b"ef".to_vec()]);
    let mut p = e.document.text.vertical.head();
    let mut penalties = Vec::new();
    while !p.is_null() {
        let n = e.document.nodes.get(p);
        if let Kind::Penalty(value) = n.kind {
            penalties.push(value);
        }
        p = n.next;
    }
    assert_eq!(penalties, [8, 104]);
    let Kind::Box(b) = &e.document.nodes.get(last).kind else {
        unreachable!()
    };
    assert_eq!(e.document.nodes.box_data(b).shift, Scaled(2 * 65536));
    // Consume the borrowed ID above, before this call can release line owners.
    e.build_page().unwrap();
    assert!(!e.outcome().pages.is_empty());
}

#[test]
fn empty_completion_has_no_line_and_ordinary_end_still_resets_and_builds() {
    let source = br"\hsize=50pt\hangindent=5pt\looseness=2\noindent\par";
    let mut e = pending(source);
    assert_eq!(e.finish_paragraph(101).unwrap(), None);
    assert_eq!(e.document.text.prev_graf, 0);
    assert!(e.document.text.vertical.is_empty());
    assert_eq!(
        e.document.layout_values.dimens[DimenParam::HangIndent.index()].value,
        Scaled(5 * 65536)
    );
    // A repeated completion in vertical mode is also inert.
    assert_eq!(e.finish_paragraph(0).unwrap(), None);
    let source=br"\font\f=test\f\vsize=5pt\hsize=50pt\hangindent=5pt\looseness=2\noindent ab\penalty-10000 cd\par";
    let mut e = pending(source);
    e.end_graf().unwrap();
    assert_eq!(
        e.document.layout_values.dimens[DimenParam::HangIndent.index()].value,
        Scaled::ZERO
    );
    assert_eq!(e.document.scalars[Target::paragraph(P::Looseness)].value, 0);
    assert!(!e.outcome().pages.is_empty());
}
