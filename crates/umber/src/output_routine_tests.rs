//! Font-free INITEX output/shipout fixtures and checkpoint ownership.
use crate::{Engine, Error, Placement, Progress, scalars::Target};
const BOOT: &str = r"\catcode123=1 \catcode125=2 \hsize=20pt\vsize=10pt\maxdepth=0pt\topskip=0pt ";
type Fixture = (
    &'static str,
    &'static str,
    Option<Error>,
    usize,
    &'static [&'static str],
);
const FIXTURES: &[Fixture] = &[
    (
        "end_par",
        include_str!("../../../fixtures/output/end_par.tex"),
        Some(Error::Group),
        0,
        &["PAR"],
    ),
    (
        "quantities",
        include_str!("../../../fixtures/output/quantities.tex"),
        None,
        1,
        &["PAGE:10.0pt,4.0pt,0.0pt", "ASSIGNED:7.0pt,9.0pt"],
    ),
    (
        "direct",
        include_str!("../../../fixtures/output/direct.tex"),
        None,
        2,
        &["VOID:7", "H:0", "V:0"],
    ),
    (
        "routine",
        include_str!("../../../fixtures/output/routine.tex"),
        None,
        1,
        &["ENTER:1,-10000,1", "EXIT:0,2", "AFTER:2"],
    ),
    (
        "multiple",
        include_str!("../../../fixtures/output/multiple.tex"),
        None,
        2,
        &["ENDROUTINE:3,0", "AFTER:3"],
    ),
    (
        "recycle",
        include_str!("../../../fixtures/output/recycle.tex"),
        None,
        1,
        &["ENTRY:1,1", "ENTRY:2,2", "AFTER:2,0"],
    ),
    (
        "limit",
        include_str!("../../../fixtures/output/limit.tex"),
        Some(Error::Unsupported),
        0,
        &[],
    ),
    (
        "unconsumed",
        include_str!("../../../fixtures/output/unconsumed.tex"),
        Some(Error::Unsupported),
        0,
        &[],
    ),
    (
        "copied_unwrapped",
        include_str!("../../../fixtures/output/copied_unwrapped.tex"),
        Some(Error::Syntax),
        0,
        &[],
    ),
    (
        "copied_wrapped",
        include_str!("../../../fixtures/output/copied_wrapped.tex"),
        None,
        1,
        &["COPIED"],
    ),
    (
        "end_inside",
        include_str!("../../../fixtures/output/end_inside.tex"),
        Some(Error::Group),
        0,
        &[],
    ),
    (
        "early_close",
        include_str!("../../../fixtures/output/early_close.tex"),
        Some(Error::Group),
        1,
        &[],
    ),
    (
        "macro_close",
        include_str!("../../../fixtures/output/macro_close.tex"),
        None,
        1,
        &[],
    ),
    (
        "replace_active",
        include_str!("../../../fixtures/output/replace_active.tex"),
        None,
        2,
        &["OLDTAIL", "NEW"],
    ),
];
fn markers(engine: &Engine<'_>) -> Vec<String> {
    engine
        .outcome()
        .message_bytes()
        .iter()
        .map(|b| {
            let text = std::str::from_utf8(b).unwrap();
            text.strip_prefix("CHECK:")
                .unwrap()
                .strip_suffix(":END")
                .unwrap()
                .to_owned()
        })
        .collect()
}
#[test]
fn output_routine_font_free_semantics() {
    for &(name, source, error, pages, expected) in FIXTURES {
        let mut e = Engine::new(source.as_bytes());
        assert_eq!(
            e.advance(usize::MAX),
            error.map_or(Ok(Progress::Complete), Err),
            "{name}"
        );
        assert_eq!(e.outcome().pages.len(), pages, "{name}");
        assert_eq!(markers(&e), expected, "{name}");
    }
}
#[test]
fn output_routine_metadata_and_no_intermediate_boundary() {
    let source = format!(
        r"{BOOT}\count0=1\output{{\outputpenalty=17\count0=2\hoffset=3pt\shipout\copy255\count0=3\outputpenalty=19\shipout\box255\global\count1=1}}\hbox{{\vrule width2pt height3pt depth1pt}}\penalty-10000\relax\end"
    );
    let mut e = Engine::new(source.as_bytes());
    while e.outcome().counts[1] == 0 {
        e.advance(1).unwrap();
        assert!(
            e.outcome().pages.is_empty()
                || (e.outcome().pages.len() == 2 && e.outcome().counts[1] == 1)
        );
        assert!(e.can_checkpoint());
    }
    let out = e.outcome();
    for (p, count, penalty) in [
        (out.pages.get(0).unwrap(), 2, 17),
        (out.pages.get(1).unwrap(), 3, 19),
    ] {
        assert_eq!(p.counts[0], count);
        assert_eq!(p.output_penalty, penalty);
        assert_eq!(p.boxes()[0].x, 3 * 65536);
    }
    assert_eq!(out.counts[0], 1);
    assert_eq!(
        e.document.scalars[Target::page(crate::scalars::PageParam::OutputPenalty)].value,
        -10000
    );
}
#[test]
fn output_routine_scoped_box255_levels_and_returned_material() {
    let source = format!(
        r"{BOOT}\count0=1{{\setbox255=\hbox{{}}\setbox0=\box255\output{{\shipout\box255\global\advance\count1by1\relax\ifnum\count1<2\hbox{{\vrule width4pt height2pt}}\penalty-10000\fi}}\hbox{{\vrule width2pt height3pt}}\penalty-10000}}\ifvoid255\count2=1\fi\end"
    );
    let out = Engine::new(source.as_bytes()).run().unwrap();
    assert_eq!(out.counts[1], 2);
    assert_eq!(out.counts[2], 1);
    assert_eq!(out.pages.len(), 2);
    let widths: Vec<_> = out
        .pages
        .iter()
        .flat_map(|p| {
            p.placements().filter_map(|p| match p {
                Placement::Rule { width, .. } => Some(width),
                _ => None,
            })
        })
        .collect();
    assert_eq!(widths, [2 * 65536, 4 * 65536]);
}
#[test]
fn output_routine_replay_error_restore_and_drop_orders() {
    for engine_first in [false, true] {
        let source = format!(
            r"{BOOT}\output{{\shipout\copy255\global\output{{\shipout\box255}}}}\count0=1\relax\hbox{{\vrule width2pt height3pt}}\penalty-10000\end"
        );
        let mut e = Engine::new(source.as_bytes());
        while e.outcome().counts[0] == 0 {
            e.advance(1).unwrap();
        }
        let saved = e.checkpoint().unwrap();
        for _ in 0..2 {
            assert_eq!(e.advance(usize::MAX), Err(Error::Unsupported));
            let retained = e.outcome();
            assert_eq!(retained.pages.len(), 1);
            assert!(!e.can_checkpoint());
            e.restore(&saved).unwrap();
            assert!(e.outcome().pages.is_empty());
            assert_eq!(e.document.page.dead_cycles, 0);
            assert_eq!(retained.pages.get(0).unwrap().placements().count(), 1);
        }
        if engine_first {
            drop(e);
            drop(saved);
        } else {
            drop(saved);
            drop(e);
        }
    }
}
#[test]
fn output_routine_deadcycles_global_scoping_and_huge_rejection() {
    let source = format!(
        r"{BOOT}\message{{CHECK:MAX:\the\maxdeadcycles:END}}{{\maxdeadcycles=3\deadcycles=7}}\message{{CHECK:STATE:\the\maxdeadcycles,\the\deadcycles:END}}\shipout\box0\shipout\hbox{{}}\end"
    );
    let mut e = Engine::new(source.as_bytes());
    e.advance(usize::MAX).unwrap();
    assert_eq!(markers(&e), ["MAX:25", "STATE:25,7"]);
    let source = format!(r"{BOOT}\deadcycles=9\hoffset=16383pt\shipout\hbox to2pt{{}}\end");
    let mut e = Engine::new(source.as_bytes());
    assert_eq!(e.advance(usize::MAX), Err(Error::Overflow));
    assert_eq!(e.document.page.dead_cycles, 0);
    assert!(e.outcome().pages.is_empty());
}
#[test]
#[ignore = "native output-routine oracle: set UMBER_PDFTEX explicitly"]
fn upstream_output_routine_state_and_dvi_geometry() {
    let dir = std::env::temp_dir().join(format!("umber-output-routine-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("texmf.cnf"),
        b"TEXINPUTS=.\nshell_escape=f\nmax_print_line=10000\n",
    )
    .unwrap();
    for &(name, source, error, pages, expected) in FIXTURES {
        std::fs::write(dir.join("probe.tex"), source).unwrap();
        let _ = std::fs::remove_file(dir.join("probe.dvi"));
        let output = std::process::Command::new(
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
        assert_eq!(output.status.success(), error.is_none(), "{name}: {log}");
        let actual: Vec<_> = log
            .split("CHECK:")
            .skip(1)
            .map(|s| s.split_once(":END").unwrap().0)
            .collect();
        assert_eq!(actual, expected, "{name}: {log}");
        let mut e = Engine::new(source.as_bytes());
        assert_eq!(
            e.advance(usize::MAX),
            error.map_or(Ok(Progress::Complete), Err),
            "{name}"
        );
        let out = e.outcome();
        assert_eq!(out.pages.len(), pages, "{name}");
        if pages == 0 {
            assert!(!dir.join("probe.dvi").exists(), "{name}");
        } else {
            let dvi = crate::page_dvi_tests::read(
                &std::fs::read(dir.join("probe.dvi")).unwrap(),
                &out.pages,
            );
            let ours: Vec<Vec<_>> = out.pages.iter().map(|p| p.placements().collect()).collect();
            assert_eq!(dvi, ours, "{name}: {log}");
        }
    }
    std::fs::remove_dir_all(&dir).unwrap();
}
#[test]
fn output_routine_can_suspend_horizontal_outer_state_and_finish_paragraph() {
    let source = format!(
        r"{BOOT}\parindent=0pt\baselineskip=2pt\output{{\shipout\box255}}\hbox{{\vrule width2pt height11pt}}\noindent\vrule width1pt height1pt\par\end"
    );
    let mut e = Engine::new(source.as_bytes());
    e.advance(usize::MAX).unwrap();
    let out = e.outcome();
    assert_eq!(out.pages.len(), 2);
    let widths: Vec<_> = out
        .pages
        .iter()
        .flat_map(|p| {
            p.placements().filter_map(|p| match p {
                Placement::Rule { width, .. } => Some(width),
                _ => None,
            })
        })
        .collect();
    assert_eq!(widths, [2 * 65536, 65536]);
}
#[test]
fn output_routine_retains_fonts_and_page_prefix_across_replay() {
    let source = format!(
        r"{BOOT}\font\f=test\f\count0=1\output{{\shipout\box255}}\relax\hbox{{abc}}\penalty-10000\end"
    );
    let mut e = Engine::new(source.as_bytes());
    e.register_tfm(b"test", &crate::font_test_data::basic())
        .unwrap();
    while e.outcome().counts[0] == 0 {
        e.advance(1).unwrap();
    }
    let saved = e.checkpoint().unwrap();
    e.advance(usize::MAX).unwrap();
    let old = e.outcome();
    let placements: Vec<_> = old.pages.get(0).unwrap().placements().collect();
    e.restore(&saved).unwrap();
    e.advance(usize::MAX).unwrap();
    let replay = e.outcome();
    drop(saved);
    drop(e);
    assert_eq!(
        old.pages.get(0).unwrap().placements().collect::<Vec<_>>(),
        placements
    );
    assert_eq!(
        replay
            .pages
            .get(0)
            .unwrap()
            .placements()
            .collect::<Vec<_>>(),
        placements
    );
    assert_eq!(old.pages.get(0).unwrap().fonts()[0].name(), b"test");
}
#[test]
fn output_routine_leftover_source_role_and_owner_survive_checkpoint() {
    let source = format!(
        r"{BOOT}\let\bgroup={{\let\egroup=}}\let\finish=\egroup\toks0={{\bgroup\shipout\box255\finish\count0=7\relax}}\output=\toks0\hbox{{\vrule width2pt height3pt}}\penalty-10000\end"
    );
    let mut e = Engine::new(source.as_bytes());
    while e.outcome().pages.is_empty() {
        e.advance(1).unwrap();
    }
    assert_eq!(e.outcome().counts[0], 0);
    assert!(
        e.document
            .sources
            .iter()
            .any(|s| s.role() == crate::token::SourceRole::Output)
    );
    let saved = e.checkpoint().unwrap();
    for _ in 0..2 {
        e.advance(usize::MAX).unwrap();
        assert_eq!(e.outcome().counts[0], 7);
        e.restore(&saved).unwrap();
        assert_eq!(e.outcome().counts[0], 0);
        assert!(
            e.document
                .sources
                .iter()
                .any(|s| s.role() == crate::token::SourceRole::Output)
        );
    }
}
#[test]
fn output_routine_multispan_exhaustion_and_noexpand_closing_role() {
    let padding = "\\relax ".repeat(5000);
    let source = format!(r"{BOOT}\output{{\shipout\box255 {padding}}}\hbox{{}}\penalty-10000\end");
    assert_eq!(Engine::new(source.as_bytes()).run().unwrap().pages.len(), 1);
    let source = format!(
        r"{BOOT}\let\egroup=}}\output{{\shipout\box255\relax\egroup {padding}}}\hbox{{}}\penalty-10000\end"
    );
    let mut e = Engine::new(source.as_bytes());
    assert_eq!(e.advance(usize::MAX), Err(Error::Syntax));
    assert_eq!(e.outcome().pages.len(), 1);
    let source =
        format!(r"{BOOT}\output{{\shipout\box255\relax\noexpand}}\hbox{{}}\penalty-10000\end");
    assert_eq!(Engine::new(source.as_bytes()).run().unwrap().pages.len(), 1);
}
#[test]
fn output_routine_reserved_box_failure_preserves_assignment_and_error_restore() {
    let source =
        format!(r"{BOOT}{{\setbox255=\hbox to7pt{{}}\count0=1\relax\hbox{{}}\penalty-10000}}\end");
    let mut e = Engine::new(source.as_bytes());
    while e.outcome().counts[0] == 0 {
        e.advance(1).unwrap();
    }
    let saved = e.checkpoint().unwrap();
    let original = e.document.boxes.head(255);
    for _ in 0..2 {
        assert_eq!(e.advance(usize::MAX), Err(Error::Unsupported));
        assert_eq!(e.document.boxes.head(255), original);
        assert!(e.outcome().pages.is_empty());
        assert_eq!(
            e.document.scalars[Target::page(crate::scalars::PageParam::OutputPenalty)].value,
            -10000
        );
        e.restore(&saved).unwrap();
    }
}
#[test]
fn output_routine_eof_keeps_published_prefix_and_restores_suspended_roots() {
    let source = include_bytes!("../../../fixtures/output/eof.tex");
    for engine_first in [false, true] {
        let mut e = Engine::new(source);
        while e.outcome().counts[0] == 0 {
            e.advance(1).unwrap();
        }
        let saved = e.checkpoint().unwrap();
        let mut retained = Vec::new();
        for _ in 0..2 {
            assert_eq!(e.advance(usize::MAX), Err(Error::Eof));
            assert!(e.document.output_frame.is_some());
            assert!(!e.can_checkpoint());
            assert_eq!(e.outcome().pages.len(), 1);
            retained.push(e.outcome());
            e.restore(&saved).unwrap();
            assert!(e.document.output_frame.is_none());
            assert!(e.outcome().pages.is_empty());
        }
        if engine_first {
            drop(e);
            drop(saved);
        } else {
            drop(saved);
            drop(e);
        }
        for out in retained {
            assert_eq!(out.pages.get(0).unwrap().boxes().len(), 2);
        }
    }
}
