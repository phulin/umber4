//! Display/number/adjustment forests across arena and conversion failures.
use super::*;
use crate::{Engine, Progress, execute::GroupKind, token::cmd};

const BOOT: &str = r"\catcode123=1\catcode125=2\catcode36=3\font\f=test\fontdimen22\f=0pt\f
\textfont0=\f\textfont1=\f\textfont2=\f\textfont3=\f
\scriptfont2=\f\scriptfont3=\f\scriptscriptfont2=\f\scriptscriptfont3=\f
\hsize=50pt\vsize=100pt\parfillskip=0pt plus1fil\relax";

fn engine(source: &[u8]) -> Engine<'_> {
    let mut e = Engine::new(source);
    e.register_tfm(b"test", &crate::font_test_data::basic())
        .unwrap();
    e
}

#[test]
fn display_number_capacity_errors_retain_completed_outer_formula_and_boxes() {
    for (limit, body, live, nodes) in [
        (1, r"\hbox{\kern2pt}\eqno b", 1, 4),
        (2, r"\hbox{\kern2pt}\eqno\hbox{\kern3pt}c", 2, 6),
    ] {
        let source =
            format!(r"{BOOT}\setbox0=\hbox{{\kern1pt}}\noindent$${body}\message{{UNREAD}}");
        let mut e = engine(source.as_bytes());
        e.math.as_mut().unwrap().limit = limit;
        let checkpoint = e.checkpoint().unwrap();
        for _ in 0..2 {
            assert_eq!(e.advance(usize::MAX), Err(Error::Capacity));
            assert_eq!(e.math.as_ref().unwrap().live, live);
            assert_eq!(e.document.nodes.live_count(), nodes);
            assert_eq!(e.document.math_frames.len(), 2);
            assert_eq!(e.current_group(), Some(GroupKind::MathShift));
            assert_eq!(e.next_expanded().unwrap().meaning.command, cmd::MESSAGE);
            e.restore(&checkpoint).unwrap();
            assert_eq!(e.document.nodes.live_count(), 0);
            assert_eq!(e.math.as_ref().unwrap().live, 0);
        }
    }
}

#[test]
fn display_conversion_handoffs_release_number_and_migration_without_outer_roots() {
    let source = format!(
        r#"{BOOT}\setbox0=\hbox{{\kern1pt}}\noindent$a\hbox{{\vbox{{\noindent$$\hbox{{\kern2pt}}\vadjust{{\kern3pt}}\insert5{{\kern4pt}}\mathaccent94 b\eqno\hbox{{\kern5pt}}$$\par}}}}$\par\end"#
    );
    for steps in [0, 1, 5, 10, 20, 40, 80] {
        let mut e = engine(source.as_bytes());
        let checkpoint = e.checkpoint().unwrap();
        let closing = loop {
            let read = e.next_expanded().unwrap();
            if read.meaning.command == 3
                && matches!(
                    e.document.math_frames.last().map(|f| &f.context),
                    Some(crate::math_frontend::Context::EquationNumber { .. })
                )
            {
                break read;
            }
            e.execute(read).unwrap();
        };
        e.math_scratch.as_mut().unwrap().interrupt_after(steps);
        match e.execute(closing) {
            Err(Error::Capacity) => {
                let formula_pending = e.document.math_frames.len() == 2;
                assert_eq!(
                    e.math.as_ref().unwrap().live,
                    if formula_pending { 5 } else { 1 },
                    "{steps}"
                );
                assert_eq!(
                    e.document.nodes.live_count(),
                    if formula_pending { 8 } else { 2 },
                    "{steps}"
                );
                assert!(e.math_scratch.as_ref().unwrap().is_empty());
            }
            Ok(()) => assert_eq!(e.math.as_ref().unwrap().live, 1),
            other => panic!("{steps}: {other:?}"),
        }
        e.math_scratch.as_mut().unwrap().clear_interrupt();
        e.restore(&checkpoint).unwrap();
        assert_eq!(e.document.nodes.live_count(), 0);
        assert_eq!(e.math.as_ref().unwrap().live, 0);
    }
}

#[test]
fn restored_display_and_zero_width_numbers_reuse_the_math_arena() {
    let source = format!(
        r"{BOOT}\noindent$$\hbox{{\kern2pt}}\vadjust{{\kern3pt}}\eqno\hbox to0pt{{\kern4pt}}$$\par\end"
    );
    let mut e = engine(source.as_bytes());
    let checkpoint = e.checkpoint().unwrap();
    let mut sizes = None;
    for _ in 0..3 {
        assert_eq!(e.advance(usize::MAX), Ok(Progress::Complete));
        assert_eq!(e.math.as_ref().unwrap().live, 0);
        assert_eq!(e.document.nodes.live_count(), 0);
        let current = (
            e.math.as_ref().unwrap().cells.len(),
            e.document.nodes.box_slot_count(),
        );
        if let Some(sizes) = sizes {
            assert_eq!(current, sizes);
        } else {
            sizes = Some(current);
        }
        e.restore(&checkpoint).unwrap();
    }
}
