use super::*;
use crate::{nodes::List, numeric::Scaled, output::Placement};

#[test]
fn offsets_translate_all_lowered_geometry_and_replay_scoped_values() {
    let source = br"\catcode123=1\catcode125=2\hsize=20pt\vsize=20pt\hoffset=-2pt\voffset=3pt{\hoffset=7pt\count0=1\relax\hrule width5pt height4pt\penalty-10000}\hrule width5pt height4pt\penalty-10000\end";
    let mut engine = Engine::new(source);
    while engine.outcome().counts[0] != 1 {
        engine.advance(1).unwrap();
    }
    let saved = engine.checkpoint().unwrap();
    for _ in 0..2 {
        engine.run_input().unwrap();
        let pages = engine.outcome().pages;
        assert_eq!(pages.len(), 2);
        for (index, x) in [(0, 7), (1, -2)] {
            let page = pages.get(index).unwrap();
            let root = page.boxes()[0];
            assert_eq!((root.x, root.y), (x * 65536, root.height + 3 * 65536));
            assert!(
                matches!(page.placements().next(), Some(Placement::Rule { x: actual_x, y, .. }) if actual_x == x * 65536 && y == 3 * 65536)
            );
        }
        engine.restore(&saved).unwrap();
    }
}

#[test]
fn huge_page_checks_include_signed_offsets_before_mag_or_publication() {
    for (width, height, depth, x, y, rejected) in [
        (Scaled::MAX.0, 0, 0, 1, 0, true),
        (Scaled::MAX.0 + 1, 0, 0, -1, 0, false),
        (0, Scaled::MAX.0, 1, 0, -1, false),
        (0, Scaled::MAX.0, 0, 0, 1, true),
        (0, Scaled::MAX.0 + 1, 0, 0, -1, true),
        (0, 0, Scaled::MAX.0 + 1, 0, -1, true),
        (0, -10, -20, -30, -40, false),
    ] {
        let mut engine = Engine::new(b"");
        engine.document.layout_values.dimens[DimenParam::HOffset.index()].value = Scaled(x);
        engine.document.layout_values.dimens[DimenParam::VOffset.index()].value = Scaled(y);
        let node = engine
            .document
            .nodes
            .alloc_box(BoxData {
                list: List::default(),
                width: Scaled(width),
                height: Scaled(height),
                depth: Scaled(depth),
                ..BoxData::default()
            })
            .unwrap();
        let result = engine.publish_page(node, 0);
        assert_eq!(
            result,
            if rejected {
                Err(Error::Overflow)
            } else {
                Ok(())
            }
        );
        assert_eq!(engine.outcome().pages.is_empty(), rejected);
        assert_eq!(
            engine.document.layout_values.prepared_mag.is_none(),
            rejected
        );
    }
}
