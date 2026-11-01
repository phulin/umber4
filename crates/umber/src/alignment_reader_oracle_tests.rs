//! A single-entry reader driver; native halign supplies the independent oracle.
use super::*;

fn body(e: &Engine<'_>, name: &[u8]) -> Vec<Token> {
    let id = e.names.lookup(name).unwrap();
    let meaning = e.resolve(Token::cs(id));
    let owner = e.values.get(meaning.value as u32);
    let range = owner.definition().body;
    (range.start..range.end)
        .map(|i| owner.tokens.get(i))
        .collect()
}
fn drive(definitions: &str, u: &str, v: &str, cell: &str, omit: bool) -> Result<Vec<String>> {
    let source =
        format!(r"{BOOT}{definitions}\def\utemplate{{{u}}}\def\vtemplate{{{v}}}\relax {cell}\cr");
    let mut e = setup(source.as_bytes());
    let u = body(&e, b"utemplate");
    let v = body(&e, b"vtemplate");
    e.begin_alignment_reader()?;
    let ranges = templates(&mut e, &u, &v);
    let first = e.peek_alignment_entry()?;
    e.begin_alignment_entry(first, if omit { None } else { Some(ranges) })?;
    loop {
        let read = e.next_expanded()?;
        if read.meaning.command == cmd::ENDV {
            e.alignment_endv()?;
            break;
        }
        e.execute(read)?;
    }
    Ok(e.outcome()
        .message_bytes()
        .iter()
        .map(|b| String::from_utf8(b.clone()).unwrap())
        .collect())
}

#[test]
#[ignore = "native alignment reader boundaries: set UMBER_PDFTEX"]
fn upstream_alignment_template_order_scanner_timing_and_errors() {
    let dir = std::env::temp_dir().join(format!("umber-alignment-reader-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("texmf.cnf"),
        "TEXINPUTS=.\nshell_escape=f\nmax_print_line=10000\n",
    )
    .unwrap();
    for (definitions, u, v, cell, omit, error) in [
        (
            "",
            r"\message{UMBER:U:END}",
            r"\message{UMBER:V:END}",
            r"\message{UMBER:C:END}",
            false,
            None,
        ),
        (
            "",
            r"\count0=12",
            r"5\relax\message{UMBER:N:\the\count0:END}",
            "34",
            false,
            None,
        ),
        (
            r"\protected\def\cell{\message{UMBER:C:END}}",
            r"\message{UMBER:U:END}",
            r"\message{UMBER:V:END}",
            r"\cell",
            false,
            None,
        ),
        (
            r"\def\amp{&}",
            r"\message{UMBER:U:END}",
            r"\message{UMBER:V:END}",
            r"\immediate\write16{\amp}",
            false,
            None,
        ),
        (
            "",
            r"\message{UMBER:BAD:END}",
            r"\message{UMBER:BAD:END}",
            r"\relax\message{UMBER:OMIT:END}",
            true,
            None,
        ),
        (
            r"\def\grab#1{}",
            r"\grab",
            "",
            "",
            false,
            Some(Error::Outer),
        ),
    ] {
        let actual = drive(definitions, u, v, cell, omit);
        let native_cell = if omit {
            format!(r"\omit{cell}")
        } else {
            cell.to_owned()
        };
        let source = format!(
            r"{BOOT}\hsize=100pt\vsize=100pt{definitions}\halign{{{u}#{v}\cr {native_cell}\cr}}\end"
        );
        std::fs::write(dir.join("probe.tex"), source).unwrap();
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
        if let Some(error) = error {
            assert_eq!(actual, Err(error));
            assert!(!result.status.success());
            assert!(
                log.contains("Forbidden control sequence found while scanning use of"),
                "{log}"
            );
        } else {
            assert!(result.status.success(), "{log}");
            let expected: Vec<_> = log
                .split("UMBER:")
                .skip(1)
                .map(|s| format!("UMBER:{}:END", s.split(":END").next().unwrap()))
                .collect();
            assert_eq!(actual.unwrap(), expected, "{log}");
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
