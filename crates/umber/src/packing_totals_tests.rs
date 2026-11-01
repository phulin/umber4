use super::*;
use crate::token::cmd;

fn source(extra: &str) -> String {
    format!(
        r"\catcode123=1\catcode125=2\noindent\vrule width3pt height2pt depth1pt\lower1pt\hbox{{\vrule width4pt height4pt depth2pt}}{extra}\kern-2pt\hskip1pt plus2pt minus3pt\hskip0pt plus4fil minus5fill\cleaders\hbox{{\vrule width1pt height5pt depth1pt}}\hskip2pt plus6filll minus7fil\relax"
    )
}
fn contents(source: &[u8]) -> (Engine<'_>, List) {
    let mut e = Engine::new(source);
    loop {
        let read = e.next_expanded().unwrap();
        if read.meaning.command == cmd::RELAX {
            break;
        }
        e.execute(read).unwrap();
    }
    let list = std::mem::take(&mut e.document.text.horizontal);
    (e, list)
}

#[test]
fn horizontal_totals_retain_natural_geometry_and_all_orders_for_exact_and_spread_packing() {
    for (spec, width, sign, order, ratio) in [
        (
            PackSpec::Natural,
            8,
            GlueSign::Normal,
            GlueOrder::Normal,
            0.0,
        ),
        (
            PackSpec::To(Scaled(14 * 65536)),
            14,
            GlueSign::Stretching,
            GlueOrder::Filll,
            1.0,
        ),
        (
            PackSpec::Spread(Scaled(-14 * 65536)),
            -6,
            GlueSign::Shrinking,
            GlueOrder::Fill,
            2.8,
        ),
    ] {
        let source = source("");
        let (mut e, list) = contents(source.as_bytes());
        let head = list.head();
        let (node, totals) = e.hpack_with_totals(list, spec).unwrap();
        assert_eq!(totals.width, Scaled(8 * 65536));
        assert_eq!(
            (totals.height, totals.depth),
            (Scaled(5 * 65536), Scaled(3 * 65536))
        );
        assert_eq!(totals.stretch, [2 * 65536, 4 * 65536, 0, 6 * 65536]);
        assert_eq!(totals.shrink, [3 * 65536, 7 * 65536, 5 * 65536, 0]);
        let b = e.document.nodes.box_mut(node.id());
        assert_eq!(b.list.head(), head);
        assert_eq!(b.width, Scaled(width * 65536));
        assert_eq!(b.glue_set, GlueSet { sign, order, ratio });
        e.document.nodes.release_node(node, &mut e.page_release);
        assert_eq!(e.document.nodes.live_count(), 0);
    }
}

#[test]
fn adjusted_totals_exclude_migrating_material_and_keep_its_ownership_separate() {
    let source = source(r"\insert5{\hrule width30pt height20pt depth10pt}\vadjust{\kern3pt}");
    let (mut e, list) = contents(source.as_bytes());
    let mut adjustments = List::default();
    let (node, totals) = e
        .hpack_adjusted_with_totals(list, PackSpec::To(Scaled(14 * 65536)), &mut adjustments)
        .unwrap();
    assert_eq!(totals.width, Scaled(8 * 65536));
    assert_eq!(
        (totals.height, totals.depth),
        (Scaled(5 * 65536), Scaled(3 * 65536))
    );
    assert_eq!(totals.stretch, [2 * 65536, 4 * 65536, 0, 6 * 65536]);
    assert_eq!(totals.shrink, [3 * 65536, 7 * 65536, 5 * 65536, 0]);
    assert!(matches!(
        e.document.nodes.get(adjustments.head()).kind,
        Kind::Insertion(_)
    ));
    assert!(matches!(
        e.document.nodes.get(adjustments.tail()).kind,
        Kind::Kern {
            width: Scaled(196608),
            ..
        }
    ));
    e.document.nodes.release_node(node, &mut e.page_release);
    assert_eq!(e.document.nodes.live_count(), 3);
    e.document.nodes.release(adjustments, &mut e.page_release);
    assert_eq!(e.document.nodes.live_count(), 0);
}
