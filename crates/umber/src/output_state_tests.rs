use crate::{Engine, Error, Progress, nodes::Kind, token::OUTPUT_ROUTINE};
const BOOT: &str = "\\catcode123=1 \\catcode125=2 \\relax\n";

const BODY: &str = r"\output{ABC}\message{STATE:\the\output:END}
\toks0=\output\output=\toks0\message{STATE:\the\output:END}
\toks0={XYZ}\output=\toks0\message{STATE:\the\output:END}
\let\out=\output{\out{LOCAL}\message{STATE:\the\out:END}}
\message{STATE:\the\output:END}\output{}\message{STATE:\the\output:END}
\toks0={}\output=\toks0\message{STATE:\the\output:END}";
const EXPECTED: &[&str] = &["{ABC}", "{ABC}", "XYZ", "{LOCAL}", "XYZ", "", ""];

#[test]
fn output_parameter_wraps_scanned_bodies_but_copies_verbatim() {
    let source = format!("{BOOT}{BODY}\\end");
    let out = Engine::new(source.as_bytes()).run().unwrap();
    assert_eq!(
        out.message_bytes(),
        EXPECTED
            .iter()
            .map(|s| format!("STATE:{s}:END").into_bytes())
            .collect::<Vec<_>>()
    );
}

#[test]
fn output_state_replays_and_rejects_custom_output_before_consumption() {
    let source = format!(
        r"{BOOT}\output{{CUSTOM}}\outputpenalty=7\count0=1\relax\hbox{{}}\penalty-10000\end"
    );
    let mut engine = Engine::new(source.as_bytes());
    while engine.outcome().counts[0] == 0 {
        engine.advance(1).unwrap();
    }
    let saved = engine.checkpoint().unwrap();
    for _ in 0..2 {
        assert_eq!(engine.advance(usize::MAX), Err(Error::Unsupported));
        assert!(engine.outcome().pages.is_empty());
        assert!(!engine.document.page.current.is_empty());
        let best = engine.document.page.state.best_break;
        assert!(matches!(
            engine.document.nodes.get(best).kind,
            Kind::Penalty(-10000)
        ));
        assert_eq!(
            engine.document.scalars
                [crate::scalars::Target::page(crate::scalars::PageParam::OutputPenalty)]
            .value,
            7
        );
        assert!(
            engine.document.token_registers[OUTPUT_ROUTINE]
                .owner
                .is_some()
        );
        engine.restore(&saved).unwrap();
    }
    drop(engine);
    drop(saved);
    let source = format!(r"{BOOT}{{\output{{CUSTOM}}}}\hbox{{}}\end");
    let mut engine = Engine::new(source.as_bytes());
    while engine.advance(32).unwrap() != Progress::Complete {}
    assert!(!engine.outcome().pages.is_empty());
}

#[test]
#[ignore = "native output-state oracle: set UMBER_PDFTEX explicitly"]
fn upstream_output_parameter_storage() {
    let dir = std::env::temp_dir().join(format!("umber-output-state-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("texmf.cnf"),
        b"TEXINPUTS=.\nshell_escape=f\nmax_print_line=10000\n",
    )
    .unwrap();
    std::fs::write(dir.join("probe.tex"), format!("{BOOT}{BODY}\\end\n")).unwrap();
    let out = std::process::Command::new(
        std::env::var_os("UMBER_PDFTEX").expect("explicit native oracle"),
    )
    .current_dir(&dir)
    .env("TEXMFCNF", &dir)
    .args([
        "-ini",
        "-etex",
        "-no-shell-escape",
        "-interaction=nonstopmode",
        "-halt-on-error",
        "probe.tex",
    ])
    .output()
    .unwrap();
    let log = std::fs::read_to_string(dir.join("probe.log")).unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    assert!(out.status.success(), "{log}");
    assert_eq!(
        log.split("STATE:")
            .skip(1)
            .map(|s| s.split_once(":END").unwrap().0)
            .collect::<Vec<_>>(),
        EXPECTED
    );
}
