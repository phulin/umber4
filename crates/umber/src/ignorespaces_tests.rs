use crate::{Engine, Error, Progress, font_test_data::basic, token::cmd};

const BOOT: &str = r"\catcode123=1\catcode125=2\catcode35=6 ";

#[test]
fn expanded_spaces_and_first_nonspace_effects_execute_in_one_dispatch() {
    for body in [
        r"\def\spaces{ }\def\empty{}\def\assign{\count0=7}\ignorespaces\empty\spaces\spaces\assign\message{\the\count0}",
        r"\let\ignore\ignorespaces\def\spaces{ }\ignorespaces\spaces\ignore\spaces\count0=7\message{\the\count0}",
        r"\let\report\message\ignorespaces\iftrue\report\else\undefined\fi{7}",
    ] {
        let source = format!(r"{BOOT}{body}\end");
        assert_eq!(
            Engine::new(source.as_bytes())
                .run()
                .unwrap()
                .message_bytes(),
            [b"7".to_vec()]
        );
    }
    let source = format!(r"{BOOT}\def\spaces{{ }}\relax\ignorespaces\spaces\count0=7\relax\end");
    let mut e = Engine::new(source.as_bytes());
    e.advance(4).unwrap(); // Catcodes and the macro definition.
    e.advance(1).unwrap(); // Explicit relax.
    assert_eq!(e.advance(1), Ok(Progress::Boundary));
    assert_eq!(e.outcome().counts[0], 7);
}

#[test]
fn first_nonspace_noexpand_meaning_is_not_expanded_again() {
    let source = format!(
        r"{BOOT}\def\spaces{{ }}\def\bad{{\message{{BAD}}}}\ignorespaces\spaces\noexpand\bad\message{{GOOD}}\end"
    );
    assert_eq!(
        Engine::new(source.as_bytes())
            .run()
            .unwrap()
            .message_bytes(),
        [b"GOOD".to_vec()]
    );
    let source = format!(
        r"{BOOT}\def\expected{{\ignorespaces}}\edef\saved{{\ignorespaces}}\ifx\expected\saved\message{{SAVED}}\fi\end"
    );
    assert_eq!(
        Engine::new(source.as_bytes())
            .run()
            .unwrap()
            .message_bytes(),
        [b"SAVED".to_vec()]
    );
}

#[test]
fn resolved_closers_and_end_use_the_normal_outer_paths() {
    for body in [
        r"{\let\close=}\ignorespaces\close\message{OUT}\ignorespaces\end",
        r"\begingroup\let\close\endgroup\ignorespaces\close\message{OUT}\ignorespaces\end",
        r"\let\stop\end\message{OUT}\ignorespaces\stop",
    ] {
        let source = format!(r"{BOOT}{body}");
        let mut e = Engine::new(source.as_bytes());
        assert_eq!(e.advance(usize::MAX), Ok(Progress::Complete));
        assert_eq!(e.outcome().message_bytes(), [b"OUT".to_vec()]);
        assert!(e.can_checkpoint());
    }
    let source = format!(
        r"{BOOT}\output{{\shipout\box255\ignorespaces}}\vsize=10pt\hrule width2pt height1pt\end"
    );
    assert_eq!(Engine::new(source.as_bytes()).run().unwrap().pages.len(), 1);
}

#[test]
fn exhausted_input_and_expansion_errors_keep_reader_failure_semantics() {
    for (body, error) in [
        (r"\ignorespaces", Error::Eof),
        (r"\ignorespaces\undefined", Error::Undefined),
        (r"\global\ignorespaces\undefined", Error::Unsupported),
        (r"\begingroup\ignorespaces}", Error::Group),
    ] {
        let source = format!(r"{BOOT}{body}");
        let mut e = Engine::new(source.as_bytes());
        let before = e.checkpoint().unwrap();
        assert_eq!(e.advance(usize::MAX), Err(error));
        assert!(!e.can_checkpoint());
        e.restore(&before).unwrap();
        assert!(e.can_checkpoint());
    }
    assert!(!cmd::is_expandable(cmd::IGNORE_SPACES));
}

#[test]
#[ignore = "native ignorespaces redispatch: set UMBER_PDFTEX"]
fn upstream_ignorespaces_execution_and_layout() {
    let oracle = crate::page_oracle_tests::Oracle::new();
    let tfm = basic();
    for body in [
        r"\def\spaces{ }\def\empty{}\def\assign{\count0=7}\ignorespaces\empty\spaces\spaces\assign\message{UMBER:\the\count0:END}",
        r"\def\spaces{ }\def\bad{\message{UMBER:BAD:END}}\ignorespaces\spaces\noexpand\bad\message{UMBER:GOOD:END}",
        r"\def\spaces{ }\let\?\ignorespaces\noindent a\?   b\spaces c\par",
        r"\def\spaces{ }\def\bad{c}\noindent a\ignorespaces\noexpand\bad\spaces b\par",
        r"\def\spaces{ }\noindent a\ignorespaces\relax\spaces b\par",
        r"{\let\close=}\ignorespaces\close\message{UMBER:OUT:END}",
        r"\noindent\hbox{a\ignorespaces}b\ignorespaces\par",
        r"\noindent a\ignorespaces\end",
    ] {
        oracle.compare(
            &tfm,
            &format!(r"{BOOT}\def\spaces{{ }}"),
            &format!(r"\noindent\hbox{{ab}}\par{body}"),
        );
    }
    let settings = format!(
        r"{BOOT}\catcode36=3\fontdimen22\f=0pt\textfont0=\f\textfont1=\f\textfont2=\f\textfont3=\f\scriptfont2=\f\scriptfont3=\f\scriptscriptfont2=\f\scriptscriptfont3=\f "
    );
    for body in [
        r"\def\spaces{ }\noindent a$\ignorespaces\spaces b\ignorespaces$\ignorespaces\spaces c\par",
        r"\def\spaces{ }\noindent$\ignorespaces\spaces{a\ignorespaces}\ignorespaces$\par",
    ] {
        oracle.compare(&tfm, &settings, body);
    }
}
