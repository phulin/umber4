use super::*;
use crate::nodes::KernKind;
use crate::{nodes::ReleaseScratch, numeric::Glue};
#[path = "packing_totals_tests.rs"]
mod totals;

#[derive(Clone, Copy)]
enum Item {
    Rule(i32, i32, i32),
    Box(i32, i32, i32, i32),
    Kern(i32),
    Glue(Glue),
}
#[derive(Debug, PartialEq)]
struct Geometry {
    width: i32,
    height: i32,
    depth: i32,
    glue: GlueSet,
}
struct Case {
    name: &'static str,
    vertical: bool,
    spec: PackSpec,
    max_depth: i32,
    items: Vec<Item>,
    expected: Geometry,
}
fn glue(width: i32, stretch: i32, shrink: i32) -> Item {
    Item::Glue(Glue {
        width: Scaled(width),
        stretch: Scaled(stretch),
        shrink: Scaled(shrink),
        ..Glue::default()
    })
}
fn cases() -> Vec<Case> {
    use GlueOrder::{Fill, Filll, Normal};
    use GlueSign::{Normal as NoSet, Shrinking, Stretching};
    use Item::{Box as B, Kern as K, Rule as R};
    use PackSpec::{Natural, Spread, To};
    let mut cases = Vec::new();
    let mut add =
        |name, vertical, spec, max_depth, items, (width, height, depth), sign, order, ratio| {
            cases.push(Case {
                name,
                vertical,
                spec,
                max_depth,
                items,
                expected: Geometry {
                    width,
                    height,
                    depth,
                    glue: GlueSet { sign, order, ratio },
                },
            });
        };
    add(
        "empty natural hbox",
        false,
        Natural,
        0,
        vec![],
        (0, 0, 0),
        NoSet,
        Normal,
        0.0,
    );
    add(
        "empty exact hbox",
        false,
        To(Scaled(-7)),
        0,
        vec![],
        (-7, 0, 0),
        NoSet,
        Normal,
        0.0,
    );
    add(
        "empty spread hbox",
        false,
        Spread(Scaled(-7)),
        0,
        vec![],
        (-7, 0, 0),
        NoSet,
        Normal,
        0.0,
    );
    add(
        "negative natural width",
        false,
        Natural,
        0,
        vec![K(-10), R(2, -3, -4)],
        (-8, 0, 0),
        NoSet,
        Normal,
        0.0,
    );
    let h = vec![R(10, 4, 2), K(-3), glue(5, 4, 3)];
    add(
        "natural hbox",
        false,
        Natural,
        0,
        h.clone(),
        (12, 4, 2),
        NoSet,
        Normal,
        0.0,
    );
    add(
        "exact stretch hbox",
        false,
        To(Scaled(20)),
        0,
        h.clone(),
        (20, 4, 2),
        Stretching,
        Normal,
        2.0,
    );
    add(
        "spread stretch hbox",
        false,
        Spread(Scaled(8)),
        0,
        h.clone(),
        (20, 4, 2),
        Stretching,
        Normal,
        2.0,
    );
    add(
        "exact shrink hbox",
        false,
        To(Scaled(10)),
        0,
        h.clone(),
        (10, 4, 2),
        Shrinking,
        Normal,
        2.0 / 3.0,
    );
    add(
        "overfull spread hbox",
        false,
        Spread(Scaled(-6)),
        0,
        h.clone(),
        (6, 4, 2),
        Shrinking,
        Normal,
        1.0,
    );
    add(
        "negative exact hbox",
        false,
        To(Scaled(-5)),
        0,
        h,
        (-5, 4, 2),
        Shrinking,
        Normal,
        1.0,
    );
    let orders = vec![
        R(10, 4, 2),
        Item::Glue(Glue {
            width: Scaled(2),
            stretch: Scaled(2),
            stretch_order: GlueOrder::Fil,
            shrink: Scaled(3),
            shrink_order: Fill,
        }),
        Item::Glue(Glue {
            stretch: Scaled(4),
            stretch_order: Filll,
            ..Glue::default()
        }),
    ];
    add(
        "highest stretch order",
        false,
        Spread(Scaled(8)),
        0,
        orders.clone(),
        (20, 4, 2),
        Stretching,
        Filll,
        2.0,
    );
    add(
        "infinite shrink is not capped",
        false,
        Spread(Scaled(-9)),
        0,
        orders,
        (3, 4, 2),
        Shrinking,
        Fill,
        3.0,
    );
    add(
        "negative stretch component",
        false,
        Spread(Scaled(8)),
        0,
        vec![R(10, 4, 2), glue(0, -4, -2)],
        (18, 4, 2),
        Stretching,
        Normal,
        -2.0,
    );
    add(
        "shifted horizontal children",
        false,
        Natural,
        0,
        vec![B(12, 5, 2, 3), B(7, 4, 6, -2)],
        (19, 6, 5),
        NoSet,
        Normal,
        0.0,
    );
    add(
        "empty natural vbox",
        true,
        Natural,
        2,
        vec![],
        (0, 0, 0),
        NoSet,
        Normal,
        0.0,
    );
    add(
        "empty spread vbox",
        true,
        Spread(Scaled(-7)),
        2,
        vec![],
        (0, -7, 0),
        NoSet,
        Normal,
        0.0,
    );
    add(
        "empty negative depth limit",
        true,
        Natural,
        -3,
        vec![],
        (0, 3, -3),
        NoSet,
        Normal,
        0.0,
    );
    add(
        "depth limited natural vbox",
        true,
        Natural,
        2,
        vec![R(8, 10, 7)],
        (8, 15, 2),
        NoSet,
        Normal,
        0.0,
    );
    add(
        "depth limited exact vbox",
        true,
        To(Scaled(20)),
        2,
        vec![R(8, 10, 7)],
        (8, 20, 2),
        NoSet,
        Normal,
        0.0,
    );
    add(
        "depth limited spread vbox",
        true,
        Spread(Scaled(-5)),
        2,
        vec![R(8, 10, 7)],
        (8, 10, 2),
        NoSet,
        Normal,
        1.0,
    );
    add(
        "uncapped final depth",
        true,
        Natural,
        Scaled::MAX.0,
        vec![R(8, 10, 7)],
        (8, 10, 7),
        NoSet,
        Normal,
        0.0,
    );
    add(
        "negative final depth limit",
        true,
        Natural,
        -3,
        vec![R(8, 10, 7)],
        (8, 20, -3),
        NoSet,
        Normal,
        0.0,
    );
    add(
        "negative natural height",
        true,
        Natural,
        2,
        vec![K(-10), R(5, -4, -2)],
        (5, -14, -2),
        NoSet,
        Normal,
        0.0,
    );
    let v = vec![B(10, 8, 6, 3), K(-4), glue(2, 4, 3), R(9, 5, 7)];
    add(
        "natural depth transitions",
        true,
        Natural,
        2,
        v.clone(),
        (13, 22, 2),
        NoSet,
        Normal,
        0.0,
    );
    add(
        "exact vertical shrink",
        true,
        To(Scaled(20)),
        2,
        v.clone(),
        (13, 20, 2),
        Shrinking,
        Normal,
        2.0 / 3.0,
    );
    add(
        "spread vertical stretch",
        true,
        Spread(Scaled(8)),
        2,
        v.clone(),
        (13, 30, 2),
        Stretching,
        Normal,
        2.0,
    );
    add(
        "overfull vertical spread",
        true,
        Spread(Scaled(-6)),
        2,
        v,
        (13, 16, 2),
        Shrinking,
        Normal,
        1.0,
    );
    add(
        "trailing glue consumes depth",
        true,
        Natural,
        2,
        vec![R(9, 10, 7), glue(2, 4, 3)],
        (9, 19, 0),
        NoSet,
        Normal,
        0.0,
    );
    add(
        "negative shifts do not widen vbox",
        true,
        Natural,
        2,
        vec![B(10, 8, 6, -13), B(-4, 2, 3, 1)],
        (0, 17, 2),
        NoSet,
        Normal,
        0.0,
    );
    cases
}
fn pack(case: &Case) -> Geometry {
    let mut e = Engine::new(b"");
    let mut list = List::default();
    for item in &case.items {
        let node = match *item {
            Item::Rule(w, h, d) => e.document.nodes.alloc(Kind::Rule {
                width: Scaled(w),
                height: Scaled(h),
                depth: Scaled(d),
            }),
            Item::Box(w, h, d, shift) => e.document.nodes.alloc_box(BoxData {
                width: Scaled(w),
                height: Scaled(h),
                depth: Scaled(d),
                shift: Scaled(shift),
                ..BoxData::default()
            }),
            Item::Kern(w) => e.document.nodes.alloc(Kind::Kern {
                width: Scaled(w),
                kind: KernKind::Explicit,
            }),
            Item::Glue(spec) => e.document.nodes.alloc(Kind::Glue {
                spec,
                subtype: GlueKind::Normal,
            }),
        }
        .unwrap();
        e.document.nodes.append(&mut list, node);
    }
    let head = list.head();
    let node = if case.vertical {
        e.vpackage(list, case.spec, Scaled(case.max_depth))
    } else {
        e.hpack(list, case.spec)
    }
    .unwrap();
    let Kind::Box(id) = &e.document.nodes.get(node.id()).kind else {
        panic!("packed box")
    };
    let b = e.document.nodes.box_data(id);
    assert_eq!(
        b.list.head(),
        head,
        "packing transfers the existing contents"
    );
    let geometry = Geometry {
        width: b.width.0,
        height: b.height.0,
        depth: b.depth.0,
        glue: b.glue_set,
    };
    e.document
        .nodes
        .release_node(node, &mut ReleaseScratch::default());
    geometry
}
#[test]
fn specifications_preserve_tex_dimensions_glue_orders_and_depth() {
    for case in cases() {
        assert_eq!(pack(&case), case.expected, "{}", case.name);
    }
}

