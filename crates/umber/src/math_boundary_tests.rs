use super::{engine, lines};
use crate::{
    Engine,
    layout_values::DimenParam,
    nodes::{Kind, List},
    numeric::Scaled,
    packing::PackSpec,
    token::{Token, cmd},
};

// Construct only ordinary nodes; this adapter does not enter a math mode or
// perform conversion. Native comparisons use equivalent real math sources.
fn paragraph(source: &[u8]) -> Engine<'_> {
    let mut e = engine(source);
    let disc = Token::cs(e.intern(b"mathdisc").unwrap());
    let mut inside = false;
    loop {
        let read = e.next_expanded().unwrap();
        if read.meaning.command == cmd::PAR {
            assert!(!inside);
            e.break_paragraph().unwrap();
            return e;
        }
        if read.meaning.command == 3 {
            let width = e.document.layout_values.dimens[DimenParam::MathSurround.index()].value;
            e.append_horizontal(Kind::MathBoundary {
                width,
                after: inside,
            })
            .unwrap();
            inside = !inside;
        } else if read.token == disc {
            e.append_horizontal(Kind::discretionary(List::default(), List::default(), 0))
                .unwrap();
        } else {
            e.execute(read).unwrap();
        }
    }
}

fn contents<'e>(e: &'e Engine<'_>, id: crate::nodes::NodeId) -> Vec<&'e Kind> {
    let Kind::Box(b) = &e.document.nodes.get(id).kind else {
        unreachable!()
    };
    let mut p = e.document.nodes.box_data(b).list.head();
    let mut kinds = Vec::new();
    while !p.is_null() {
        let n = e.document.nodes.get(p);
        kinds.push(&n.kind);
        p = n.next;
    }
    kinds
}

const BOOT: &str = r"\catcode123=1\catcode125=2\catcode36=3\parfillskip=0pt plus1fil ";

