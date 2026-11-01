use std::{
    ffi::{OsStr, OsString},
    io::Write,
    path::PathBuf,
};
fn main() {
    match run() {
        Ok(outcome) => {
            let mut output = std::io::stdout().lock();
            for diagnostic in outcome.diagnostics {
                if diagnostic.destination() == umber::DiagnosticDestination::LogOnly {
                    continue;
                }
                output.write_all(diagnostic.bytes()).unwrap();
                output.write_all(b"\n").unwrap();
            }
        }
        Err(error) => {
            eprintln!("umber: {error}");
            std::process::exit(1);
        }
    }
}
struct Entry {
    logical: Vec<u8>,
    name: Vec<u8>,
    bytes: Vec<u8>,
}
struct Inputs(Vec<Entry>);
impl umber::InputResolver for Inputs {
    fn resolve(&self, requested: &[u8], including: &[u8]) -> Option<umber::InputFile<'_>> {
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
                    return Some(umber::InputFile {
                        name: &entry.name,
                        bytes: &entry.bytes,
                    });
                }
            }
        }
        None
    }
}
#[cfg(unix)]
fn bytes(value: &OsStr) -> Result<Vec<u8>, String> {
    use std::os::unix::ffi::OsStrExt;
    Ok(value.as_bytes().to_vec())
}
#[cfg(not(unix))]
fn bytes(value: &OsStr) -> Result<Vec<u8>, String> {
    value
        .to_str()
        .map(|s| s.as_bytes().to_vec())
        .ok_or("path cannot be represented as TeX bytes".into())
}
#[cfg(unix)]
fn os_string(value: Vec<u8>) -> Result<OsString, String> {
    use std::os::unix::ffi::OsStringExt;
    Ok(OsString::from_vec(value))
}
#[cfg(not(unix))]
fn os_string(value: Vec<u8>) -> Result<OsString, String> {
    String::from_utf8(value)
        .map(OsString::from)
        .map_err(|e| e.to_string())
}
fn run() -> Result<umber::Outcome, String> {
    let mut arguments = std::env::args_os().skip(1);
    let path = PathBuf::from(
        arguments
            .next()
            .ok_or("usage: umber FILE.tex [--tfm NAME=PATH | --input NAME=PATH]...")?,
    );
    let source = std::fs::read(&path).map_err(|e| e.to_string())?;
    let name = bytes(
        std::fs::canonicalize(&path)
            .map_err(|e| e.to_string())?
            .as_os_str(),
    )?;
    let mut inputs = Inputs(Vec::new());
    let mut fonts = Vec::new();
    while let Some(option) = arguments.next() {
        if option != "--tfm" && option != "--input" {
            return Err("expected --tfm or --input NAME=PATH".into());
        }
        let mapping = bytes(&arguments.next().ok_or("expected NAME=PATH")?)?;
        let split = mapping
            .iter()
            .position(|&b| b == b'=')
            .ok_or("expected NAME=PATH")?;
        let logical = mapping[..split].to_vec();
        let path = PathBuf::from(os_string(mapping[split + 1..].to_vec())?);
        let data = std::fs::read(&path).map_err(|e| e.to_string())?;
        if option == "--tfm" {
            fonts.push((logical, data));
        } else {
            if inputs.0.iter().any(|entry| entry.logical == logical) {
                return Err("duplicate input name".into());
            }
            let resolved = bytes(
                std::fs::canonicalize(path)
                    .map_err(|e| e.to_string())?
                    .as_os_str(),
            )?;
            inputs.0.push(Entry {
                logical,
                name: resolved,
                bytes: data,
            });
        }
    }
    let mut engine = umber::Engine::with_inputs(&name, &source, &inputs);
    for (name, bytes) in fonts {
        engine
            .register_tfm(&name, &bytes)
            .map_err(|e| e.to_string())?;
    }
    engine.run().map_err(|e| e.to_string())
}
