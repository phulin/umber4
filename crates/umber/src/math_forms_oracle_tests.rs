use super::super::forms_tests::{chars, delimiter, kind, metric, recipe_font, surround};
use super::*;
fn del_source(d: Delimiter) -> String {
    format!(
        "\\delimiter\"{:X} ",
        (u32::from(d.small_family) << 20)
            | (u32::from(d.small_char) << 12)
            | (u32::from(d.large_family) << 8)
            | u32::from(d.large_char)
    )
}
fn grouped_fraction(
    e: &mut Engine<'_>,
    m: &mut MathStore,
    thickness: RuleThickness,
    left: Delimiter,
    right: Delimiter,
) -> MathList {
    let numerator = chars(e, m, b"ab");
    let denominator = chars(e, m, b"c");
    let f = kind(
        e,
        m,
        Kind::Fraction(Fraction {
            numerator,
            denominator,
            left,
            right,
            thickness,
        }),
    );
    kind(e, m, Kind::Atom(Atom::new(Class::Ord, Field::SubMath(f))))
}
#[test]
#[ignore = "native math forms oracle: set UMBER_PDFTEX"]
fn upstream_math_fractions_decorations_radicals_and_delimiters() {
    let oracle = Oracle::new();
    let tfm = metrics(&[], &[(b'd', 2, b'e')]);
    for code in 0..8 {
        let style = Style::new(code).unwrap();
        for (thickness, command) in [
            (RuleThickness::Default, r"\over"),
            (RuleThickness::Explicit(Scaled::ZERO), r"\atop"),
            (RuleThickness::Explicit(Scaled(1)), r"\above1sp"),
            (RuleThickness::Explicit(Scaled(-65537)), r"\above-65537sp"),
        ] {
            oracle.compare(
                &tfm,
                r"\nulldelimiterspace=1.2pt ",
                &format!(r"{{\mathchar97 \mathchar98 {command} \mathchar99 }}\mathchar100 "),
                style,
                |e, m| {
                    let mut l = grouped_fraction(
                        e,
                        m,
                        thickness,
                        Delimiter::default(),
                        Delimiter::default(),
                    );
                    add(e, m, &mut l, Atom::new(Class::Ord, character(b'd')));
                    l
                },
            );
        }
        for form in 0..3 {
            let prefix = match form {
                0 => r"\overline",
                1 => r"\underline",
                _ => r#"\radical"64000 "#,
            };
            let body = format!(
                r"{prefix}{{\mathchar97 \mathchar98 }}_{{\mathchar99 }}^{{\mathchar100 }}\mathchar101 "
            );
            oracle.compare(&tfm, "", &body, style, |e, m| {
                let mut f = Fields::new(Field::SubMath(chars(e, m, b"ab")));
                f.sub = character(b'c');
                f.sup = character(b'd');
                let k = match form {
                    0 => Kind::Over(f),
                    1 => Kind::Under(f),
                    _ => Kind::Radical {
                        fields: f,
                        delimiter: delimiter(b'd'),
                    },
                };
                let mut l = kind(e, m, k);
                add(e, m, &mut l, Atom::new(Class::Ord, character(b'e')));
                l
            });
        }
        oracle.compare(
            &tfm,
            "",
            r"\vcenter{\kern7pt}_{}^{\mathchar97 }\mathchar98 ",
            style,
            |e, m| {
                let mut content = nodes::List::default();
                let n = e
                    .document
                    .nodes
                    .alloc(nodes::Kind::Kern {
                        width: Scaled(7 * 65536),
                        explicit: true,
                    })
                    .unwrap();
                e.document.nodes.append(&mut content, n);
                let b = e.vpackage(content, PackSpec::Natural, Scaled::MAX).unwrap();
                let mut f = Fields::new(Field::Box(b));
                f.sub = Field::SubMath(MathList::default());
                f.sup = character(b'a');
                let mut l = kind(e, m, Kind::VCenter(f));
                add(e, m, &mut l, Atom::new(Class::Ord, character(b'b')));
                l
            },
        );
        let d = delimiter(b'd');
        let body = format!(
            r"\left{}\mathbin\mathchar97 \mathchar98 \scriptstyle\mathinner\mathchar99 \right{}\mathchar100 ",
            del_source(d),
            del_source(d)
        );
        oracle.compare(
            &tfm,
            r"\delimiterfactor=901\delimitershortfall=5pt ",
            &body,
            style,
            |e, m| {
                let mut inner = kind(e, m, Kind::Atom(Atom::new(Class::Bin, character(b'a'))));
                add(e, m, &mut inner, Atom::new(Class::Ord, character(b'b')));
                m.append(
                    &mut inner,
                    Kind::Style(Style::new(4).unwrap()),
                    &mut e.document.nodes,
                )
                .unwrap();
                add(e, m, &mut inner, Atom::new(Class::Inner, character(b'c')));
                let mut l = surround(e, m, inner, d, d);
                add(e, m, &mut l, Atom::new(Class::Ord, character(b'd')));
                l
            },
        );
    }
}
#[test]
#[ignore = "native extensible delimiter oracle: set UMBER_PDFTEX"]
fn upstream_math_extensible_recipes_search_and_negative_metric_boxes() {
    let oracle = Oracle::new();
    for bits in 0..8 {
        let recipe = [
            if bits & 1 != 0 { b'a' } else { 0 },
            if bits & 2 != 0 { b'c' } else { 0 },
            if bits & 4 != 0 { b'd' } else { 0 },
            b'e',
        ];
        for signed in 0..4 {
            let mut tfm = recipe_font(recipe);
            match signed {
                1 => metric(&mut tfm, 0, -104858),
                2 => metric(&mut tfm, 1, -104858),
                3 => metric(&mut tfm, 2, -700000),
                _ => {}
            }
            let d = delimiter(b'b');
            let body = format!(
                r"\left{}\vbox to40pt{{\kern0pt}}\right{}",
                del_source(d),
                del_source(d)
            );
            oracle.compare(
                &tfm,
                r"\delimiterfactor=901\delimitershortfall=0pt ",
                &body,
                Style::TEXT,
                |e, m| {
                    let b = e
                        .vpackage(
                            nodes::List::default(),
                            PackSpec::To(Scaled(40 * 65536)),
                            Scaled::MAX,
                        )
                        .unwrap();
                    let inner = kind(e, m, Kind::Atom(Atom::new(Class::Ord, Field::Box(b))));
                    surround(e, m, inner, d, d)
                },
            );
        }
    }
    // Nonpositive repeat extent terminates without repetitions, while caps
    // retain their raw signed dimensions rather than hpack's zero clamps.
    for value in [-734003, -209715] {
        let mut tfm = recipe_font([b'a', 0, b'd', b'e']);
        metric(&mut tfm, 0, value);
        let d = delimiter(b'b');
        let body = format!(r"\left{}\mathchar97 \right{}", del_source(d), del_source(d));
        oracle.compare(
            &tfm,
            r"\delimiterfactor=901\delimitershortfall=0pt ",
            &body,
            Style::TEXT,
            |e, m| {
                let inner = chars(e, m, b"a");
                surround(e, m, inner, d, d)
            },
        );
    }
}
#[test]
#[ignore = "native math search oracle: set UMBER_PDFTEX"]
fn upstream_math_delimiter_family_size_search_nulls_and_fraction_delimiters() {
    let oracle = Oracle::new();
    let mut tfm = metrics(&[], &[(b'a', 2, b'b')]);
    // A valid larger-chain target may be absent. The chain stops there, while
    // the separate large specification remains eligible in another family.
    tfm[(8 + b'b' as usize) * 4] = 0;
    for style in [Style::TEXT, Style::new(4).unwrap(), Style::new(6).unwrap()] {
        for height in [1, 3, 5, 8, 12, 30] {
            for d in [
                delimiter(b'a'),
                Delimiter {
                    small_family: 15,
                    small_char: b'a',
                    large_family: 3,
                    large_char: b'c',
                },
                Delimiter {
                    small_char: b'a',
                    large_family: 3,
                    large_char: b'c',
                    ..Delimiter::default()
                },
                Delimiter::default(),
            ] {
                let body = format!(
                    r"\left{}\vbox to{height}pt{{}}\right{}",
                    del_source(d),
                    del_source(d)
                );
                oracle.compare(&tfm,r"\delimiterfactor=901\delimitershortfall=3sp\nulldelimiterspace=1.2pt\textfont0=\s ",&body,style,|e,m|{let b=e.vpackage(nodes::List::default(),PackSpec::To(Scaled(height*65536)),Scaled::MAX).unwrap();let inner=kind(e,m,Kind::Atom(Atom::new(Class::Ord,Field::Box(b))));surround(e,m,inner,d,d)});
            }
        }
    }
    let tfm = recipe_font(*b"acde");
    let d = delimiter(b'b');
    for (thickness, command) in [
        (RuleThickness::Default, r"\overwithdelims"),
        (RuleThickness::Explicit(Scaled::ZERO), r"\atopwithdelims"),
        (RuleThickness::Explicit(Scaled(65537)), r"\abovewithdelims"),
    ] {
        let extra = if matches!(thickness, RuleThickness::Explicit(Scaled(65537))) {
            "65537sp "
        } else {
            ""
        };
        let body = format!(
            r"{{\mathchar97 \mathchar98 {command}{}{}{extra}\mathchar99 }}\mathchar100 ",
            del_source(d),
            del_source(d)
        );
        oracle.compare(
            &tfm,
            r"\nulldelimiterspace=1.2pt ",
            &body,
            Style::TEXT,
            |e, m| {
                let mut l = grouped_fraction(e, m, thickness, d, d);
                add(e, m, &mut l, Atom::new(Class::Ord, character(b'd')));
                l
            },
        );
    }
    // A zero repeat character is required and present, unlike optional zero
    // top/middle/bottom pieces. It is not an absent-piece sentinel.
    let mut tfm = recipe_font([0, 0, 0, 0]);
    tfm[8 * 4] = 1;
    let body = format!(
        r"\left{}\vbox to20pt{{}}\right{}",
        del_source(d),
        del_source(d)
    );
    oracle.compare(
        &tfm,
        r"\delimiterfactor=901\delimitershortfall=0pt ",
        &body,
        Style::TEXT,
        |e, m| {
            let b = e
                .vpackage(
                    nodes::List::default(),
                    PackSpec::To(Scaled(20 * 65536)),
                    Scaled::MAX,
                )
                .unwrap();
            let inner = kind(e, m, Kind::Atom(Atom::new(Class::Ord, Field::Box(b))));
            surround(e, m, inner, d, d)
        },
    );
}
#[test]
#[ignore = "native CM math forms oracle: set UMBER_PDFTEX and UMBER_TFM_DIR"]
fn upstream_math_real_cm_fractions_radicals_and_growing_delimiters() {
    let oracle = Oracle::new();
    let root = PathBuf::from(std::env::var_os("UMBER_TFM_DIR").expect("explicit fonts/tfm/public"));
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
        r"\catcode123=1\catcode125=2\catcode36=3\catcode94=7\catcode95=8\thinmuskip=3mu\medmuskip=4mu plus2mu minus4mu\thickmuskip=5mu plus5mu\scriptspace=.5pt\nulldelimiterspace=1.2pt\delimiterfactor=901\delimitershortfall=5pt ",
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
    let left = Delimiter {
        small_family: 0,
        small_char: b'(',
        large_family: 3,
        large_char: 0,
    };
    let right = Delimiter {
        small_family: 0,
        small_char: b')',
        large_family: 3,
        large_char: 1,
    };
    let root_del = Delimiter {
        small_family: 2,
        small_char: 112,
        large_family: 3,
        large_char: 112,
    };
    for code in 0..8 {
        let style = Style::new(code).unwrap();
        let body = r#"\left\delimiter"028300 {\mathchar97 \mathchar98 \over \radical"270370{\mathchar99 }}\right\delimiter"029301 ^{\mathchar50 }\mathchar100 "#;
        oracle.compare_setup(&fonts, &boot, "", body, style, |e, m| {
            let numerator = chars(e, m, b"ab");
            let radical = kind(
                e,
                m,
                Kind::Radical {
                    fields: Fields::new(character(b'c')),
                    delimiter: root_del,
                },
            );
            let f = kind(
                e,
                m,
                Kind::Fraction(Fraction {
                    numerator,
                    denominator: radical,
                    left: Delimiter::default(),
                    right: Delimiter::default(),
                    thickness: RuleThickness::Default,
                }),
            );
            let inner = kind(e, m, Kind::Atom(Atom::new(Class::Ord, Field::SubMath(f))));
            let l = surround(e, m, inner, left, right);
            let id = l.head;
            if let Kind::Atom(a) = &mut m.node_mut(id).kind {
                a.fields.sup = character(b'2');
            }
            let mut l = l;
            add(e, m, &mut l, Atom::new(Class::Ord, character(b'd')));
            l
        });
    }
    for height in [20, 40, 80] {
        let brace = Delimiter {
            small_family: 2,
            small_char: 102,
            large_family: 3,
            large_char: 8,
        };
        let body = format!(r#"\left\delimiter"266308 \vbox to{height}pt{{}}\right. "#);
        oracle.compare_setup(&fonts, &boot, "", &body, Style::TEXT, |e, m| {
            let b = e
                .vpackage(
                    nodes::List::default(),
                    PackSpec::To(Scaled(height * 65536)),
                    Scaled::MAX,
                )
                .unwrap();
            let inner = kind(e, m, Kind::Atom(Atom::new(Class::Ord, Field::Box(b))));
            surround(e, m, inner, brace, Delimiter::default())
        });
    }
}

#[test]
#[ignore = "native math chain/rounding oracle: set UMBER_PDFTEX"]
fn upstream_math_nonmonotone_delimiter_chain_and_divide_before_multiply() {
    let oracle = Oracle::new();
    let mut tfm = metrics(&[], &[(b'a', 2, b'b'), (b'b', 2, b'c')]);
    // Insert a third height entry; b has zero extent, c is taller than a.
    let word = |n: usize| usize::from(u16::from_be_bytes([tfm[2 * n], tfm[2 * n + 1]]));
    let height_end = 6 + word(1) + (word(3) + 1 - word(2)) + word(4) + word(5);
    let length = word(0) + 1;
    tfm[..2].copy_from_slice(&(length as u16).to_be_bytes());
    tfm[10..12].copy_from_slice(&3u16.to_be_bytes());
    tfm.splice(height_end * 4..height_end * 4, 1153434i32.to_be_bytes());
    tfm[(8 + b'b' as usize) * 4 + 1] = 0;
    tfm[(8 + b'c' as usize) * 4 + 1] = 0x21;
    let d = delimiter(b'a');
    // axis=156250sp. At 327500sp from the axis, the /500 quotient
    // changes from654 to655: target crosses the first glyph's extent.
    for height in [483749, 483750, 483751, 800000] {
        let body = format!(r"\left{}\vbox to{height}sp{{}}\right.", del_source(d));
        oracle.compare(
            &tfm,
            r"\delimiterfactor=901\delimitershortfall=5pt ",
            &body,
            Style::TEXT,
            |e, m| {
                let b = e
                    .vpackage(
                        nodes::List::default(),
                        PackSpec::To(Scaled(height)),
                        Scaled::MAX,
                    )
                    .unwrap();
                let inner = kind(e, m, Kind::Atom(Atom::new(Class::Ord, Field::Box(b))));
                surround(e, m, inner, d, Delimiter::default())
            },
        );
    }
}
