use super::*;
use crate::{Progress, font_test_data::basic, nodes::GlueSign, numeric::GlueOrder};

const SOURCE: &[u8] = include_bytes!("../../../fixtures/mvp/horizontal-boxes.tex");
const EXPECTED: &str = include_str!("../../../fixtures/mvp/horizontal-boxes.expected");

#[test]
fn horizontal_boxes_match_native_ordering_scope_and_void_fixtures() {
    let mut engine = Engine::new(SOURCE);
    engine.run_input().unwrap();
    let expected: Vec<_> = EXPECTED.lines().map(str::as_bytes).collect();
    assert_eq!(engine.outcome().message_bytes(), expected);
    assert!(engine.document.box_frames.is_empty());
}

fn data<'e>(engine: &'e Engine<'_>, index: usize) -> &'e crate::nodes::BoxData {
    let Kind::Box(id) = &engine
        .document
        .nodes
        .get(engine.document.boxes.head(index))
        .kind
    else {
        panic!("box register")
    };
    engine.document.nodes.box_data(id)
}
const BOOT: &str = "\\catcode123=1\\catcode125=2\\catcode35=6\\endlinechar=-1\\relax\n";

#[test]
fn executable_packing_specs_aliases_and_infinite_horizontal_glue() {
    for (body, width, height, depth, sign, order, ratio) in [
        (
            r"\hbox{\vrule width3pt height4pt depth2pt\kern-1pt\hskip5pt plus3pt minus2pt}",
            7,
            4,
            2,
            GlueSign::Normal,
            GlueOrder::Normal,
            0.0,
        ),
        (
            r"\make to20pt\bgroup\kern7pt\hfil\egroup",
            20,
            0,
            0,
            GlueSign::Stretching,
            GlueOrder::Fil,
            13.0,
        ),
        (
            r"\hbox spread-7pt{}",
            -7,
            0,
            0,
            GlueSign::Normal,
            GlueOrder::Normal,
            0.0,
        ),
        (
            r"\hbox to10pt{\hfil\kern2pt\hfilneg\hss}",
            10,
            0,
            0,
            GlueSign::Stretching,
            GlueOrder::Fil,
            8.0,
        ),
        (
            r"\hbox to10pt{\kern3pt\hfil\hfill\hss\hfilneg}",
            10,
            0,
            0,
            GlueSign::Stretching,
            GlueOrder::Fill,
            7.0,
        ),
        (
            r"\hbox spread-3pt{\hss\kern5pt}",
            2,
            0,
            0,
            GlueSign::Shrinking,
            GlueOrder::Fil,
            3.0,
        ),
        (
            r"\hbox spread-10pt{\hskip5pt minus2pt}",
            -5,
            0,
            0,
            GlueSign::Shrinking,
            GlueOrder::Normal,
            1.0,
        ),
        (
            r"\hbox to\wd1{\hfil}",
            17,
            0,
            0,
            GlueSign::Stretching,
            GlueOrder::Fil,
            17.0,
        ),
    ] {
        let source = format!(
            "{BOOT}\\let\\make=\\hbox\\let\\bgroup={{\\let\\egroup=}}\\setbox1=\\hbox to17pt{{}}\\setbox0={body}\\end"
        );
        let mut engine = Engine::new(source.as_bytes());
        engine.run_input().unwrap();
        let data = data(&engine, 0);
        assert_eq!(
            (data.width.0, data.height.0, data.depth.0),
            (width * 65536, height * 65536, depth * 65536),
            "{body}"
        );
        assert_eq!(
            (data.glue_set.sign, data.glue_set.order, data.glue_set.ratio),
            (sign, order, ratio),
            "{body}"
        );
    }
}

#[test]
fn restricted_par_preserves_list_and_shaping_finishes_before_nested_font_changes() {
    let source = format!(
        "{BOOT}\\font\\a=test\\font\\b=test at12pt\\setbox0=\\hbox{{\\a a\\par\\hbox{{\\global\\b a}}a}}\\end"
    );
    let mut engine = Engine::new(source.as_bytes());
    engine.register_tfm(b"test", &basic()).unwrap();
    engine.run_input().unwrap();
    let root = data(&engine, 0);
    assert_eq!(
        root.width,
        Scaled((7.5 * 65536.0 + 2.0 * 9.0 * 65536.0) as i32)
    );
    let first = engine.document.nodes.get(root.list.head());
    assert!(matches!(
        first.kind,
        Kind::Character {
            font: 1,
            character: b'a'
        }
    ));
    let nested = engine.document.nodes.get(first.next);
    let Kind::Box(id) = &nested.kind else {
        panic!("nested box")
    };
    let nested = engine.document.nodes.box_data(id);
    assert_eq!(nested.width, Scaled(9 * 65536));
    let last = engine.document.nodes.get(root.list.tail());
    assert!(matches!(
        last.kind,
        Kind::Character {
            font: 2,
            character: b'a'
        }
    ));
    assert!(engine.document.text.vertical.is_empty());
}

