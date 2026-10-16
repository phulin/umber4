//! Compares the whole-file lexer prototypes with the current reader.
//!
//! Every lexer first lexes each case and must return the first lexer's
//! tokens and name count; by default that is the current reader. Timed
//! repetitions then rotate the lexer order. Warm runs reuse the name table
//! from verification, so every lookup hits; cold runs start each repetition
//! with an empty table.
use std::fmt::Write as _;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::Instant;
use umber::lexing::{Lexer, Session, Source, Table, lex};

const USAGE: &str = "usage: lexing [--repeats N] [--lexers A,B,...] [--report FILE] \\
    NAME=TABLE:PATH[,PATH...]...
TABLE is initex, plain, package, or expl3. A directory contributes its .tex,
.sty, .cls, and .ltx files recursively, in sorted order. Lexers default to
every available one: current, scalar-fnv, scalar, avx2-fnv, and avx2.";

struct Case {
    name: String,
    table_name: String,
    table: Table,
    files: Vec<(PathBuf, Source)>,
}

struct Measured {
    bytes: usize,
    tokens: usize,
    lines: usize,
    fallback_lines: usize,
    names: usize,
    /// Multi-letter control words, other control sequences, spaces, and
    /// character tokens.
    mix: [usize; 4],
    /// Nanoseconds per repetition, for each available lexer.
    warm: Vec<Vec<u64>>,
    cold: Vec<Vec<u64>>,
}

fn collect(path: &Path, files: &mut Vec<PathBuf>) {
    if !path.is_dir() {
        files.push(path.to_owned());
        return;
    }
    let mut entries: Vec<_> = std::fs::read_dir(path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    for entry in entries {
        let source = entry
            .extension()
            .is_some_and(|extension| ["tex", "sty", "cls", "ltx"].iter().any(|e| extension == *e));
        if entry.is_dir() || source {
            collect(&entry, files);
        }
    }
}

fn parse_case(argument: &str) -> Case {
    let (name, rest) = argument.split_once('=').expect(USAGE);
    let (table_name, paths) = rest.split_once(':').expect(USAGE);
    let table = match table_name {
        "initex" => Table::initex(),
        "plain" => Table::plain(),
        "package" => Table::package(),
        "expl3" => Table::expl3(),
        _ => panic!("unknown table {table_name}\n{USAGE}"),
    };
    let mut paths_found = Vec::new();
    for path in paths.split(',') {
        collect(Path::new(path), &mut paths_found);
    }
    let files = paths_found
        .into_iter()
        .map(|path| {
            let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            (path, Source::new(&bytes))
        })
        .collect();
    Case {
        name: name.to_owned(),
        table_name: table_name.to_owned(),
        table,
        files,
    }
}

fn time(lexer: Lexer, case: &Case, session: &mut Session, out: &mut Vec<u32>) -> u64 {
    let start = Instant::now();
    for (_, source) in &case.files {
        out.clear();
        lex(lexer, source, &case.table, session, out);
        black_box(&*out);
    }
    start.elapsed().as_nanos() as u64
}

fn measure(case: &Case, lexers: &[Lexer], repeats: usize) -> Measured {
    let mut out = Vec::new();
    let mut expected = Vec::new();
    let mut sessions: Vec<Session> = lexers.iter().map(|&lexer| Session::new(lexer)).collect();
    let (mut tokens, mut lines, mut fallback_lines, mut mix) = (0, 0, 0, [0; 4]);
    let counted = lexers.iter().position(|&lexer| lexer != Lexer::Current);
    for (index, &lexer) in lexers.iter().enumerate() {
        for (file, (path, source)) in case.files.iter().enumerate() {
            out.clear();
            let counts = lex(lexer, source, &case.table, &mut sessions[index], &mut out);
            if index == 0 {
                tokens += out.len();
                for &token in &out {
                    mix[class(token)] += 1;
                }
                expected.push(out.clone());
            } else {
                assert!(
                    out == expected[file],
                    "{} differs on {}",
                    lexer.name(),
                    path.display()
                );
            }
            if Some(index) == counted {
                lines += counts.lines;
                fallback_lines += counts.fallback_lines;
            }
        }
        assert_eq!(
            sessions[index].names(),
            sessions[0].names(),
            "{} names",
            lexer.name()
        );
    }
    let mut warm = vec![Vec::new(); lexers.len()];
    let mut cold = vec![Vec::new(); lexers.len()];
    for repeat in 0..repeats {
        for offset in 0..lexers.len() {
            let index = (repeat + offset) % lexers.len();
            warm[index].push(time(lexers[index], case, &mut sessions[index], &mut out));
        }
        for offset in 0..lexers.len() {
            let index = (repeat + offset) % lexers.len();
            sessions[index].forget_names();
            cold[index].push(time(lexers[index], case, &mut sessions[index], &mut out));
        }
    }
    Measured {
        bytes: case.files.iter().map(|(_, source)| source.len()).sum(),
        tokens,
        lines,
        fallback_lines,
        names: sessions[0].names(),
        mix,
        warm,
        cold,
    }
}

/// Token classes for the mix: see `Measured::mix`.
fn class(token: u32) -> usize {
    const CS: u32 = 1 << 24;
    const NAMED: u32 = 2 + 256 + 256;
    match token {
        u32::MAX => 1,
        _ if token >= CS + NAMED => 0,
        _ if token >= CS => 1,
        0x0A20 => 2,
        _ => 3,
    }
}

fn median(samples: &[u64]) -> u64 {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
    })
}

fn processor() -> String {
    std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|info| {
            let line = info.lines().find(|line| line.starts_with("model name"))?;
            Some(line.split_once(':')?.1.trim().to_owned())
        })
        .unwrap_or_else(|| "unknown".to_owned())
}

