use std::io::Write;
fn main() {
    let Some(path) = std::env::args_os().nth(1) else {
        eprintln!("usage: umber FILE.tex");
        std::process::exit(2);
    };
    let result = std::fs::read(path)
        .map_err(|e| e.to_string())
        .and_then(|bytes| {
            let engine = umber::Engine::new(&bytes);
            #[cfg(feature = "producer")]
            let result = engine.run_producer();
            #[cfg(not(feature = "producer"))]
            let result = engine.run();
            result.map_err(|e| e.to_string())
        });
    match result {
        Ok(outcome) => {
            let mut output = std::io::stdout().lock();
            for message in outcome.messages {
                output.write_all(&message).unwrap();
                output.write_all(b"\n").unwrap();
            }
        }
        Err(error) => {
            eprintln!("umber: {error}");
            std::process::exit(1);
        }
    }
}