#[test]
fn saved_box_owners_and_header_mutation_replay_with_matching_nodes() {
    let source = format!(
        "{BOOT}\\setbox0=\\hbox{{\\kern1pt}}\\setbox1=\\hbox{{\\hbox{{\\kern2pt}}\\kern3pt}}{{\\setbox0=\\hbox{{\\kern7pt}}\\count0=1\\relax{{\\global\\setbox0=\\copy1}}\\wd0=19pt\\setbox1=\\box1}}\\count0=2\\relax\\end"
    );
    let mut engine = Engine::new(source.as_bytes());
    while engine.outcome().counts[0] != 1 {
        engine.advance(1).unwrap();
    }
    let saved = engine.checkpoint().unwrap();
    let original = engine.document.boxes.head(0);
    assert_eq!(data(&engine, 0).width, Scaled(7 * 65536));
    for _ in 0..4 {
        engine.run_input().unwrap();
        assert_eq!(data(&engine, 0).width, Scaled(19 * 65536));
        assert!(engine.document.boxes.head(1).is_null());
        engine.restore(&saved).unwrap();
        assert_eq!(engine.document.boxes.head(0), original);
        assert_eq!(data(&engine, 0).width, Scaled(7 * 65536));
        assert_eq!(data(&engine, 1).width, Scaled(5 * 65536));
    }
    engine.run_input().unwrap();
    drop(engine);
    drop(saved);
}

#[test]
fn active_frames_exclude_checkpoint_and_failed_construction_bulk_restores() {
    let source = format!(
        "{BOOT}\\setbox0=\\hbox{{\\kern1pt}}\\count0=1\\relax\\setbox1=\\hbox{{\\setbox0=\\hbox{{\\kern9pt}}\\hbox{{\\unknown}}}}\\end"
    );
    let mut engine = Engine::new(source.as_bytes());
    while engine.outcome().counts[0] != 1 {
        engine.advance(1).unwrap();
    }
    let saved = engine.checkpoint().unwrap();
    let mut foreign = Engine::new(b"\\end");
    let foreign_root = foreign.checkpoint().unwrap();
    foreign.run_input().unwrap();
    for _ in 0..4 {
        assert_eq!(engine.advance(usize::MAX), Err(Error::Undefined));
        assert_eq!(engine.document.box_frames.len(), 2);
        assert!(!engine.can_checkpoint());
        assert!(matches!(engine.checkpoint(), Err(Error::Checkpoint)));
        assert_eq!(engine.restore(&foreign_root), Err(Error::Checkpoint));
        assert_eq!(engine.document.box_frames.len(), 2);
        engine.restore(&saved).unwrap();
        assert!(engine.document.box_frames.is_empty());
        assert!(engine.can_checkpoint());
        assert_eq!(data(&engine, 0).width, Scaled(65536));
        assert!(engine.document.boxes.head(1).is_null());
    }
}

#[test]
fn invalid_box_operands_preserve_destination_and_failure_consumption() {
    for (body, error) in [
        (r"\setbox256=\csname notcreated\endcsname", Error::Number),
        (r"\wd256=\csname notcreated\endcsname", Error::Number),
        (r"\setbox0=\kern1pt", Error::Syntax),
        (r"\setbox0=\hbox{\vskip1pt}", Error::Syntax),
        (r"\setbox0=\hbox{\hrule}", Error::Syntax),
        (r"\setbox0=\hbox{\endgroup}", Error::Group),
        (r"\setbox0=\hbox{\begingroup}", Error::Group),
        (r"\setbox0=\hbox{\end}", Error::Group),
        (r"\setbox0=\hbox to-1sp{", Error::Eof),
    ] {
        let source = format!("{BOOT}\\setbox0=\\hbox{{\\kern1pt}}\\count0=1\\relax{body}");
        let mut engine = Engine::new(source.as_bytes());
        while engine.outcome().counts[0] != 1 {
            engine.advance(1).unwrap();
        }
        let saved = engine.checkpoint().unwrap();
        assert_eq!(engine.advance(usize::MAX), Err(error), "{body}");
        assert_eq!(data(&engine, 0).width, Scaled(65536));
        assert_eq!(engine.names.lookup(b"notcreated"), None);
        engine.restore(&saved).unwrap();
        assert!(engine.can_checkpoint());
    }
}

#[test]
fn command_budget_defers_through_everyhbox_and_restricted_construction() {
    let source = format!(
        "{BOOT}\\everyhbox={{\\global\\count1=7}}\\setbox0=\\hbox{{\\global\\count0=9\\hbox{{}}}}\\count0=10\\relax\\end"
    );
    let mut engine = Engine::new(source.as_bytes());
    loop {
        assert_eq!(engine.advance(1).unwrap(), Progress::Boundary);
        assert!(engine.document.box_frames.is_empty());
        if engine.outcome().counts[0] == 9 {
            break;
        }
    }
    assert_eq!(engine.outcome().counts[1], 7);
    assert!(!engine.document.boxes.head(0).is_null());
    engine.run_input().unwrap();
}

#[test]
fn void_append_preserves_depth_and_empty_append_uses_parent_baseline_glue() {
    let source = format!(
        "{BOOT}\\hsize=100pt\\vsize=1000pt\\baselineskip=12pt\\lineskiplimit=0pt\\lineskip=1pt\\hbox{{\\vrule width1pt height5pt depth2pt}}\\message{{\\the\\prevdepth}}\\box255\\message{{\\the\\prevdepth}}\\hbox{{}}\\count0=1\\relax\\end"
    );
    let mut engine = Engine::new(source.as_bytes());
    while engine.outcome().counts[0] != 1 {
        engine.advance(1).unwrap();
    }
    assert_eq!(engine.outcome().message_bytes(), [b"2.0pt", b"2.0pt"]);
    assert_eq!(engine.document.text.prev_depth, Scaled::ZERO);
    let mut id = engine.document.page.current.head();
    let mut baseline = Vec::new();
    while !id.is_null() {
        let node = engine.document.nodes.get(id);
        if let Kind::Glue {
            spec,
            subtype: crate::nodes::GlueKind::BaselineSkip,
        } = node.kind
        {
            baseline.push(spec.width);
        }
        id = node.next;
    }
    assert_eq!(baseline, [Scaled(10 * 65536)]);
    engine.run_input().unwrap();
}
