use crate::{
    Engine, Error, Progress,
    layout_values::REGISTERS,
    numeric::{Glue, GlueOrder, Scaled},
    token::{Meaning, Token, cmd},
};

#[test]
fn same_source_numeric_fixture_has_independently_checked_values() {
    let result = Engine::new(include_bytes!("../../../fixtures/mvp/layout-values.tex"))
        .run()
        .unwrap();
    assert_eq!(
        result.messages,
        [
            "UMBER:80909/1.23457pt/0.0pt/0.5pt/8/16:END",
            "UMBER:4736286/786432/1864679/186467/65781/70124/841489:END",
            "UMBER:8.0pt minus 3.0fill:END",
            "UMBER:-12.0pt minus -4.5fill/1.0pt plus 1.0fil:END",
            "UMBER:2.5pt/2.5pt plus 2.0pt minus 1.0fil/\\dimen255/\\skip255:END",
            "UMBER:9.0pt/5.0pt:END",
            "UMBER:8.0pt/4.0pt plus 1.0fill:END",
            "UMBER:ALIASES:END",
            "UMBER:12.75pt/524288/262144:END",
            "UMBER:AFTER/11.0pt:END",
            "UMBER:42.0pt/1.0pt plus 2.0fil/1000/97/65:END",
            "UMBER:3946881/54613:END",
            "UMBER:0.0pt/0.0pt:END",
            "UMBER:16384.0pt/1073741824:END",
            "UMBER:FORMAT:END",
        ]
        .map(|s| s.as_bytes().to_vec())
    );
}
#[test]
fn dimensions_preserve_cursor_boundaries_in_files_and_stored_sources() {
    let cases = [
        (".pt!", 0),
        (" ,pt!", 0),
        ("-0.00000762939453125pt!", -1),
        (".5000000000000000000000000000000000000000pt!", 32768),
        ("1.9sp!", 1),
        ("--'10pt!", 524288),
        ("\"10pt!", 1048576),
        ("`A sp!", 65),
    ];
    for (input, want) in cases {
        let mut file = Engine::new(input.as_bytes());
        assert_eq!(file.scan_dimen(), Ok(Scaled(want)), "{input}");
        assert_eq!(file.next_raw().unwrap().token, Token::other(b'!'));
        let source = format!("\\catcode123=1 \\catcode125=2\\def\\input{{{input}}}\\input");
        let mut stored = Engine::new(source.as_bytes());
        stored.advance(3).unwrap();
        assert_eq!(stored.scan_dimen(), Ok(Scaled(want)), "stored {input}");
        assert_eq!(stored.next_raw().unwrap().token, Token::other(b'!'));
    }
}
#[test]
fn internal_units_and_optional_spaces_follow_quantity_level() {
    let mut engine = Engine::new(br"\! X");
    let id = engine.intern(b"!").unwrap();
    engine.assign_meaning(
        id as usize,
        Meaning {
            command: cmd::DIMEN_PARAM,
            value: 0,
        },
        false,
    );
    engine.document.layout_values.dimens[REGISTERS].value = Scaled(123);
    assert_eq!(engine.scan_dimen(), Ok(Scaled(123)));
    assert_eq!(engine.next_raw().unwrap().token, Token::SPACE);
    let mut engine = Engine::new(br"2\! X");
    let id = engine.intern(b"!").unwrap();
    engine.assign_meaning(
        id as usize,
        Meaning {
            command: cmd::DIMEN_PARAM,
            value: 0,
        },
        false,
    );
    engine.document.layout_values.dimens[REGISTERS].value = Scaled(-123);
    assert_eq!(engine.scan_dimen(), Ok(Scaled(-246)));
    assert_eq!(engine.next_raw().unwrap().token, Token::SPACE);
    let mut engine = Engine::new(br"2em X");
    assert_eq!(engine.scan_dimen(), Ok(Scaled(0)));
    assert_eq!(engine.next_raw().unwrap().token, Token::character(11, b'X'));
    let mut engine = Engine::new(br"2\count0!");
    engine.document.scalars[crate::scalars::Target::count(0)].value = -17;
    assert_eq!(engine.scan_dimen(), Ok(Scaled(-34)));
    assert_eq!(engine.next_raw().unwrap().token, Token::other(b'!'));
}
#[test]
fn glue_copy_signs_order_and_direct_large_dimension_conversion() {
    let mut engine = Engine::new(br"-\! plus1pt!");
    let id = engine.intern(b"!").unwrap();
    engine.assign_meaning(
        id as usize,
        Meaning {
            command: cmd::GLUE_PARAM,
            value: 4,
        },
        false,
    );
    engine.document.layout_values.glues[REGISTERS + 4].value = Glue {
        width: Scaled(10),
        stretch: Scaled(20),
        stretch_order: GlueOrder::Fil,
        ..Glue::default()
    };
    let glue = engine.scan_glue().unwrap();
    assert_eq!(glue.width, Scaled(-10));
    assert_eq!(glue.stretch, Scaled(-20));
    assert_eq!(engine.next_raw().unwrap().token, Token::SPACE);
    let mut engine = Engine::new(br"1pt plus0fill!");
    assert_eq!(engine.scan_glue().unwrap().stretch_order, GlueOrder::Fill);
    assert_eq!(engine.next_raw().unwrap().token, Token::other(b'!'));
    let mut engine = Engine::new(br"\hsize!");
    engine.document.layout_values.dimens[REGISTERS].value = Scaled(1073741824);
    assert_eq!(engine.scan_glue().unwrap().width, Scaled(1073741824));
    let mut engine = Engine::new(br"\hsize!");
    engine.document.layout_values.dimens[REGISTERS].value = Scaled(1073741824);
    assert_eq!(engine.scan_dimen(), Err(Error::Overflow));
}
#[test]
fn command_inventory_has_explicit_expandable_classification() {
    assert!(cmd::DIMEN > 64 && !cmd::is_expandable(cmd::DIMEN));
    assert!(cmd::is_assignment(cmd::DIMEN));
    for command in [
        cmd::MACRO,
        cmd::PROTECTED_MACRO,
        cmd::UNDEFINED,
        cmd::IFDIM,
        cmd::THE,
    ] {
        assert!(cmd::is_expandable(command));
    }
    for command in [cmd::RELAX, cmd::NUMEXPR, cmd::SKIP, cmd::DIMEN_PARAM] {
        assert!(!cmd::is_expandable(command));
    }
}
#[test]
fn errors_leave_typed_bindings_uncommitted_and_restore_can_replay() {
    for source in [
        include_bytes!("../../../fixtures/mvp/failures/dimension-overflow.tex").as_slice(),
        include_bytes!("../../../fixtures/mvp/failures/dimension-intermediate-overflow.tex")
            .as_slice(),
        include_bytes!("../../../fixtures/mvp/failures/glue-multiply-overflow.tex").as_slice(),
        include_bytes!("../../../fixtures/mvp/failures/dimension-divide-zero.tex").as_slice(),
        include_bytes!("../../../fixtures/mvp/failures/magnification-frozen.tex").as_slice(),
        include_bytes!("../../../fixtures/mvp/failures/glue-order.tex").as_slice(),
    ] {
        let mut engine = Engine::new(source);
        let root = engine.checkpoint().unwrap();
        loop {
            match engine.advance(1) {
                Ok(Progress::Boundary) => {}
                Ok(Progress::Complete) => panic!("accepted invalid input"),
                Err(_) => break,
            }
        }
        assert!(!engine.can_checkpoint());
        engine.restore(&root).unwrap();
        assert!(engine.can_checkpoint());
        assert_eq!(engine.document.layout_values.dimens[0].value, Scaled(0));
        assert_eq!(
            engine.document.layout_values.glues[0].value,
            Glue::default()
        );
    }
}
