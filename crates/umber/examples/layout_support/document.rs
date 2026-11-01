//! Full-document inspection through immutable host bundles and public output.
use std::{error::Error, io, io::Write, path::Path};
use umber::{Engine, InputFile, InputResolver, Placement, Progress};

struct Entry {
    logical: Vec<u8>,
    name: Vec<u8>,
    bytes: Vec<u8>,
}
struct Inputs(Vec<Entry>);
impl InputResolver for Inputs {
    fn resolve(&self, requested: &[u8], including: &[u8]) -> Option<InputFile<'_>> {
        let prefix = including
            .iter()
            .rposition(|&b| b == b'/')
            .map_or(&b""[..], |i| &including[..=i]);
        let missing_extension = !requested.rsplit(|&b| b == b'/').next()?.contains(&b'.');
        for extension in [&b""[..], &b".tex"[..]] {
            if !extension.is_empty() && !missing_extension {
                continue;
            }
            for entry in &self.0 {
                let matches = |candidate: &[u8], area: &[u8]| {
                    candidate.len() == area.len() + requested.len() + extension.len()
                        && candidate.starts_with(area)
                        && candidate[area.len()..].starts_with(requested)
                        && candidate.ends_with(extension)
                };
                if matches(&entry.logical, b"")
                    || matches(&entry.name, b"")
                    || matches(&entry.name, prefix)
                {
                    return Some(InputFile {
                        name: &entry.name,
                        bytes: &entry.bytes,
                    });
                }
            }
        }
        None
    }
}
fn entry(argument: String) -> Result<Entry, Box<dyn Error>> {
    let (logical, path) = argument
        .split_once('=')
        .filter(|(name, path)| !name.is_empty() && !path.is_empty())
        .ok_or_else(|| io::Error::other("expected NAME=PATH"))?;
    Ok(Entry {
        logical: logical.as_bytes().to_vec(),
        name: std::fs::canonicalize(path)?
            .as_os_str()
            .as_encoded_bytes()
            .to_vec(),
        bytes: std::fs::read(path)?,
    })
}
pub(super) fn run(mut args: impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    let wrapper = args.next().ok_or_else(|| {
        io::Error::other("expected WRAPPER [--input NAME=PATH] [--tfm NAME=PATH]")
    })?;
    let wrapper = std::fs::canonicalize(Path::new(&wrapper))?;
    let source = std::fs::read(&wrapper)?;
    let mut inputs = Inputs(Vec::new());
    let mut fonts = Vec::new();
    let mut calendar = None;
    while let Some(option) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| io::Error::other(format!("missing value for {option}")))?;
        match option.as_str() {
            "--input" => {
                let input = entry(value)?;
                if inputs.0.iter().any(|entry| entry.logical == input.logical) {
                    return Err(io::Error::other("duplicate input name").into());
                }
                inputs.0.push(input);
            }
            "--tfm" => fonts.push(entry(value)?),
            "--calendar" => {
                let fields = value
                    .split(',')
                    .map(str::parse::<i32>)
                    .collect::<Result<Vec<_>, _>>()?;
                calendar =
                    Some(<[i32; 4]>::try_from(fields).map_err(|_| {
                        io::Error::other("calendar must be MINUTES,DAY,MONTH,YEAR")
                    })?);
            }
            _ => return Err(io::Error::other(format!("unknown option {option}")).into()),
        }
    }
    let mut engine = Engine::with_inputs(wrapper.as_os_str().as_encoded_bytes(), &source, &inputs);
    for font in &fonts {
        engine.register_tfm(&font.logical, &font.bytes)?;
    }
    if let Some([minutes, day, month, year]) = calendar {
        engine.set_calendar(minutes, day, month, year);
    }
    if engine.advance(usize::MAX)? != Progress::Complete {
        return Err(io::Error::other("document did not complete").into());
    }
    let outcome = engine.outcome();
    let mut out = io::BufWriter::new(io::stdout().lock());
    write!(out, "{{\"pages\":[")?;
    for (index, page) in outcome.pages.iter().enumerate() {
        if index != 0 {
            write!(out, ",")?;
        }
        write!(out, "{{\"counts\":{:?},\"fonts\":[", page.counts)?;
        for (index, font) in page.fonts().iter().enumerate() {
            if index != 0 {
                write!(out, ",")?;
            }
            write!(
                out,
                "{{\"name\":{:?},\"size\":{}}}",
                font.name(),
                font.size()
            )?;
        }
        write!(out, "],\"events\":[")?;
        for (index, event) in page.placements().enumerate() {
            if index != 0 {
                write!(out, ",")?;
            }
            match event {
                Placement::Glyph {
                    font,
                    character,
                    x,
                    y,
                } => {
                    write!(out, "[\"g\",{font},{character},{x},{y}]")?;
                }
                Placement::Rule {
                    x,
                    y,
                    width,
                    height,
                } => {
                    write!(out, "[\"r\",{x},{y},{width},{height}]")?;
                }
            }
        }
        write!(out, "]}}")?;
    }
    writeln!(out, "]}}")?;
    out.flush()?;
    Ok(())
}
