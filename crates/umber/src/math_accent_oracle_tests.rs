use super::super::{
    accent_tests::accent,
    forms_tests::{chars, kind, metric},
};
use super::*;
fn body(accent_family: u8, accent_char: u8, nucleus: &str, scripts: u8) -> String {
    format!(
        r"\mathaccent{} {nucleus}{}{}\mathchar102 ",
        u16::from(accent_family) * 256 + u16::from(accent_char),
        if scripts & 1 != 0 {
            r"_{\mathchar100 }"
        } else {
            ""
        },
        if scripts & 2 != 0 {
            r"^{\mathchar101 }"
        } else {
            ""
        }
    )
}
fn fields(e: &mut Engine<'_>, m: &mut MathStore, submath: bool, scripts: u8) -> Fields {
    let mut f = Fields::new(if submath {
        Field::SubMath(chars(e, m, b"bc"))
    } else {
        character(b'b')
    });
    if scripts & 1 != 0 {
        f.sub = character(b'd');
    }
    if scripts & 2 != 0 {
        f.sup = character(b'e');
    }
    f
}
fn formula(e: &mut Engine<'_>, m: &mut MathStore, f: Fields, family: u8, c: u8) -> MathList {
    let mut l = kind(e, m, accent(f, family, c));
    add(e, m, &mut l, Atom::new(Class::Ord, character(b'f')));
    l
}
#[test]
#[ignore = "native math accent oracle: set UMBER_PDFTEX"]
fn upstream_math_accents_width_chains_skew_scripts_and_missing_glyphs() {
    let oracle = Oracle::new();
    for style in (0..8).map(|s| Style::new(s).unwrap()) {
        for submath in [false, true] {
            for scripts in 0..4 {
                let tfm = metrics(
                    &[[128, b's', 128, 0]],
                    &[(b'b', 1, 0), (b'a', 2, b'c'), (b'c', 2, b'd')],
                );
                let settings = r"\skewchar\t=115\skewchar\s=115\skewchar\ss=115 ";
                let source = body(
                    0,
                    b'a',
                    if submath {
                        r"{\mathchar98 \mathchar99 }"
                    } else {
                        r"\mathchar98 "
                    },
                    scripts,
                );
                oracle.compare(&tfm, settings, &source, style, |e, m| {
                    let f = fields(e, m, submath, scripts);
                    formula(e, m, f, 0, b'a')
                });
            }
        }
        for c in [0, b'a'] {
            let tfm = metrics(&[], &[]);
            let source = body(0, c, r"\mathchar0 ", 3);
            oracle.compare(&tfm, "", &source, style, |e, m| {
                let mut f = Fields::new(character(0));
                f.sub = character(b'd');
                f.sup = character(b'e');
                formula(e, m, f, 0, c)
            });
        }
        let mut tfm = metrics(&[], &[]);
        tfm[(8 + b'a' as usize) * 4 + 1] = 0;
        oracle.compare(
            &tfm,
            "",
            &body(0, b'a', r"\mathchar98 ", 3),
            style,
            |e, m| {
                let f = fields(e, m, false, 3);
                formula(e, m, f, 0, b'a')
            },
        );
    }
    for scripts in [0, 3] {
        let tfm = metrics(&[], &[]);
        oracle.compare(
            &tfm,
            "",
            &body(0, 0, r"\mathchar98 ", scripts),
            Style::TEXT,
            |e, m| {
                let f = fields(e, m, false, scripts);
                formula(e, m, f, 0, 0)
            },
        );
    }
    // Selection uses metric width; italic correction belongs to char_box and
    // may make the chosen accent wider than the nucleus.
    let mut tfm = metrics(&[], &[(b'c', 2, b'a')]);
    tfm[(8 + b'd' as usize) * 4 + 2] &= 3;
    oracle.compare(
        &tfm,
        "",
        &body(0, b'c', r"\mathchar100 ", 0),
        Style::TEXT,
        |e, m| formula(e, m, Fields::new(character(b'd')), 0, b'c'),
    );
    for vertical in [false, true] {
        let command = if vertical { "vbox" } else { "hbox" };
        let nucleus = format!(r"{{\{command}{{\kern7pt}}}}");
        let tfm = metrics(&[], &[]);
        oracle.compare(
            &tfm,
            "",
            &body(0, b'a', &nucleus, 3),
            Style::TEXT,
            |e, m| {
                let mut list = nodes::List::default();
                let n = e
                    .document
                    .nodes
                    .alloc(nodes::Kind::Kern {
                        width: Scaled(7 * 65536),
                        explicit: true,
                    })
                    .unwrap();
                e.document.nodes.append(&mut list, n);
                let b = if vertical {
                    e.vpackage(list, PackSpec::Natural, Scaled::MAX).unwrap()
                } else {
                    e.hpack(list, PackSpec::Natural).unwrap()
                };
                let mut f = Fields::new(Field::Box(b));
                f.sub = character(b'd');
                f.sup = character(b'e');
                formula(e, m, f, 0, b'a')
            },
        );
    }
    // Accent and nucleus families select different fonts; only the nucleus
    // font's skew character participates. Ligatures are not skew kerns.
    for program in [
        vec![[128, b's', 128, 0]],
        vec![[128, b's', 0, b'c']],
        vec![[129, 0, 0, 1], [128, b's', 128, 0]],
    ] {
        let tfm = metrics(&program, &[(b'b', 1, 0)]);
        for skew in [-1, 115, 256] {
            let settings = format!(r"\textfont1=\s\skewchar\t={skew}\skewchar\s=-1 ");
            oracle.compare(
                &tfm,
                &settings,
                &body(1, b'a', r"\mathchar98 ", 0),
                Style::TEXT,
                |e, m| {
                    let f = fields(e, m, false, 0);
                    formula(e, m, f, 1, b'a')
                },
            );
        }
    }
    // Stop at the first too-wide accent, even if a later variant is narrower.
    let tfm = metrics(&[], &[(b'a', 2, b'd'), (b'd', 2, b'e')]);
    oracle.compare(
        &tfm,
        "",
        &body(0, b'a', r"\mathchar98 ", 0),
        Style::TEXT,
        |e, m| {
            let f = fields(e, m, false, 0);
            formula(e, m, f, 0, b'a')
        },
    );
    // Missing larger glyphs stop the chain without losing the initial accent.
    let tfm = metrics(&[], &[(b'a', 2, 0)]);
    oracle.compare(
        &tfm,
        "",
        &body(0, b'a', r"\mathchar98 ", 0),
        Style::TEXT,
        |e, m| {
            let f = fields(e, m, false, 0);
            formula(e, m, f, 0, b'a')
        },
    );
    // Signed metrics/x-height and empty-but-present scripts exercise clearance,
    // centering, final-width preservation, and the height-repair kern.
    for signed in 0..3 {
        let mut tfm = metrics(&[], &[]);
        if signed == 1 {
            metric(&mut tfm, 0, -104858);
        }
        if signed == 2 {
            metric(&mut tfm, 1, -104858);
            metric(&mut tfm, 2, -104859);
        }
        oracle.compare(
            &tfm,
            r"\fontdimen5\t=-1sp ",
            &body(0, b'a', r"\mathchar98 ", 0),
            Style::TEXT,
            |e, m| {
                let f = fields(e, m, false, 0);
                formula(e, m, f, 0, b'a')
            },
        );
    }
    let tfm = metrics(&[], &[]);
    oracle.compare(
        &tfm,
        "",
        r"\mathaccent97 \mathchar98 _{}^{}\mathchar102 ",
        Style::TEXT,
        |e, m| {
            let mut f = Fields::new(character(b'b'));
            f.sub = Field::SubMath(MathList::default());
            f.sup = Field::SubMath(MathList::default());
            formula(e, m, f, 0, b'a')
        },
    );
}
#[test]
#[ignore = "native CM math accent oracle: set UMBER_PDFTEX and UMBER_TFM_DIR"]
fn upstream_math_accents_real_cm_narrow_wide_and_nested_scripts() {
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
        r"\catcode123=1\catcode125=2\catcode36=3\catcode94=7\catcode95=8\scriptspace=.5pt ",
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
    boot.push_str(r"\skewchar\ia=127\skewchar\ib=127\skewchar\ic=127 ");
    for style in (0..8).map(|s| Style::new(s).unwrap()) {
        for (family, c) in [(0, 94), (0, 22), (0, 126), (1, 126), (3, 98), (3, 101)] {
            for scripts in [0, 3] {
                let source = body(family, c, r"\mathchar358 ", scripts);
                oracle.compare_setup(&fonts, &boot, "", &source, style, |e, m| {
                    let mut f = Fields::new(Field::Character {
                        family: 1,
                        character: b'f',
                    });
                    if scripts != 0 {
                        f.sub = character(b'd');
                        f.sup = character(b'e');
                    }
                    formula(e, m, f, family, c)
                });
            }
        }
    }
    for count in [2, 4, 8] {
        let nucleus = format!(
            "{{{}}}",
            (0..count).map(|_| r"\mathchar97 ").collect::<String>()
        );
        oracle.compare_setup(
            &fonts,
            &boot,
            "",
            &body(3, 98, &nucleus, 0),
            Style::TEXT,
            |e, m| {
                let f = Fields::new(Field::SubMath(chars(e, m, &vec![b'a'; count])));
                formula(e, m, f, 3, 98)
            },
        );
    }
    oracle.compare_setup(&fonts,&boot,"",r"\mathaccent94 {\mathaccent94 \mathchar358 _{\mathchar49 }^{\mathchar50 }}^{\mathchar51 }\mathchar102 ",Style::TEXT,|e,m|{let mut inner=Fields::new(Field::Character{family:1,character:b'f'});inner.sub=character(b'1');inner.sup=character(b'2');let inner=kind(e,m,accent(inner,0,94));let mut outer=Fields::new(Field::SubMath(inner));outer.sup=character(b'3');formula(e,m,outer,0,94)});
}
