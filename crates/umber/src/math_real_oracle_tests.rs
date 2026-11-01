use super::*;
#[test]
#[ignore = "native typed-math oracle: set UMBER_PDFTEX explicitly"]
fn upstream_math_cramped_styles_signed_parameters_boxes_and_missing_glyphs() {
    let oracle = Oracle::new();
    let tfm = metrics(&[], &[]);
    for style in [1, 3, 5, 7].map(|s| Style::new(s).unwrap()) {
        for settings in [
            "",
            r"\fontdimen5\t=-.12345pt\fontdimen22\t=-.23457pt\fontdimen6\t=-.54321pt\thinmuskip=-3mu plus1fil minus2mu\medmuskip=-4mu plus1fill minus3mu\thickmuskip=-5mu plus2mu minus1fil ",
        ] {
            oracle.compare(
                &tfm,
                settings,
                r"\mathchar97 _{\mathchar98 }^{\mathchar99 }\mathbin\mathchar100 \mathchar101 ",
                style,
                |e, m| {
                    let mut a = Atom::new(Class::Ord, character(b'a'));
                    a.fields.sub = character(b'b');
                    a.fields.sup = character(b'c');
                    let mut l = MathList::default();
                    add(e, m, &mut l, a);
                    add(e, m, &mut l, Atom::new(Class::Bin, character(b'd')));
                    add(e, m, &mut l, Atom::new(Class::Ord, character(b'e')));
                    l
                },
            );
        }
    }
    for nucleus in [Class::Ord, Class::Op] {
        let body = format!(
            "{}{}",
            class_source(nucleus, 0),
            "_{\\mathchar97 }^{\\mathchar98 }"
        );
        oracle.compare(&tfm, "", &body, Style::TEXT, |e, m| {
            let mut a = Atom::new(nucleus, character(0));
            a.fields.sub = character(b'a');
            a.fields.sup = character(b'b');
            let mut l = MathList::default();
            add(e, m, &mut l, a);
            l
        });
    }
    // Boxed noncharacter operators retain their own geometry. Empty scripts
    // remain present fields and still add script space for side scripts.
    for vertical in [false, true] {
        for limits in [Limits::Side, Limits::AboveBelow] {
            let command = if vertical { "vbox" } else { "hbox" };
            let policy = if limits == Limits::Side {
                r"\nolimits"
            } else {
                r"\limits"
            };
            let body = format!(r"\mathop{{\{command}{{\kern7pt}}}}{policy}_{{}}^{{}}");
            oracle.compare(&tfm, "", &body, Style::TEXT, |e, m| {
                let mut contents = nodes::List::default();
                let n = e
                    .document
                    .nodes
                    .alloc(nodes::Kind::Kern {
                        width: Scaled(7 * 65536),
                        explicit: true,
                    })
                    .unwrap();
                e.document.nodes.append(&mut contents, n);
                let b = if vertical {
                    e.vpackage(contents, PackSpec::Natural, Scaled::MAX)
                        .unwrap()
                } else {
                    e.hpack(contents, PackSpec::Natural).unwrap()
                };
                let mut a = Atom::new(Class::Op, Field::Box(b));
                a.limits = limits;
                a.fields.sub = Field::SubMath(MathList::default());
                a.fields.sup = Field::SubMath(MathList::default());
                let mut l = MathList::default();
                add(e, m, &mut l, a);
                l
            });
        }
    }
}
#[test]
#[ignore = "native CM typed-math oracle: set UMBER_PDFTEX and UMBER_TFM_DIR"]
fn upstream_math_real_cm_fonts_and_operator_larger_characters() {
    let oracle = Oracle::new();
    let root = PathBuf::from(
        std::env::var_os("UMBER_TFM_DIR").expect("explicit fonts/tfm/public directory"),
    );
    let names = [
        "cmr10", "cmr7", "cmr5", "cmmi10", "cmmi7", "cmmi5", "cmsy10", "cmsy7", "cmsy5", "cmex10",
    ];
    let data: Vec<_> = names
        .iter()
        .map(|n| std::fs::read(root.join(format!("cm/{n}.tfm"))).unwrap())
        .collect();
    let fonts: Vec<_> = names
        .iter()
        .zip(&data)
        .map(|(n, b)| (n.as_bytes(), b.as_slice()))
        .collect();
    let mut boot = String::from(
        r"\catcode123=1\catcode125=2\catcode36=3\catcode94=7\catcode95=8\thinmuskip=3mu\medmuskip=4mu plus2mu minus4mu\thickmuskip=5mu plus5mu\scriptspace=.5pt ",
    );
    for (selector, name) in ["ra", "rb", "rc", "ia", "ib", "ic", "sa", "sb", "sc", "ex"]
        .iter()
        .zip(names)
    {
        boot.push_str(&format!(r"\font\{selector}={name} "));
    }
    for (size, selectors) in [
        ["ra", "ia", "sa", "ex"],
        ["rb", "ib", "sb", "ex"],
        ["rc", "ic", "sc", "ex"],
    ]
    .into_iter()
    .enumerate()
    {
        for (family, selector) in selectors.into_iter().enumerate() {
            boot.push_str(&format!(
                "\\{}font{family}=\\{selector} ",
                ["text", "script", "scriptscript"][size]
            ));
        }
    }
    for style in [
        Style::DISPLAY,
        Style::TEXT,
        Style::new(4).unwrap(),
        Style::new(6).unwrap(),
    ] {
        for limits in [Limits::Normal, Limits::Side, Limits::AboveBelow] {
            let policy = match limits {
                Limits::Normal => "",
                Limits::Side => r"\nolimits",
                Limits::AboveBelow => r"\limits",
            };
            let body = format!(
                r"\mathchar358 _{{\mathchar49 }}^{{\mathchar376 }}\mathop\mathchar4944 {policy}_{{\mathchar49 }}^{{\mathchar50 }}\mathop\mathchar4946 {policy}_{{\mathchar49 }}^{{\mathchar50 }}"
            );
            oracle.compare_setup(&fonts, &boot, "", &body, style, |e, m| {
                let mut l = MathList::default();
                let mut a = Atom::new(
                    Class::Ord,
                    Field::Character {
                        family: 1,
                        character: b'f',
                    },
                );
                a.fields.sub = character(b'1');
                a.fields.sup = Field::Character {
                    family: 1,
                    character: b'x',
                };
                add(e, m, &mut l, a);
                for c in [80, 82] {
                    let mut a = Atom::new(
                        Class::Op,
                        Field::Character {
                            family: 3,
                            character: c,
                        },
                    );
                    a.limits = limits;
                    a.fields.sub = character(b'1');
                    a.fields.sup = character(b'2');
                    add(e, m, &mut l, a);
                }
                l
            });
        }
    }
}