fn samples(text: &mut String, lexers: &[Lexer], runs: &[Vec<u64>]) {
    let entries: Vec<String> = lexers
        .iter()
        .zip(runs)
        .map(|(lexer, runs)| {
            let values: Vec<String> = runs.iter().map(u64::to_string).collect();
            format!("\"{}\": [{}]", lexer.name(), values.join(", "))
        })
        .collect();
    text.push_str(&entries.join(", "));
}

fn report(path: &str, repeats: usize, lexers: &[Lexer], results: &[(Case, Measured)]) {
    let executable = std::env::current_exe().unwrap();
    let mut text = String::new();
    write!(
        text,
        "{{\n  \"processor\": \"{}\",\n  \"executable\": {{\"path\": \"{}\", \"fnv1a64\": \
         \"{:016x}\"}},\n  \"repeats\": {repeats},\n",
        processor(),
        executable.display(),
        fnv(&std::fs::read(&executable).unwrap()),
    )
    .unwrap();
    text.push_str(
        "  \"unit\": \"nanoseconds per repetition over all files of a case\",\n  \"cases\": [\n",
    );
    for (number, (case, measured)) in results.iter().enumerate() {
        let files: Vec<String> = case
            .files
            .iter()
            .map(|(path, source)| {
                let bytes = std::fs::read(path).unwrap();
                format!(
                    "{{\"path\": \"{}\", \"bytes\": {}, \"fnv1a64\": \"{:016x}\"}}",
                    path.display(),
                    source.len(),
                    fnv(&bytes)
                )
            })
            .collect();
        write!(
            text,
            "    {{\"name\": \"{}\", \"table\": \"{}\", \"bytes\": {}, \"tokens\": {}, \"lines\": {}, \
             \"fallback_lines\": {}, \"names\": {}, \"mix\": {{\"named\": {}, \"symbols\": {}, \
             \"spaces\": {}, \"characters\": {}}},\n     \"files\": [{}],\n     \"warm\": {{",
            case.name,
            case.table_name,
            measured.bytes,
            measured.tokens,
            measured.lines,
            measured.fallback_lines,
            measured.names,
            measured.mix[0],
            measured.mix[1],
            measured.mix[2],
            measured.mix[3],
            files.join(", "),
        )
        .unwrap();
        samples(&mut text, lexers, &measured.warm);
        text.push_str("},\n     \"cold\": {");
        samples(&mut text, lexers, &measured.cold);
        text.push_str(if number + 1 == results.len() {
            "}}\n"
        } else {
            "}},\n"
        });
    }
    text.push_str("  ]\n}\n");
    std::fs::write(path, text).unwrap_or_else(|e| panic!("{path}: {e}"));
}

fn main() {
    let mut arguments = std::env::args().skip(1);
    let (mut repeats, mut report_path, mut cases) = (11, None, Vec::new());
    let mut lexers: Vec<Lexer> = Lexer::ALL
        .into_iter()
        .filter(|lexer| lexer.available())
        .collect();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--repeats" => repeats = arguments.next().and_then(|n| n.parse().ok()).expect(USAGE),
            "--report" => report_path = Some(arguments.next().expect(USAGE)),
            "--lexers" => {
                let names = arguments.next().expect(USAGE);
                lexers = names
                    .split(',')
                    .map(|name| {
                        let lexer = Lexer::ALL.into_iter().find(|lexer| lexer.name() == name);
                        lexer.unwrap_or_else(|| panic!("unknown lexer {name}\n{USAGE}"))
                    })
                    .collect();
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return;
            }
            _ => cases.push(parse_case(&argument)),
        }
    }
    if cases.is_empty() {
        eprintln!("{USAGE}");
        std::process::exit(2);
    }
    println!(
        "{} repetitions on {}; medians, speedup against {}",
        repeats,
        processor(),
        lexers[0].name()
    );
    println!(
        "{:<10} {:>9} {:>9} {:<10} {:>9} {:>8} {:>7} {:>8} {:>8}",
        "case", "KiB", "tokens", "lexer", "warm ms", "MB/s", "ns/tok", "speedup", "cold x"
    );
    let mut results = Vec::new();
    for case in cases {
        let measured = measure(&case, &lexers, repeats);
        let current = (median(&measured.warm[0]), median(&measured.cold[0]));
        for (index, lexer) in lexers.iter().enumerate() {
            let (warm, cold) = (median(&measured.warm[index]), median(&measured.cold[index]));
            println!(
                "{:<10} {:>9} {:>9} {:<10} {:>9.3} {:>8.0} {:>7.2} {:>8.2} {:>8.2}",
                if index == 0 { case.name.as_str() } else { "" },
                if index == 0 {
                    (measured.bytes / 1024).to_string()
                } else {
                    String::new()
                },
                if index == 0 {
                    measured.tokens.to_string()
                } else {
                    String::new()
                },
                lexer.name(),
                warm as f64 / 1e6,
                measured.bytes as f64 * 1e3 / warm as f64,
                warm as f64 / measured.tokens as f64,
                current.0 as f64 / warm as f64,
                current.1 as f64 / cold as f64,
            );
        }
        let share = |count: usize| 100.0 * count as f64 / measured.tokens as f64;
        println!(
            "{:<10} {} lines, {} prepared-line fallbacks, {} names; tokens are {:.1}% \
             multi-letter control words, {:.1}% other control sequences, {:.1}% spaces, \
             {:.1}% characters",
            "",
            measured.lines,
            measured.fallback_lines,
            measured.names,
            share(measured.mix[0]),
            share(measured.mix[1]),
            share(measured.mix[2]),
            share(measured.mix[3]),
        );
        results.push((case, measured));
    }
    if let Some(path) = report_path {
        report(&path, repeats, &lexers, &results);
    }
}