fn scaled(value: i32) -> String {
    let mut bytes = Vec::new();
    crate::numeric::print_scaled(Scaled(value), |c| bytes.push(c));
    String::from_utf8(bytes).unwrap()
}
fn displayed(case: &Case, geometry: &Geometry) -> String {
    let mut line = format!(
        "\\{}box({}+{})x{}",
        if case.vertical { "v" } else { "h" },
        scaled(geometry.height),
        scaled(geometry.depth),
        scaled(geometry.width)
    );
    let g = geometry.glue;
    if g.sign != GlueSign::Normal && g.ratio != 0.0 {
        line.push_str(", glue set ");
        if g.sign == GlueSign::Shrinking {
            line.push_str("- ");
        }
        line.push_str(&scaled((g.ratio * 65536.0).round() as i32));
        line.push_str(["", "fil", "fill", "filll"][g.order as usize]);
    }
    line
}
fn native_box(case: &Case) -> String {
    use std::fmt::Write;
    let mut text = format!(
        "\\boxmaxdepth={}sp\\setbox0=\\{}box",
        case.max_depth,
        if case.vertical { "v" } else { "h" }
    );
    match case.spec {
        PackSpec::Natural => {}
        PackSpec::To(n) => write!(text, " to{}sp", n.0).unwrap(),
        PackSpec::Spread(n) => write!(text, " spread{}sp", n.0).unwrap(),
    }
    text.push('{');
    for item in &case.items {
        match *item {
            Item::Rule(w, h, d) => write!(
                text,
                "\\{}rule width{w}sp height{h}sp depth{d}sp",
                if case.vertical { "h" } else { "v" }
            )
            .unwrap(),
            Item::Box(w, h, d, shift) => {
                if case.vertical {
                    text.push_str("\\prevdepth=-1000pt");
                }
                write!(
                    text,
                    "\\setbox1=\\hbox{{}}\\wd1={w}sp\\ht1={h}sp\\dp1={d}sp\\{} {shift}sp\\box1",
                    if case.vertical { "moveright" } else { "lower" }
                )
                .unwrap();
            }
            Item::Kern(w) => write!(text, "\\kern{w}sp").unwrap(),
            Item::Glue(g) => write!(
                text,
                "\\{}skip{}sp plus{}{} minus{}{}",
                if case.vertical { "v" } else { "h" },
                g.width.0,
                scaled(g.stretch.0),
                std::str::from_utf8(g.stretch_order.suffix()).unwrap(),
                scaled(g.shrink.0),
                std::str::from_utf8(g.shrink_order.suffix()).unwrap()
            )
            .unwrap(),
        }
    }
    text.push_str("}\\shipout\\box0\n");
    text
}
struct Oracle(std::path::PathBuf);
impl Drop for Oracle {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
#[ignore = "native packing oracle: set UMBER_PDFTEX explicitly"]
fn upstream_natural_exact_and_spread_box_geometry() {
    let oracle =
        Oracle(std::env::temp_dir().join(format!("gentle-packing-oracle-{}", std::process::id())));
    std::fs::create_dir_all(&oracle.0).unwrap();
    std::fs::write(
        oracle.0.join("texmf.cnf"),
        b"TEXINPUTS = .\nshell_escape = f\nmax_print_line = 10000\n",
    )
    .unwrap();
    let cases = cases();
    let mut source = String::from(
        "\\catcode123=1\\catcode125=2\\tracingoutput=1\\showboxbreadth=0\\showboxdepth=0\\hbadness=10000\\vbadness=10000\\hfuzz=16383pt\\vfuzz=16383pt\\overfullrule=0pt\n",
    );
    for case in &cases {
        source.push_str(&native_box(case));
    }
    source.push_str("\\message{GENTLE-PACKING-END}\\end\n");
    std::fs::write(oracle.0.join("probe.tex"), source).unwrap();
    let output = std::process::Command::new(
        std::env::var_os("UMBER_PDFTEX").expect("explicit native oracle"),
    )
    .current_dir(&oracle.0)
    .env("TEXMFCNF", &oracle.0)
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
    let log = std::fs::read_to_string(oracle.0.join("probe.log")).unwrap();
    assert!(
        output.status.success() && log.contains("GENTLE-PACKING-END"),
        "{log}"
    );
    let actual: Vec<_> = log
        .lines()
        .filter(|line| line.starts_with("\\hbox(") || line.starts_with("\\vbox("))
        .map(|line| line.strip_suffix(" []").unwrap_or(line))
        .collect();
    let expected: Vec<_> = cases
        .iter()
        .map(|case| displayed(case, &pack(case)))
        .collect();
    assert_eq!(actual, expected);
}
