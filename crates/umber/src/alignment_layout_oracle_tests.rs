//! Constructed backend lists versus independently executed native halign sources.
use super::*;
use crate::{numeric::print_scaled, page_dvi_tests, page_oracle_tests::displayed};
use std::{path::PathBuf, process::Command};
fn glue_source(g: Glue) -> String {
    let amount = |value: Scaled, order: GlueOrder| {
        if order == GlueOrder::Normal {
            format!("{}sp", value.0)
        } else {
            let mut number = Vec::new();
            print_scaled(value, |c| number.push(c));
            format!(
                "{}{}",
                String::from_utf8(number).unwrap(),
                std::str::from_utf8(order.suffix()).unwrap()
            )
        }
    };
    format!(
        "{}sp plus{} minus{}",
        g.width.0,
        amount(g.stretch, g.stretch_order),
        amount(g.shrink, g.shrink_order)
    )
}
fn source_items(items: &[Item], vertical: bool) -> String {
    let mut out = String::new();
    for item in items {
        match item {
            Item::Rule(w, h, d) => {
                out.push_str(if vertical { "\\hrule" } else { "\\vrule" });
                for (name, value) in [("width", w), ("height", h), ("depth", d)] {
                    if *value != RUNNING {
                        out.push_str(&format!(" {name}{value}sp"));
                    }
                }
                out.push(' ');
            }
            Item::Kern(width) => out.push_str(&format!("\\kern{width}sp ")),
            Item::Glue(spec) => out.push_str(&format!(
                "\\{}skip{} ",
                if vertical { "v" } else { "h" },
                glue_source(*spec)
            )),
            Item::Glyph(c) => out.push(char::from(*c)),
            Item::Adjustment(items) => {
                out.push_str(&format!("\\vadjust{{{}}}", source_items(items, true)))
            }
        }
    }
    out
}
fn source(fixture: &Fixture) -> String {
    let mut out = String::from(
        r"\catcode123=1\catcode125=2\catcode35=6\catcode38=4\font\f=test\f\baselineskip=12sp\lineskip=1sp\lineskiplimit=0pt\boxmaxdepth=16383pt\tracingoutput=1\showboxbreadth=10000\showboxdepth=10000\hbadness=10000\hfuzz=16383pt\vbadness=10000\vfuzz=16383pt ",
    );
    out.push_str(&format!(
        "\\tabskip={} \\setbox0=\\vbox{{\\halign",
        glue_source(fixture.tabs[0])
    ));
    match fixture.spec {
        PackSpec::Natural => {}
        PackSpec::To(w) => out.push_str(&format!(" to{}sp", w.0)),
        PackSpec::Spread(w) => out.push_str(&format!(" spread{}sp", w.0)),
    }
    out.push('{');
    for (i, skip) in fixture.tabs[1..].iter().enumerate() {
        if i != 0 {
            out.push('&');
        }
        out.push_str(&format!("#\\tabskip={} ", glue_source(*skip)));
    }
    out.push_str("\\cr\n");
    for row in &fixture.rows {
        for (i, entry) in row.entries.iter().enumerate() {
            if i != 0 {
                out.push('&');
            }
            out.push_str("\\omit ");
            out.push_str(&source_items(&entry.items, false));
            for _ in 1..entry.span {
                out.push_str("\\span\\omit ");
            }
        }
        out.push_str("\\cr\n");
        if !row.material.is_empty() {
            out.push_str(&format!(
                "\\noalign{{{}}}\n",
                source_items(&row.material, true)
            ));
        }
    }
    out.push_str("}}\\shipout\\box0\\end");
    out
}
#[test]
#[ignore = "native pure horizontal alignment: set UMBER_PDFTEX"]
fn upstream_alignment_layout_constructed_geometry_and_dvi() {
    let dir = std::env::temp_dir().join(format!(
        "umber-alignment-layout-oracle-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("texmf.cnf"),
        "TEXINPUTS=.\nTFMFONTS=.\nshell_escape=f\nmax_print_line=10000\n",
    )
    .unwrap();
    std::fs::write(dir.join("test.tfm"), crate::font_test_data::basic()).unwrap();
    let binary = PathBuf::from(std::env::var_os("UMBER_PDFTEX").expect("explicit native oracle"));
    for fixture in fixtures() {
        let source = source(&fixture);
        let mut e = engine();
        let list = build(&mut e, &fixture);
        let b = e.vpackage(list, PackSpec::Natural, Scaled::MAX).unwrap();
        e.publish_page(b).unwrap();
        let out = e.outcome();
        std::fs::write(dir.join("probe.tex"), &source).unwrap();
        let result = Command::new(&binary)
            .current_dir(&dir)
            .env("TEXMFCNF", &dir)
            .env("TFMFONTS", &dir)
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
        assert!(result.status.success(), "{source}\n{log}");
        let native: Vec<_> = log
            .lines()
            .filter_map(|line| {
                line.find("\\hbox(")
                    .or_else(|| line.find("\\vbox("))
                    .map(|i| &line[i..])
            })
            .collect();
        let ours: Vec<_> = out
            .pages
            .iter()
            .flat_map(|p| {
                p.boxes()
                    .iter()
                    .map(|b| displayed(b, p.boxes()))
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(native, ours, "{source}\n{log}");
        let native =
            page_dvi_tests::read(&std::fs::read(dir.join("probe.dvi")).unwrap(), &out.pages);
        let ours: Vec<Vec<_>> = out.pages.iter().map(|p| p.placements().collect()).collect();
        assert_eq!(native, ours, "{source}\n{log}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}
