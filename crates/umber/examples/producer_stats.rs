#[cfg(feature = "producer")]
fn main() {
    let path = std::env::args_os()
        .nth(1)
        .expect("usage: producer_stats FILE.tex");
    let bytes = std::fs::read(path).expect("read source");
    let (outcome, stats) = umber::Engine::new(&bytes)
        .run_producer_diagnostic()
        .expect("execute source");
    for message in outcome.messages {
        println!("{}", String::from_utf8_lossy(&message));
    }
    println!("{stats:?}");
}
#[cfg(not(feature = "producer"))]
fn main() {
    panic!("build with --features producer");
}
