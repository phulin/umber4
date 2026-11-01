use super::*;

const SOURCE: &[u8] = include_bytes!("../../../fixtures/mvp/vertical-boxes.tex");
const EXPECTED: &str = include_str!("../../../fixtures/mvp/vertical-boxes.expected");

#[test]
fn native_vertical_geometry_unboxing_auxiliary_state_and_replayed_hooks() {
    let mut engine = Engine::new(SOURCE);
    engine.run_input().unwrap();
    assert_eq!(
        engine.outcome().message_bytes(),
        EXPECTED.lines().map(str::as_bytes).collect::<Vec<_>>()
    );
    assert!(engine.document.box_frames.is_empty());
}

const BOOT: &str = "\\catcode123=1\\catcode125=2\\endlinechar=-1\\relax\n";
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

#[test]
fn internal_paragraphs_return_to_their_parent_and_close_before_unsave() {
    let source = format!(
        "{BOOT}\\font\\f=test\\f\\hsize=100pt\\parskip=3pt\\output={{\\notexecuted}}\\everypar={{\\message{{\\ifhmode H\\else V\\fi/\\ifinner I\\else O\\fi}}}}\\setbox0=\\vbox{{\\noindent A\\par\\noindent B\\par}}\\setbox1=\\vbox{{\\hsize=25pt\\noindent A}}\\end"
    );
    let mut engine = Engine::new(source.as_bytes());
    engine
        .register_tfm(b"test", &crate::font_test_data::basic())
        .unwrap();
    engine.run_input().unwrap();
    assert_eq!(engine.outcome().message_bytes(), [b"H/O", b"H/O", b"H/O"]);
    assert!(engine.outcome().pages.is_empty());
    assert_eq!(data(&engine, 1).width, Scaled(25 * 65536));
    assert_eq!(
        engine.document.layout_values.dimens[DimenParam::HSize.index()].value,
        Scaled(100 * 65536)
    );
    let mut id = data(&engine, 0).list.head();
    assert!(matches!(engine.document.nodes.get(id).kind, Kind::Box(_)));
    let mut paragraph_skips = 0;
    while !id.is_null() {
        let node = engine.document.nodes.get(id);
        if matches!(
            node.kind,
            Kind::Glue {
                subtype: crate::nodes::GlueKind::ParSkip,
                ..
            }
        ) {
            paragraph_skips += 1;
        }
        id = node.next;
    }
    assert_eq!(paragraph_skips, 1);
}

#[test]
fn unboxing_lastbox_and_slot_reuse_preserve_retained_sources() {
    let source = format!(
        "{BOOT}\\setbox0=\\vbox{{\\hbox{{\\vrule width5pt height4pt depth2pt}}}}\\setbox1=\\hbox{{\\kern7pt}}\\count0=1\\relax\\setbox2=\\vbox{{\\unvcopy0\\global\\setbox3=\\lastbox}}\\wd3=99pt\\setbox4=\\hbox{{\\unhcopy1\\unhbox1}}\\setbox5=\\vtop{{\\kern1pt\\copy2}}\\end"
    );
    let mut engine = Engine::new(source.as_bytes());
    while engine.outcome().counts[0] != 1 {
        engine.advance(1).unwrap();
    }
    let saved = engine.checkpoint().unwrap();
    let original = engine.document.boxes.head(0);
    for _ in 0..3 {
        engine.run_input().unwrap();
        assert_eq!(engine.document.boxes.head(0), original);
        assert!(engine.document.boxes.head(1).is_null());
        assert_eq!(data(&engine, 0).width, Scaled(5 * 65536));
        assert_eq!(data(&engine, 3).width, Scaled(99 * 65536));
        assert_eq!(data(&engine, 4).width, Scaled(14 * 65536));
        assert_eq!(
            (data(&engine, 5).height, data(&engine, 5).depth),
            (Scaled::ZERO, Scaled(65536))
        );
        let child = data(&engine, 0).list.head();
        let Kind::Box(id) = &engine.document.nodes.get(child).kind else {
            panic!("source child")
        };
        assert_eq!(engine.document.nodes.box_data(id).width, Scaled(5 * 65536));
        engine.restore(&saved).unwrap();
        assert!(engine.document.boxes.head(3).is_null());
        assert!(engine.document.box_frames.is_empty());
    }
}