#[test]
fn after_math_break_excludes_its_signed_width_and_discards_following_glue() {
    for surround in [-2, 0, 2] {
        let source = format!(
            r"{BOOT}\mathsurround={surround}pt\hsize={}pt\noindent\hbox to10pt{{}}$\hbox to10pt{{}}$\hskip3pt\hbox to10pt{{}}\par",
            20 + surround
        );
        let e = paragraph(source.as_bytes());
        let lines = lines(&e);
        assert_eq!(lines.len(), 2, "surround {surround}");
        let boundaries: Vec<_> = contents(&e, lines[0])
            .into_iter()
            .filter_map(|kind| {
                if let Kind::MathBoundary { width, after } = kind {
                    Some((width.0, *after))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(boundaries, [(surround * 65536, false), (0, true)]);
        assert!(
            !contents(&e, lines[1])
                .iter()
                .any(|kind| matches!(kind, Kind::MathBoundary { .. }))
        );
    }
}

#[test]
fn empty_math_after_a_forced_break_is_discarded_with_its_signed_width() {
    for surround in [-3, 3] {
        let source = format!(
            r"{BOOT}\mathsurround={surround}pt\hsize=10pt\noindent\hbox to10pt{{}}\penalty-10000 $$\hbox to10pt{{}}\par"
        );
        let e = paragraph(source.as_bytes());
        let lines = lines(&e);
        assert_eq!(lines.len(), 2);
        assert!(
            !contents(&e, lines[1])
                .iter()
                .any(|kind| matches!(kind, Kind::MathBoundary { .. }))
        );
        for id in lines {
            let Kind::Box(b) = &e.document.nodes.get(id).kind else {
                unreachable!()
            };
            assert_eq!(e.document.nodes.box_data(b).glue_set.ratio, 0.0);
        }
    }
}

#[test]
fn math_glue_and_explicit_kerns_cannot_split_a_formula() {
    let source = format!(
        r"{BOOT}\hsize=10pt\noindent$\hbox to5pt{{}}\hskip1pt\hbox to5pt{{}}\kern0pt\hskip1pt\hbox to5pt{{}}$\hskip1pt\hbox to5pt{{}}\par"
    );
    let e = paragraph(source.as_bytes());
    let mut pairs = 0;
    for id in lines(&e) {
        let kinds = contents(&e, id);
        let before = kinds
            .iter()
            .position(|kind| matches!(kind, Kind::MathBoundary { after: false, .. }));
        let after = kinds
            .iter()
            .position(|kind| matches!(kind, Kind::MathBoundary { after: true, .. }));
        if let Some(before) = before {
            let after = after.expect("formula stays in one line");
            assert_eq!(
                kinds[before..after]
                    .iter()
                    .filter(|kind| matches!(kind, Kind::Box(_)))
                    .count(),
                3
            );
            pairs += 1;
        }
    }
    assert_eq!(pairs, 1);
}

#[test]
fn explicit_penalties_and_discretionaries_still_break_inside_math() {
    for point in [r"\penalty-10000", r"\mathdisc"] {
        let source = format!(
            r"{BOOT}\let\mathdisc\relax\exhyphenpenalty=-10000\hsize=10pt\noindent$\hbox to5pt{{}}{point}\hbox to5pt{{}}$\hbox to5pt{{}}\par"
        );
        let e = paragraph(source.as_bytes());
        let lines = lines(&e);
        assert_eq!(lines.len(), 2, "{point}");
        assert!(
            contents(&e, lines[0])
                .iter()
                .any(|kind| matches!(kind, Kind::MathBoundary { after: false, .. }))
        );
        assert!(
            !contents(&e, lines[0])
                .iter()
                .any(|kind| matches!(kind, Kind::MathBoundary { after: true, .. }))
        );
        assert!(
            contents(&e, lines[1])
                .iter()
                .any(|kind| matches!(kind, Kind::MathBoundary { after: true, .. }))
        );
    }
}

#[test]
fn mathsurround_is_scoped_typed_state_and_replays() {
    let source = br"\catcode123=1\catcode125=2
\message{\the\mathsurround}\mathsurround=1pt
{\mathsurround=-2pt\advance\mathsurround by.5pt\message{\the\mathsurround}}
\message{\the\mathsurround}{\global\mathsurround=3pt}
{\globaldefs=-1\global\mathsurround=4pt}\message{\the\mathsurround}\end";
    let mut e = Engine::new(source);
    let before = e.checkpoint().unwrap();
    for _ in 0..2 {
        e.advance(usize::MAX).unwrap();
        assert_eq!(
            e.outcome().message_bytes(),
            [
                b"0.0pt".to_vec(),
                b"-1.5pt".to_vec(),
                b"1.0pt".to_vec(),
                b"3.0pt".to_vec()
            ]
        );
        e.restore(&before).unwrap();
    }
}

#[test]
fn packed_boundary_widths_move_output_and_are_not_lastkern() {
    let mut e = engine(
        br"\catcode123=1\catcode125=2\font\f=test\f\noindent\kern4pt\message{\the\lastkern}\message{\the\lastkern}\end",
    );
    // This test uses direct nodes rather than the paragraph adapter for output.
    while e.document.text.horizontal.is_empty() {
        let read = e.next_expanded().unwrap();
        e.execute(read).unwrap();
    }
    let read = e.next_expanded().unwrap();
    e.execute(read).unwrap();
    e.append_horizontal(Kind::MathBoundary {
        width: Scaled(2 * 65536),
        after: false,
    })
    .unwrap();
    let read = e.next_expanded().unwrap();
    e.execute(read).unwrap();
    assert_eq!(
        e.outcome().message_bytes(),
        [b"4.0pt".to_vec(), b"0.0pt".to_vec()]
    );
    let stale = std::mem::take(&mut e.document.text.horizontal);
    e.document
        .nodes
        .release(stale, &mut crate::nodes::ReleaseScratch::default());
    for kind in [
        Kind::Character {
            font: 1,
            character: b'a',
        },
        Kind::MathBoundary {
            width: Scaled(2 * 65536),
            after: false,
        },
        Kind::Character {
            font: 1,
            character: b'b',
        },
        Kind::MathBoundary {
            width: Scaled(-3 * 65536),
            after: true,
        },
        Kind::Character {
            font: 1,
            character: b'c',
        },
    ] {
        e.append_horizontal(kind).unwrap();
    }
    let widths: Vec<_> = b"abc"
        .iter()
        .map(|&c| e.character_metrics(1, c).unwrap().width.0)
        .collect();
    let list = std::mem::take(&mut e.document.text.horizontal);
    let b = e.hpack(list, PackSpec::Natural).unwrap();
    let Kind::Box(id) = &e.document.nodes.get(b.id()).kind else {
        unreachable!()
    };
    assert_eq!(
        e.document.nodes.box_data(id).width.0,
        widths.iter().sum::<i32>() - 65536
    );
    e.publish_page(b).unwrap();
    let xs: Vec<_> = e
        .outcome()
        .pages
        .iter()
        .next()
        .unwrap()
        .placements()
        .filter_map(|p| {
            if let crate::Placement::Glyph { x, .. } = p {
                Some(x)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        xs,
        [0, widths[0] + 2 * 65536, widths[0] + widths[1] - 65536]
    );
}
