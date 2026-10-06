# Umber4

A fresh design for a TeX interpreter in Rust that runs as a native binary or
WebAssembly module, targets performance comparable to native pdfTeX, and supports
incremental compilation through resumable checkpoints.

Start with the [high-level architecture proposal](docs/architecture.md).
Its [subsystem index](docs/architecture.md#11-detailed-subsystem-proposals) links
the deep dives. The [expansion](docs/expansion.md) and
[storage](docs/storage.md) designs are the foundation for the execution path.
The [MVP](docs/mvp.md) implements a small expansion/register interpreter in
[crates/umber](crates/umber/). Layout, shipout, formats, and checkpoints remain
design proposals.

[Native measurements](docs/mvp-results.md) reach pdfTeX-comparable throughput
across eight core workloads. The implementation builds new data in place and
reuses captured token ranges and invocation storage.

```bash
cargo build --release
target/release/umber fixtures/mvp/macros.tex
python3 scripts/oracle.py
python3 benchmarks/mvp.py
cargo build --release --lib --target wasm32-unknown-unknown
```

The oracle and benchmark runners use native `pdftex -ini -etex`, without a
format or fonts. Install pdfTeX or pass `--pdftex /path/to/pdftex`.

Repository guidance is in [AGENTS.md](AGENTS.md). The copied and adapted
[coordinate](.agents/skills/coordinate/SKILL.md) and
[parallel](.agents/skills/parallel/SKILL.md) skills support future issue-driven
implementation. This checkout initially has no GitHub remote.
