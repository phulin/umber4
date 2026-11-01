use std::io::Write;
fn main() {
    let result = run();
    match result {
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

fn run() -> Result<umber::Outcome, String> {
    let mut arguments = std::env::args_os().skip(1);
    let path = arguments
        .next()
        .ok_or("usage: umber FILE.tex [--tfm NAME=PATH]...")?;
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut engine = umber::Engine::new(&bytes);
    while let Some(option) = arguments.next() {
        if option != "--tfm" {
            return Err("expected --tfm NAME=PATH".into());
        }
        let mapping = arguments.next().ok_or("expected NAME=PATH after --tfm")?;
        let mapping = mapping.to_str().ok_or("font mapping must be UTF-8")?;
        let (name, path) = mapping
            .split_once('=')
            .ok_or("expected NAME=PATH after --tfm")?;
        let metrics = std::fs::read(path).map_err(|e| e.to_string())?;
        engine
            .register_tfm(name.as_bytes(), &metrics)
            .map_err(|e| e.to_string())?;
    }
    engine.run().map_err(|e| e.to_string())
}
