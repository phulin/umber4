//! Independent typed AST/native source pairs, compared through ordinary packing.
use super::{
    tests::{BOOT, add, character, metrics},
    *,
};
use crate::{
    Engine, PackedBox, Placement, nodes,
    numeric::{Scaled, print_scaled},
    packing::PackSpec,
};
use std::{path::PathBuf, process::Command};
fn scaled(v: i32) -> String {
    let mut b = Vec::new();
    print_scaled(Scaled(v), |c| b.push(c));
    String::from_utf8(b).unwrap()
}
fn displayed(b: &PackedBox, boxes: &[PackedBox]) -> String {
    let mut s = format!(
        "\\{}box({}+{})x{}",
        if b.vertical { "v" } else { "h" },
        scaled(b.height),
        scaled(b.depth),
        scaled(b.width)
    );
    if b.glue.sign != 0 && b.glue.ratio != 0.0 {
        s.push_str(", glue set ");
        if b.glue.sign < 0 {
            s.push_str("- ");
        }
        s.push_str(&scaled((b.glue.ratio * 65536.0).round() as i32));
        s.push_str(["", "fil", "fill", "filll"][b.glue.order as usize]);
    }
    if let Some(parent) = b.parent {
        let parent = boxes[parent as usize];
        let shift = if parent.vertical {
            b.x - parent.x
        } else {
            b.y - parent.y
        };
        if shift != 0 {
            s.push_str(&format!(", shifted {}", scaled(shift)));
        }
    }
    s
}
struct Oracle {
    dir: PathBuf,
    binary: PathBuf,
}
impl Drop for Oracle {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
impl Oracle {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "umber-math-core-oracle-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("texmf.cnf"),
            b"TEXINPUTS = .\nTFMFONTS = .\nshell_escape = f\nmax_print_line = 10000\n",
        )
        .unwrap();
        Self {
            dir,
            binary: std::env::var_os("UMBER_PDFTEX")
                .expect("explicit native oracle executable")
                .into(),
        }
    }
    fn compare(
        &self,
        tfm: &[u8],
        settings: &str,
        body: &str,
        style: Style,
        build: impl FnOnce(&mut Engine<'_>, &mut MathStore) -> MathList,
    ) {
        self.compare_setup(
            &[(b"math", tfm)],
            BOOT.trim_end_matches("\\end"),
            settings,
            body,
            style,
            build,
        );
    }
    fn compare_setup(
        &self,
        fonts: &[(&[u8], &[u8])],
        boot: &str,
        settings: &str,
        body: &str,
        style: Style,
        build: impl FnOnce(&mut Engine<'_>, &mut MathStore) -> MathList,
    ) {
        let setup = format!("{boot}{settings}\\end");
        let mut e = Engine::new(setup.as_bytes());
        for (name, bytes) in fonts {
            e.register_tfm(name, bytes).unwrap();
        }
        e.advance(usize::MAX).unwrap();
        let mut math = MathStore::default();
        let l = build(&mut e, &mut math);
        let l = e
            .math_to_hlist(&mut math, l, style, false, &mut Scratch::default())
            .unwrap();
        assert_eq!(math.live, 0);
        let b = e.hpack(l, PackSpec::Natural).unwrap();
        e.publish_page(b, 0).unwrap();
        assert_eq!(e.document.nodes.live_count(), 0);
        let outcome = e.outcome();
        let style_name = [
            "\\displaystyle",
            "\\textstyle",
            "\\scriptstyle",
            "\\scriptscriptstyle",
        ][style.choice()];
        // Native-only overline supplies cramped display/text styles. Its
        // outer hbox/vbox and rule are excluded; compare the clean inner box
        // and relative glyph placements of the formula under test.
        let formula = if style.cramped() {
            format!("{style_name}\\overline{{{body}}}")
        } else {
            format!("{style_name} {body}")
        };
        let native = format!(
            r"{boot}{settings}\tracingoutput=1\showboxbreadth=10000\showboxdepth=10000\hbadness=10000\hfuzz=16383pt\setbox0=\hbox{{${formula}$}}\shipout\box0\end"
        );
        for (name, bytes) in fonts {
            std::fs::write(
                self.dir
                    .join(format!("{}.tfm", std::str::from_utf8(name).unwrap())),
                bytes,
            )
            .unwrap();
        }
        std::fs::write(self.dir.join("probe.tex"), &native).unwrap();
        let output = Command::new(&self.binary)
            .current_dir(&self.dir)
            .env("TEXMFCNF", &self.dir)
            .env("TFMFONTS", &self.dir)
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
        let log = String::from_utf8_lossy(&std::fs::read(self.dir.join("probe.log")).unwrap())
            .into_owned();
        assert!(output.status.success(), "{native}\n{log}");
        let expected: Vec<_> = outcome
            .pages
            .iter()
            .flat_map(|p| p.boxes().iter().map(|b| displayed(b, p.boxes())))
            .collect();
        let actual: Vec<_> = log
            .lines()
            .map(|l| l.trim_start_matches('.'))
            .filter(|l| l.starts_with("\\vbox(") || l.starts_with("\\hbox("))
            .skip(if style.cramped() { 2 } else { 0 })
            .map(str::to_owned)
            .collect();
        assert_eq!(expected, actual, "box structures: {body}\n{log}");
        let mut actual = crate::page_dvi_tests::read(
            &std::fs::read(self.dir.join("probe.dvi")).unwrap(),
            &outcome.pages,
        );
        let mut expected: Vec<Vec<Placement>> = outcome
            .pages
            .iter()
            .map(|p| p.placements().collect())
            .collect();
        if style.cramped() {
            normalize_glyphs(&mut actual);
            normalize_glyphs(&mut expected);
        }
        assert_eq!(expected, actual, "DVI placements: {body}\n{log}");
    }
}
fn class_source(class: Class, c: u8) -> String {
    let name = [
        "mathord",
        "mathop",
        "mathbin",
        "mathrel",
        "mathopen",
        "mathclose",
        "mathpunct",
        "mathinner",
    ][class as usize];
    format!("\\{name}\\mathchar\"00{c:02X}")
}
fn chars(e: &mut Engine<'_>, m: &mut MathStore, bytes: &[u8]) -> MathList {
    let mut l = MathList::default();
    for c in bytes {
        add(e, m, &mut l, Atom::new(Class::Ord, character(*c)));
    }
    l
}
#[test]
#[ignore = "native typed-math oracle: set UMBER_PDFTEX explicitly"]
fn upstream_math_atoms_styles_ligatures_scripts_operators_and_choices() {
    let oracle = Oracle::new();
    let tfm = metrics(&[], &[]);
    let classes = [
        Class::Ord,
        Class::Op,
        Class::Bin,
        Class::Rel,
        Class::Open,
        Class::Close,
        Class::Punct,
        Class::Inner,
    ];
    for style in [
        Style::DISPLAY,
        Style::TEXT,
        Style::new(4).unwrap(),
        Style::new(6).unwrap(),
    ] {
        for before in classes {
            for after in classes {
                let classes = [Class::Ord, before, after, Class::Ord];
                let body = classes
                    .iter()
                    .zip(b"abcd")
                    .map(|(class, c)| class_source(*class, *c))
                    .collect::<Vec<_>>()
                    .join(" ");
                oracle.compare(&tfm, "", &body, style, |e, m| {
                    let mut l = MathList::default();
                    for (class, c) in classes.iter().zip(b"abcd") {
                        add(e, m, &mut l, Atom::new(*class, character(*c)));
                    }
                    l
                });
            }
        }
        for sup in [false, true] {
            for sub in [false, true] {
                let body = format!(
                    "\\mathchar\"0061{}{}",
                    if sup {
                        "^{\\mathchar\"0062\\mathchar\"0063}"
                    } else {
                        ""
                    },
                    if sub {
                        "_{\\mathchar\"0064\\mathchar\"0065}"
                    } else {
                        ""
                    }
                );
                oracle.compare(&tfm, "", &body, style, |e, m| {
                    let mut a = Atom::new(Class::Ord, character(b'a'));
                    if sup {
                        a.sup = Field::SubMath(chars(e, m, b"bc"));
                    }
                    if sub {
                        a.sub = Field::SubMath(chars(e, m, b"de"));
                    }
                    let mut l = MathList::default();
                    add(e, m, &mut l, a);
                    l
                });
            }
        }
        for policy in [Limits::Normal, Limits::Side, Limits::AboveBelow] {
            let suffix = match policy {
                Limits::Normal => "",
                Limits::Side => "\\nolimits",
                Limits::AboveBelow => "\\limits",
            };
            for (sup, sub) in [(false, false), (true, false), (false, true), (true, true)] {
                let body = format!(
                    "\\mathop\\mathchar\"0061{suffix}{}{}",
                    if sup { "^{\\mathchar\"0062}" } else { "" },
                    if sub { "_{\\mathchar\"0063}" } else { "" }
                );
                oracle.compare(&tfm, "", &body, style, |e, m| {
                    let mut a = Atom::new(Class::Op, character(b'a'));
                    a.limits = policy;
                    if sup {
                        a.sup = character(b'b');
                    }
                    if sub {
                        a.sub = character(b'c');
                    }
                    let mut l = MathList::default();
                    add(e, m, &mut l, a);
                    l
                });
            }
        }
        oracle.compare(&tfm,"",r"\mathchar97 \mathchoice{\mathbin\mathchar98 }{\mathrel\mathchar99 }{\mathpunct\mathchar100 }{\mathopen\mathchar101 }\mathchar102 ",style,|e,m| {
            let mut choices=std::array::from_fn(|_|MathList::default()); for (i,class) in [Class::Bin,Class::Rel,Class::Punct,Class::Open].into_iter().enumerate() { add(e,m,&mut choices[i],Atom::new(class,character(b'b'+i as u8))); }
            let mut l=chars(e,m,b"a");m.append(&mut l,Kind::Choice(choices),&mut e.document.nodes).unwrap();add(e,m,&mut l,Atom::new(Class::Ord,character(b'f')));l
        });
    }
    // All math ligature operations, including text-italic suppression and script
    // transfer when an Ord replaces the following differently classed atom.
    for op in 0..=11 {
        let tfm = metrics(&[[128, b'b', op, b'c']], &[(b'a', 1, 0)]);
        oracle.compare(
            &tfm,
            "",
            r"\mathchar97 \mathrel\mathchar98 ^{\mathchar100 }",
            Style::TEXT,
            |e, m| {
                let mut l = chars(e, m, b"a");
                let mut a = Atom::new(Class::Rel, character(b'b'));
                a.sup = character(b'd');
                add(e, m, &mut l, a);
                l
            },
        );
    }
    for program in [
        vec![[128, b'b', 128, 0]],
        vec![[255, b'b', 0, 1], [128, b'a', 0, b'c'], [255, 0, 0, 1]],
    ] {
        let tags = if program.len() == 1 {
            vec![(b'a', 1, 0)]
        } else {
            vec![]
        };
        let tfm = metrics(&program, &tags);
        oracle.compare(
            &tfm,
            "",
            r"\mathchar97 \mathchar98 ",
            Style::TEXT,
            |e, m| chars(e, m, b"ab"),
        );
    }
    let tfm = metrics(&[], &[(b'a', 2, b'b')]);
    oracle.compare(
        &tfm,
        "",
        r"\mathop\mathchar97 _{\mathchar99 }^{\mathchar100 }",
        Style::DISPLAY,
        |e, m| {
            let mut a = Atom::new(Class::Op, character(b'a'));
            a.sub = character(b'c');
            a.sup = character(b'd');
            let mut l = MathList::default();
            add(e, m, &mut l, a);
            l
        },
    );
    for shifted in [false, true] {
        let body = if shifted {
            r"\hbox{\raise2pt\hbox{\kern7pt}}_{\mathchar99 }^{\mathchar100 }"
        } else {
            r"\hbox{\kern7pt}_{\mathchar99 }^{\mathchar100 }"
        };
        oracle.compare(&tfm, "", body, Style::TEXT, |e, m| {
            let mut content = nodes::List::default();
            let kern = e
                .document
                .nodes
                .alloc(nodes::Kind::Kern {
                    width: Scaled(7 * 65536),
                    explicit: true,
                })
                .unwrap();
            e.document.nodes.append(&mut content, kern);
            let b = e.hpack(content, PackSpec::Natural).unwrap();
            let b = if shifted {
                e.document.nodes.box_mut(b.id()).shift = Scaled(-2 * 65536);
                let mut content = nodes::List::default();
                e.document.nodes.append(&mut content, b);
                e.hpack(content, PackSpec::Natural).unwrap()
            } else {
                b
            };
            let mut a = Atom::new(Class::Ord, Field::Box(b));
            a.sub = character(b'c');
            a.sup = character(b'd');
            let mut l = MathList::default();
            add(e, m, &mut l, a);
            l
        });
    }
    // Style nodes inside a chosen branch affect later siblings. The choice is
    // not an independently converted subformula.
    oracle.compare(
        &tfm,
        "",
        r"\mathchar97 \mathchoice{}{\scriptstyle\mathbin\mathchar98 }{}{}\mathchar99 ",
        Style::TEXT,
        |e, m| {
            let mut branch = MathList::default();
            m.append(
                &mut branch,
                Kind::Style(Style::new(4).unwrap()),
                &mut e.document.nodes,
            )
            .unwrap();
            add(e, m, &mut branch, Atom::new(Class::Bin, character(b'b')));
            let mut l = chars(e, m, b"a");
            m.append(
                &mut l,
                Kind::Choice([
                    MathList::default(),
                    branch,
                    MathList::default(),
                    MathList::default(),
                ]),
                &mut e.document.nodes,
            )
            .unwrap();
            add(e, m, &mut l, Atom::new(Class::Ord, character(b'c')));
            l
        },
    );
    // Cramped script/script-script styles arise through nested subscript fields.
    for superscript in [false, true] {
        let body = if superscript {
            r"\mathchar97 ^{\mathchar98 _{\mathchar99 }^{\mathchar100 }}"
        } else {
            r"\mathchar97 _{\mathchar98 _{\mathchar99 }^{\mathchar100 }}"
        };
        oracle.compare(&tfm, "", body, Style::TEXT, |e, m| {
            let mut inner = Atom::new(Class::Ord, character(b'b'));
            inner.sub = character(b'c');
            inner.sup = character(b'd');
            let mut field = MathList::default();
            add(e, m, &mut field, inner);
            let mut outer = Atom::new(Class::Ord, character(b'a'));
            if superscript {
                outer.sup = Field::SubMath(field)
            } else {
                outer.sub = Field::SubMath(field)
            }
            let mut l = MathList::default();
            add(e, m, &mut l, outer);
            l
        });
    }
}

fn normalize_glyphs(pages: &mut [Vec<Placement>]) {
    for page in pages {
        page.retain(|p| matches!(p, Placement::Glyph { .. }));
        let Some(Placement::Glyph {
            x: origin_x,
            y: origin_y,
            ..
        }) = page.first().copied()
        else {
            continue;
        };
        for item in page {
            if let Placement::Glyph { x, y, .. } = item {
                *x -= origin_x;
                *y -= origin_y;
            }
        }
    }
}
#[path = "math_real_oracle_tests.rs"]
mod real;
