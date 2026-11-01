//! Font-free same-source insertion oracles, failure boundaries and snapshot replay.
use crate::{Engine, Error, Progress, nodes::Kind, numeric::Scaled, scalars::Target};
const BOOT: &str = r"\catcode123=1 \catcode125=2 \hsize=20pt \vsize=100pt \maxdepth=7pt \topskip=10pt \parindent=0pt \count5=1000 \dimen5=1000pt ";
const CASES: &[(&str, &str, &[&str])] = &[
    (
        "freeze",
        r"\count0=1000 \dimen0=1000pt \skip0=3pt plus2pt minus1pt \everyvbox{\message{BAD-HOOK}}\hangindent=1pt\insert0{\message{CHECK:CAPTURE=\the\prevdepth,\the\hangindent:END}\hrule height8pt depth2pt}\message{CHECK:FIRST=\the\pagegoal,\the\pagetotal,\the\pagedepth:END}\vsize=200pt\maxdepth=30pt\topskip=20pt\vskip50pt\kern12pt\penalty100\hrule height4pt depth3pt\par\message{CHECK:BOX=\the\pagegoal,\the\pagetotal,\the\pagedepth:END}\output{\message{CHECK:OUT=\the\pagegoal,\the\insertpenalties,\the\ht0,\the\dp0:END}\shipout\box255}\penalty-10000\end",
        &[
            "CAPTURE=-1000.0pt,0.0pt",
            "FIRST=87.0pt,0.0pt,0.0pt",
            "BOX=87.0pt,20.0pt,3.0pt",
            "OUT=87.0pt,0,8.0pt,2.0pt",
        ],
    ),
    (
        "capture",
        r"\everyvbox{\message{BAD-HOOK}}\boxmaxdepth=0pt\setbox1=\hbox{\spacefactor1700\insert5{\hrule height2pt depth1pt}\message{CHECK:INS=\the\spacefactor:END}\vadjust{\hrule height4pt depth1pt}\message{CHECK:ADJ=\the\spacefactor:END}}\message{CHECK:WIDTH=\the\wd1:END}\shipout\copy1\message{CHECK:RETAINED=\ifvoid5 Y\else N\fi:END}\output{\message{CHECK:OUT=\the\ht5,\the\dp5:END}\shipout\box255}\hbox{\insert5{\hrule width3pt height2pt depth1pt}\vadjust{\hrule width4pt height4pt depth1pt}}\penalty-10000\end",
        &[
            "INS=1700",
            "ADJ=1700",
            "WIDTH=0.0pt",
            "RETAINED=Y",
            "OUT=2.0pt,1.0pt",
        ],
    ),
    (
        "levels",
        r"\setbox5=\vbox{\hrule height1pt}\output{\message{CHECK:INSIDE=\the\ht5:END}\shipout\box255}{\setbox5=\vbox{\hrule height2pt}\insert5{\hrule height3pt}\hbox{}\penalty-10000\message{CHECK:LOCAL=\the\ht5:END}}\message{CHECK:RESTORED=\the\ht5:END}\end",
        &["INSIDE=5.0pt", "LOCAL=5.0pt", "RESTORED=1.0pt"],
    ),
    (
        "paragraph",
        r"\baselineskip=12pt\interlinepenalty=31\output{\message{CHECK:PAR=\the\ht5,\the\dp5:END}\shipout\vbox{\unvbox5\unvbox255}}\noindent\vrule width1pt height2pt depth1pt\insert5{\hrule width3pt height3pt depth2pt}\vadjust{\hrule width2pt height4pt depth1pt}\hfil\penalty-10000\vrule width4pt height2pt depth1pt\par\penalty-10000\end",
        &["PAR=3.0pt,2.0pt"],
    ),
    (
        "holding",
        r"\holdinginserts=1\output{\message{CHECK:HOLD=\ifvoid5 Y\else N\fi,\the\insertpenalties:END}\shipout\box255}\insert5{\hrule width9pt height3pt}\hbox{\vrule width1pt height2pt}\penalty-10000\end",
        &["HOLD=Y,0"],
    ),
    (
        "direct-state",
        r"{\floatingpenalty=9\holdinginserts=2\insertpenalties=7}\message{CHECK:STATE=\the\floatingpenalty,\the\holdinginserts,\the\insertpenalties:END}\output{\message{CHECK:ENTRY=\the\insertpenalties:END}\insertpenalties=23\shipout\box255}\hbox{}\penalty-10000\message{CHECK:AFTER=\the\insertpenalties:END}\end",
        &["STATE=0,0,7", "ENTRY=0", "AFTER=0"],
    ),
    (
        "scaling",
        r"\count5=333\skip5=2sp plus3sp minus4sp\insert5{\kern1001sp}\message{CHECK:SCALE=\number\pagegoal,\number\pagestretch,\number\pageshrink:END}\output{\shipout\vbox{\unvbox5\unvbox255}}\hbox{}\penalty-10000\end",
        &["SCALE=6553265,3,4"],
    ),
    (
        "class-timing",
        r"\skip5=1pt plus2pt\insert5{\hrule height2pt}\skip5=10pt plus20pt\count5=500\insert5{\hrule height2pt}\message{CHECK:TIMING=\the\pagegoal,\the\pagestretch:END}\output{\message{CHECK:CLASS=\the\ht5:END}\shipout\box255}\hbox{}\penalty-10000\end",
        &["TIMING=96.00055pt,2.0pt", "CLASS=4.0pt"],
    ),
    (
        "empty",
        r"\insert5{}\message{CHECK:EMPTY=\the\pagegoal:END}\output{\message{CHECK:CLASS=\ifvoid5 V\else B\fi,\the\ht5,\the\dp5:END}\shipout\box255}\hbox{}\penalty-10000\end",
        &["EMPTY=100.0pt", "CLASS=B,0.0pt,0.0pt"],
    ),
    (
        "selected",
        r"\vsize=30pt\topskip=0pt\output{\message{CHECK:SELECT=\the\ht5:END}\shipout\vbox{\unvbox5\unvbox255}}\insert5{\hrule width1pt height1pt}\hbox{\vrule width2pt height10pt}\penalty0\insert5{\hrule width3pt height1pt}\hrule width4pt height30pt\vskip0pt\par\end",
        &["SELECT=1.0pt", "SELECT=1.0pt", "SELECT=0.0pt"],
    ),
];
fn markers(e: &Engine<'_>) -> Vec<String> {
    e.outcome()
        .message_bytes()
        .into_iter()
        .filter_map(|b| {
            let s = String::from_utf8(b).unwrap();
            s.strip_prefix("CHECK:")
                .and_then(|s| s.strip_suffix(":END"))
                .map(str::to_owned)
        })
        .collect()
}
#[test]
fn insertion_same_source_semantics() {
    for &(name, body, expected) in CASES {
        let source = format!("{BOOT}{body}");
        let mut e = Engine::new(source.as_bytes());
        assert_eq!(e.advance(usize::MAX), Ok(Progress::Complete), "{name}");
        assert_eq!(markers(&e), expected, "{name}");
    }
}
#[test]
#[ignore = "requires explicit UMBER_PDFTEX native oracle"]
fn insertion_native_boxes_messages_and_placements() {
    let dir = std::env::temp_dir().join(format!("umber-insertion-oracle-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("texmf.cnf"),
        "TEXINPUTS=.\nshell_escape=f\nmax_print_line=10000\n",
    )
    .unwrap();
    for &(name, body, expected) in CASES {
        let source = format!("{BOOT}{body}");
        std::fs::write(dir.join("probe.tex"), &source).unwrap();
        let result = std::process::Command::new(
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
        assert!(result.status.success(), "{name}: {log}");
        let actual: Vec<_> = log
            .split("CHECK:")
            .skip(1)
            .map(|s| s.split_once(":END").unwrap().0)
            .collect();
        assert_eq!(actual, expected, "{name}: {log}");
        let mut e = Engine::new(source.as_bytes());
        e.run_input().unwrap();
        let out = e.outcome();
        let dvi =
            crate::page_dvi_tests::read(&std::fs::read(dir.join("probe.dvi")).unwrap(), &out.pages);
        let ours: Vec<Vec<_>> = out.pages.iter().map(|p| p.placements().collect()).collect();
        assert_eq!(dvi, ours, "{name}: {log}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn insertion_capture_fields_before_unsave_and_unconstrained_depth() {
    let source = format!(
        r"{BOOT}\splittopskip=2pt\splitmaxdepth=3pt\floatingpenalty=4\boxmaxdepth=0pt\setbox0=\hbox{{\insert5{{\splittopskip=7pt plus1fil\splitmaxdepth=8pt\floatingpenalty=9\hrule height2pt depth6pt}}}}\end"
    );
    let mut e = Engine::new(source.as_bytes());
    e.run_input().unwrap();
    let Kind::Box(id) = &e.document.nodes.get(e.document.boxes.head(0)).kind else {
        panic!()
    };
    let head = e.document.nodes.box_data(id).list.head();
    let Kind::Insertion(id) = &e.document.nodes.get(head).kind else {
        panic!()
    };
    let data = e.document.nodes.insertion_data(id);
    assert_eq!(
        (data.height, data.max_depth, data.float_cost),
        (Scaled(8 * 65536), Scaled(8 * 65536), 9)
    );
    assert_eq!(data.split_top.width, Scaled(7 * 65536));
    assert_eq!(data.split_top.stretch, Scaled::ONE);
    assert_eq!(e.document.scalars[Target::FLOATING_PENALTY].value, 4);
}
#[test]
fn insertion_failure_restore_pending_pages_payloads_and_open_frames() {
    for body in [
        r"\insert5{\hrule height101pt}",
        r"\dimen5=1pt\insert5{\hrule height2pt}",
        r"\insert5{\hbox{\undefined}}",
        r"\insert255{}",
        r"\vadjust{}",
    ] {
        let source = format!(r"{BOOT}\insert5{{\hrule height1pt}}\count0=1\relax{body}");
        let mut e = Engine::new(source.as_bytes());
        while e.outcome().counts[0] == 0 {
            e.advance(1).unwrap();
        }
        let saved = e.checkpoint().unwrap();
        for _ in 0..3 {
            assert!(
                matches!(
                    e.advance(usize::MAX),
                    Err(Error::Unsupported | Error::Undefined | Error::Syntax)
                ),
                "{body}"
            );
            e.restore(&saved).unwrap();
            assert!(e.can_checkpoint());
            assert!(e.document.box_frames.is_empty());
            assert_eq!(e.document.page.quantity(0), Scaled(99 * 65536));
        }
    }
}
#[test]
fn insertion_checkpoint_replays_selected_classes_and_retained_output() {
    let source = format!(
        r"{BOOT}\insert5{{\hrule width3pt height2pt}}\count0=1\relax\output{{\shipout\vbox{{\unvbox5\unvbox255}}}}\hbox{{\vrule width4pt height5pt}}\penalty-10000\end"
    );
    let mut e = Engine::new(source.as_bytes());
    while e.outcome().counts[0] == 0 {
        e.advance(1).unwrap();
    }
    let saved = e.checkpoint().unwrap();
    e.run_input().unwrap();
    let retained = e.outcome();
    let expected: Vec<_> = retained.pages.get(0).unwrap().placements().collect();
    for _ in 0..3 {
        e.restore(&saved).unwrap();
        e.run_input().unwrap();
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
    drop(e);
    drop(saved);
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

#[test]
fn insertion_class_preparation_fails_before_transferring_any_children() {
    let source = format!(
        r"{BOOT}\count6=1000\dimen6=1000pt\insert6{{\hrule height1pt}}\insert5{{\hrule height1pt}}\setbox6=\hbox{{}}\hbox{{}}\penalty-10000"
    );
    let mut e = Engine::new(source.as_bytes());
    assert_eq!(e.advance(usize::MAX), Err(Error::Unsupported));
    let mut p = e.document.page.current.head();
    let mut insertions = 0;
    while !p.is_null() {
        let node = e.document.nodes.get(p);
        if let Kind::Insertion(id) = &node.kind {
            assert!(!e.document.nodes.insertion_data(id).list.is_empty());
            insertions += 1;
        }
        p = node.next;
    }
    assert_eq!(insertions, 2);
    let Kind::Box(id) = &e.document.nodes.get(e.document.boxes.head(5)).kind else {
        panic!()
    };
    assert!(e.document.nodes.box_data(id).list.is_empty());
}
