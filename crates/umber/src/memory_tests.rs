use crate::Engine;

const BOOT: &str = "\\catcode123=1 \\catcode125=2 \\catcode35=6\\relax\n";

#[test]
fn active_definitions_and_forwarded_ranges_survive_replacement() {
    let payload = "abcdef ".repeat(1200);
    let source = format!(
        r"{BOOT}
\def\active{{\def\active{{NEW}}\message{{OLD}}}}
\active\message{{\active}}
\def\take#1{{\def\producer{{REPLACED}}\edef\result{{#1}}}}
\def\producer{{\take{{{payload}}}}}
\producer\message{{\ifx\producer\active BAD\else OK\fi}}
\def\wanted{{{payload}}}\message{{\ifx\result\wanted YES\else NO\fi}}
\toks0={{OLD}}\toks1=\toks0
{{\toks0={{LOCAL}}\global\toks2=\toks0\toks0={{OTHER}}}}
\message{{\the\toks0/\the\toks1/\the\toks2}}
\end"
    );
    let outcome = Engine::new(source.as_bytes()).run().unwrap();
    assert_eq!(
        outcome.messages,
        [b"OLD".as_slice(), b"NEW", b"OK", b"YES", b"OLD/OLD/LOCAL"]
    );
}

#[test]
fn replacement_reuses_values_with_a_fixed_live_working_set() {
    let mut counts = Vec::new();
    for iterations in [100, 1000, 10000] {
        let source = format!(
            r"{BOOT}
\def\step{{\edef\actual{{\the\count0:abcdefgh abcdefgh abcdefgh}}
\let\alias=\actual\toks0={{abcdefgh}}\toks1=\toks0
{{\def\actual{{discarded}}\toks0={{local}}\global\toks2=\toks1}}
\advance\count0by1\relax\ifnum\count0<{iterations} \expandafter\step\fi}}
\step\end"
        );
        let mut engine = Engine::new(source.as_bytes());
        engine.run_input().unwrap();
        assert_eq!(engine.document.scalars.counts()[0].value, iterations);
        counts.push(engine.values.retained());
    }
    assert_eq!((counts[0].0, counts[0].1), (counts[1].0, counts[1].1));
    assert_eq!((counts[1].0, counts[1].1), (counts[2].0, counts[2].1));
    assert!(counts[2].0 <= 4, "{counts:?}");
    assert!(counts[2].1 <= 7, "{counts:?}");
}

#[test]
fn upstream_reclamation_transitions() {
    let result = Engine::new(include_bytes!("../../../fixtures/mvp/reclamation.tex"))
        .run()
        .unwrap();
    assert_eq!(
        result.messages,
        [
            b"UMBER:OLD/NEW:END".as_slice(),
            b"UMBER:OLD/GLOBAL:END",
            b"UMBER:OLD:END",
            b"UMBER:NEW:END",
            b"UMBER:forwarded {nested} tokens/REPLACED:END",
            b"UMBER:OLD:END",
            b"UMBER:NEW/NEW/LOCAL:END",
        ]
    );
}
