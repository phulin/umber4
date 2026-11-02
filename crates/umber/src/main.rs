use std::{
    ffi::{OsStr, OsString},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
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
    let first=arguments.next().ok_or("usage: umber FILE.tex [--format FILE] [--tfm NAME=PATH | --input NAME=PATH] [--calendar M,D,M,Y]; umber build-format SETUP.tex -o FILE")?;
    let build = first == "build-format";
    let path = PathBuf::from(if build {
        arguments
            .next()
            .ok_or("expected setup source after build-format")?
    } else {
        first
    });
    let source = std::fs::read(&path).map_err(|e| e.to_string())?;
    let name = bytes(
        std::fs::canonicalize(&path)
            .map_err(|e| e.to_string())?
            .as_os_str(),
    )?;
    let mut inputs = Inputs(Vec::new());
    let mut fonts = Vec::new();
    let mut format = None;
    let mut output = None;
    let mut calendar = None;
    while let Some(option) = arguments.next() {
        let value = arguments.next().ok_or("missing option value")?;
        if option == "--format" {
            if build || format.is_some() {
                return Err("--format is valid once for a document run".into());
            }
            format = Some(PathBuf::from(value));
            continue;
        }
        if option == "-o" {
            if !build || output.is_some() {
                return Err("-o is required once for build-format".into());
            }
            output = Some(PathBuf::from(value));
            continue;
        }
        if option == "--calendar" {
            if calendar.is_some() {
                return Err("duplicate --calendar".into());
            }
            let fields = value
                .to_str()
                .ok_or("calendar must be MINUTES,DAY,MONTH,YEAR")?
                .split(',')
                .map(str::parse::<i32>)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            calendar = Some(
                <[i32; 4]>::try_from(fields)
                    .map_err(|_| "calendar must be MINUTES,DAY,MONTH,YEAR")?,
            );
            continue;
        }
        if option != "--tfm" && option != "--input" {
            return Err("expected --format, --calendar, --tfm or --input".into());
        }
        let mapping = bytes(&value)?;
        let split = mapping
            .iter()
            .position(|&b| b == b'=')
            .ok_or("expected NAME=PATH")?;
        let logical = mapping[..split].to_vec();
        if logical.is_empty() || split + 1 == mapping.len() {
            return Err("expected nonempty NAME=PATH".into());
        }
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
    if build && output.is_none() {
        return Err("build-format requires -o FILE".into());
    }
    let mut engine = if let Some(path) = format {
        let image = std::fs::read(path).map_err(|e| e.to_string())?;
        let extra: Vec<_> = fonts
            .iter()
            .map(|(name, bytes)| (name.as_slice(), bytes.as_slice()))
            .collect();
        umber::Engine::with_format_inputs(
            &image,
            umber::InputFile {
                name: &name,
                bytes: &source,
            },
            Some(&inputs),
            &extra,
        )
        .map_err(|e| e.to_string())?
        // image and extra directory are released before the first command.
    } else {
        let mut engine = umber::Engine::with_inputs(&name, &source, &inputs);
        for (name, bytes) in &fonts {
            engine
                .register_tfm(name, bytes)
                .map_err(|e| e.to_string())?;
        }
        engine
    };
    drop(fonts);
    if let Some([minutes, day, month, year]) = calendar {
        engine.set_calendar(minutes, day, month, year);
    }
    if let Some(path) = output {
        engine.advance(usize::MAX).map_err(|e| e.to_string())?;
        let image = engine.encode_format().map_err(|e| e.to_string())?;
        write_format(&path, &image).map_err(|e| e.to_string())?;
        return Ok(engine.outcome());
    }
    engine.run().map_err(|e| e.to_string())
}

fn write_format(path: &Path, image: &[u8]) -> std::io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let mut temporary = path.as_os_str().to_os_string();
    temporary.push(format!(
        ".tmp-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let temporary = PathBuf::from(temporary);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| {
        file.write_all(image)?;
        drop(file);
        std::fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
