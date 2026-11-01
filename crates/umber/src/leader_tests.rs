//! Ordinary leaders: borrowed repeated output, scanners and semantic owners.
use crate::{
    Engine, Error, Progress,
    nodes::{GlueKind, Kind},
    output::Placement,
};
pub(super) const BOOT: &str = r"\catcode123=1\catcode125=2\catcode35=6\hsize=70pt\vsize=100pt\topskip=0pt\maxdepth=0pt\parindent=0pt\parfillskip=0pt plus1fil ";
pub(super) fn cases() -> Vec<String> {
    let mut cases = Vec::new();
    for alignment in ["leaders", "cleaders", "xleaders"] {
        for space in [0, 1, 4, 5, 6, 19, 20, 21, 37, 100] {
            cases.push(format!(r"\setbox0=\hbox{{\kern-5sp\{alignment}\hbox{{\vrule width7sp height3sp depth2sp}}\hskip{space}sp\kern2sp}}"));
            cases.push(format!(r"\setbox0=\vbox{{\kern-3sp\{alignment}\hbox{{\vrule width7sp height3sp depth2sp}}\vskip{space}sp\kern2sp}}"));
        }
        for extent in [-2, 0, 2] {
            cases.push(format!(r"\setbox0=\hbox{{\{alignment}\hbox to{extent}sp{{\vrule width0sp height3sp}}\hskip30sp}}"));
            cases.push(format!(r"\setbox0=\vbox{{\{alignment}\vbox to{extent}sp{{\hrule width3sp height0sp}}\vskip30sp}}"));
        }
        cases.push(format!(r"\setbox2=\hbox{{\lower2sp\hbox{{\vrule width7sp height3sp depth2sp}}\global\setbox3=\lastbox}}\hoffset=-2pt\voffset=3pt\setbox0=\hbox{{\kern3sp\{alignment}\box3\hskip37sp}}"));
        cases.push(format!(r"\setbox2=\vbox{{\moveright4sp\hbox{{\vrule width7sp height3sp depth2sp}}\global\setbox3=\lastbox}}\hoffset=2pt\voffset=-3pt\setbox0=\vbox{{\kern3sp\{alignment}\box3\vskip37sp}}"));
        cases.push(format!(r"\setbox0=\hbox to100sp{{\vrule width2sp height6sp depth2sp\{alignment}\vrule width900sp\hfill}}"));
        cases.push(format!(r"\setbox0=\vbox to100sp{{\hrule width9sp height2sp\{alignment}\hrule height900sp depth800sp\vfill}}"));
        cases.push(format!(r"\setbox0=\hbox{{\kern-3sp\{alignment}\hbox to11sp{{\kern-2sp\leaders\hbox{{\vrule width3sp height2sp}}\hfil}}\hskip73sp}}"));
        cases.push(format!(r"\setbox0=\vbox{{\kern-3sp\{alignment}\vbox to11sp{{\kern-2sp\leaders\vbox{{\hrule width3sp height2sp}}\vfil}}\vskip73sp}}"));
    }
    cases.extend([
        r"\setbox0=\hbox to101sp{\hskip0sp plus1fil\cleaders\hbox{\vrule width7sp height3sp}\hskip1sp plus2fil\hskip1sp plus1fil\xleaders\hbox{\vrule width9sp height4sp}\hskip0sp plus3fil}",
        r"\setbox0=\hbox to101sp{\hskip200sp minus50sp\cleaders\hbox{\vrule width7sp height3sp}\hskip0sp minus100sp\kern2sp}",
        r"\setbox0=\vbox to101sp{\vskip0sp plus1fil\cleaders\vbox{\hrule width7sp height3sp}\vskip1sp plus2fil\vskip1sp plus1fil\xleaders\vbox{\hrule width9sp height4sp}\vskip0sp plus3fil}",
        r"\setbox0=\vbox to101sp{\vskip200sp minus50sp\cleaders\vbox{\hrule width7sp height3sp}\vskip0sp minus100sp\kern2sp}",
        r"\setbox0=\hbox to20sp{\leaders\box10\hfil\vrule width2sp height3sp}",
        r"\setbox0=\hbox{\spacefactor1700\leaders\hrule\hskip12sp\message{CHECK:SF:\the\spacefactor:END}}",
        r"\setbox0=\vbox{\prevdepth=7sp\leaders\vrule width3sp\vskip12sp\message{CHECK:PD:\number\prevdepth:END}}",
        r"\everyhbox{\message{CHECK:HOOK:END}}\setbox0=\hbox{\leaders\hbox{\immediate\write16{CHECK:ONCE:END}\vrule width3sp height2sp}\hskip30sp}",
        r"\setbox0=\hbox{\leaders\relax\hrule\relax\hskip10sp\vrule width2sp height3sp}",
    ].map(str::to_owned));
    cases
}
pub(super) fn source(body: &str) -> String {
    format!(
        r"{BOOT}{body}\message{{CHECK:BOX:\number\wd0,\number\ht0,\number\dp0:END}}\shipout\box0\setbox2=\box255\setbox3=\box255\end"
    )
}
#[test]
fn leader_all_axis_variants_and_scanner_cases_complete() {
    for body in cases() {
        let source = source(&body);
        let mut e = Engine::new(source.as_bytes());
        assert_eq!(e.advance(usize::MAX), Ok(Progress::Complete), "{body}");
        assert_eq!(e.document.nodes.live_count(), 0, "{body}");
        assert_eq!(e.outcome().pages.len(), 1, "{body}");
    }
}
#[test]
fn leader_inert_rightskip_and_tail_penalty_keep_exact_owners() {
    let source = format!(
        r"{BOOT}\hsize=12pt\noindent\vrule width10pt height1pt\leaders\hbox{{\vrule width1pt height9pt}}\hskip1pt\vrule width10pt height1pt\par\count0=1\relax\end"
    );
    let mut e = Engine::new(source.as_bytes());
    while e.outcome().counts[0] == 0 {
        e.advance(1).unwrap();
    }
    let mut p = e.document.page.current.head();
    let mut inert = 0;
    while !p.is_null() {
        if let Kind::Box(id) = &e.document.nodes.get(p).kind {
            let parent_height = e.document.nodes.box_data(id).height.0;
            let mut child = e.document.nodes.box_data(id).list.head();
            while !child.is_null() {
                if let Kind::Leader(id) = &e.document.nodes.get(child).kind {
                    let data = e.document.nodes.leader_data(id);
                    if data.subtype == GlueKind::RightSkip {
                        assert!(!data.list.is_empty());
                        assert_eq!(parent_height, 65536);
                        inert += 1;
                    }
                }
                child = e.document.nodes.get(child).next;
            }
        }
        p = e.document.nodes.get(p).next;
    }
    assert_eq!(inert, 1);
    e.run_input().unwrap();
    assert_eq!(e.document.nodes.live_count(), 0);
    let source = format!(
        r"{BOOT}\noindent\leaders\hbox{{\vrule width1pt height1pt}}\hskip0pt plus1fil\par\end"
    );
    let mut e = Engine::new(source.as_bytes());
    e.run_input().unwrap();
    assert_eq!(e.document.nodes.live_count(), 0);
}
#[test]
fn leader_replay_copy_nonconsuming_templates_and_published_teardown() {
    let source = format!(
        r"{BOOT}\setbox0=\hbox{{\xleaders\hbox{{\cleaders\hbox{{\vrule width1pt height2pt}}\hskip4pt}}\hskip23pt}}\count0=1\relax\shipout\copy0\shipout\copy0\setbox0=\box255\end"
    );
    for engine_first in [false, true] {
        let mut e = Engine::new(source.as_bytes());
        while e.outcome().counts[0] == 0 {
            e.advance(1).unwrap();
        }
        let saved = e.checkpoint().unwrap();
        let live = e.document.nodes.live_count();
        assert!(live > 0);
        e.run_input().unwrap();
        let retained = e.outcome();
        let expected: Vec<_> = retained.pages.get(0).unwrap().placements().collect();
        assert_eq!(
            expected,
            retained
                .pages
                .get(1)
                .unwrap()
                .placements()
                .collect::<Vec<_>>()
        );
        for _ in 0..2 {
            e.restore(&saved).unwrap();
            assert_eq!(e.document.nodes.live_count(), live);
            e.run_input().unwrap();
            assert_eq!(e.document.nodes.live_count(), 0);
            assert_eq!(
                e.outcome()
                    .pages
                    .get(0)
                    .unwrap()
                    .placements()
                    .collect::<Vec<_>>(),
                expected
            );
        }
        if engine_first {
            drop(e);
            drop(saved);
        } else {
            drop(saved);
            drop(e);
        }
        assert_eq!(
            retained
                .pages
                .get(0)
                .unwrap()
                .placements()
                .collect::<Vec<_>>(),
            expected
        );
    }
}
#[test]
fn leader_invalid_skip_releases_taken_template_and_restore_recovers_it() {
    for body in [
        r"\setbox1=\hbox{\leaders\box0\vskip1pt}",
        r"\setbox1=\hbox{\cleaders\box0\hskip bad}",
        r"\xleaders\hbox{\unknown}",
        r"\leaders\hbox{\kern1pt}\relax",
    ] {
        let source =
            format!(r"{BOOT}\setbox0=\hbox{{\vrule width1pt height1pt}}\count0=1\relax{body}");
        let mut e = Engine::new(source.as_bytes());
        while e.outcome().counts[0] == 0 {
            e.advance(1).unwrap();
        }
        let saved = e.checkpoint().unwrap();
        let original = e.document.boxes.head(0);
        for _ in 0..2 {
            assert!(matches!(
                e.advance(usize::MAX),
                Err(Error::Syntax | Error::Number | Error::Undefined | Error::Eof)
            ));
            if body.contains("\\box0") {
                assert!(e.document.boxes.head(0).is_null());
                assert_eq!(e.document.nodes.live_count(), 0);
            }
            e.restore(&saved).unwrap();
            assert_eq!(e.document.boxes.head(0), original);
        }
    }
}
#[test]
fn leader_repeat_origins_and_rule_extent_are_observable() {
    let source =
        source(r"\setbox0=\hbox{\kern2pt\cleaders\hbox{\vrule width3pt height1pt}\hskip10pt}");
    let out = Engine::new(source.as_bytes()).run().unwrap();
    let x: Vec<_> = out
        .pages
        .get(0)
        .unwrap()
        .placements()
        .filter_map(|p| match p {
            Placement::Rule { x, .. } => Some(x),
            _ => None,
        })
        .collect();
    assert_eq!(x, [163845, 360453, 557061]); // 2pt + (1pt+10sp)/2, then 3pt.
}

#[cfg(test)]
#[path = "leader_oracle_tests.rs"]
mod oracle_tests;

#[test]
fn leader_failed_borrowed_lowering_releases_once_and_reuses_scratch_after_restore() {
    let source = format!(
        r"{BOOT}\font\f=test\f\setbox0=\hbox{{\leaders\hbox{{a}}\hskip20pt}}\count0=1\relax\shipout\box0\end"
    );
    let mut e = Engine::new(source.as_bytes());
    e.register_tfm(b"test", &crate::font_test_data::basic())
        .unwrap();
    while e.outcome().counts[0] == 0 {
        e.advance(1).unwrap();
    }
    let saved = e.checkpoint().unwrap();
    e.document.fonts.len = 0;
    assert_eq!(e.advance(usize::MAX), Err(Error::Font));
    assert!(e.document.boxes.head(0).is_null());
    assert_eq!(e.document.nodes.live_count(), 0);
    assert!(e.outcome().pages.is_empty());
    for _ in 0..2 {
        e.restore(&saved).unwrap();
        e.run_input().unwrap();
        assert_eq!(e.document.nodes.live_count(), 0);
        assert!(!e.outcome().pages.get(0).unwrap().glyphs().is_empty());
    }
}