#[test]
fn wrong_type_and_mode_fail_before_taking_owners_and_restore_open_frames() {
    for (body, error) in [
        (r"\setbox2=\hbox{\unhbox0}", Error::Syntax),
        (r"\setbox2=\vbox{\unvbox1}", Error::Syntax),
        (r"\raise\csname notcreated\endcsname\hbox{}", Error::Syntax),
        (
            r"\setbox2=\hbox{\moveright\csname notcreated\endcsname\hbox{}}",
            Error::Syntax,
        ),
        (
            r"\setbox2=\hbox{\unvbox\csname notcreated\endcsname}",
            Error::Syntax,
        ),
        (r"\setbox2=\lastbox", Error::Syntax),
        (r"\setbox2=\vbox{\noindent A\unknown}", Error::Undefined),
        (r"\setbox2=\vtop{\begingroup}", Error::Group),
    ] {
        let source = format!(
            "{BOOT}\\setbox0=\\vbox{{\\hbox{{\\kern1pt}}}}\\setbox1=\\hbox{{\\kern2pt}}\\count0=1\\relax{body}"
        );
        let mut engine = Engine::new(source.as_bytes());
        while engine.outcome().counts[0] != 1 {
            engine.advance(1).unwrap();
        }
        let saved = engine.checkpoint().unwrap();
        let heads = (engine.document.boxes.head(0), engine.document.boxes.head(1));
        assert_eq!(engine.advance(usize::MAX), Err(error), "{body}");
        assert_eq!(
            (engine.document.boxes.head(0), engine.document.boxes.head(1)),
            heads
        );
        assert_eq!(engine.names.lookup(b"notcreated"), None);
        assert!(!engine.can_checkpoint());
        engine.restore(&saved).unwrap();
        assert!(engine.can_checkpoint());
        assert!(engine.document.box_frames.is_empty());
    }
}

#[test]
fn lastbox_skips_discretionary_replacements_and_detaches_only_a_genuine_tail() {
    let mut engine = Engine::new(b"");
    engine.document.text.mode = Mode::RestrictedHorizontal;
    for kind in [
        Kind::discretionary(List::default(), List::default(), 2),
        Kind::Kern {
            width: Scaled(1),
            explicit: true,
        },
    ] {
        let node = engine.document.nodes.alloc(kind).unwrap();
        engine
            .document
            .nodes
            .append(&mut engine.document.text.horizontal, node);
    }
    let protected = engine
        .document
        .nodes
        .alloc_box(crate::nodes::BoxData::default())
        .unwrap();
    let id = protected.id();
    engine
        .document
        .nodes
        .append(&mut engine.document.text.horizontal, protected);
    assert!(engine.take_last_box().unwrap().is_none());
    assert_eq!(engine.document.text.horizontal.tail(), id);
    let last = engine
        .document
        .nodes
        .alloc_box(crate::nodes::BoxData::default())
        .unwrap();
    let last_id = last.id();
    engine
        .document
        .nodes
        .append(&mut engine.document.text.horizontal, last);
    let taken = engine.take_last_box().unwrap().unwrap();
    assert_eq!(taken.id(), last_id);
    assert_eq!(engine.document.text.horizontal.tail(), id);
    assert!(engine.take_last_box().unwrap().is_none());
    engine
        .document
        .nodes
        .release_node(taken, &mut engine.page_release);
    let list = std::mem::take(&mut engine.document.text.horizontal);
    engine
        .document
        .nodes
        .release(list, &mut engine.page_release);
}

#[test]
fn failure_after_consuming_unbox_restores_the_abandoned_child_forest() {
    let source = format!(
        "{BOOT}\\setbox0=\\vbox{{\\hbox{{\\kern1pt}}}}\\count0=1\\relax\\setbox1=\\vbox{{\\unvbox0\\relax\\unknown}}"
    );
    let mut engine = Engine::new(source.as_bytes());
    while engine.outcome().counts[0] != 1 {
        engine.advance(1).unwrap();
    }
    let saved = engine.checkpoint().unwrap();
    let original = engine.document.boxes.head(0);
    for _ in 0..3 {
        assert_eq!(engine.advance(usize::MAX), Err(Error::Undefined));
        assert!(engine.document.boxes.head(0).is_null());
        assert!(!engine.document.text.vertical.is_empty());
        assert!(!engine.can_checkpoint());
        engine.restore(&saved).unwrap();
        assert_eq!(engine.document.boxes.head(0), original);
        assert_eq!(data(&engine, 0).width, Scaled(65536));
        assert!(engine.document.box_frames.is_empty());
    }
}
