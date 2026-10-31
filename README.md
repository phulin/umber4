# Umber4

A fresh design for a TeX interpreter in Rust that runs as a native binary or
WebAssembly module, targets performance comparable to native pdfTeX, and supports
incremental compilation through resumable checkpoints.

Start with the [high-level architecture proposal](docs/architecture.md).
Its [subsystem index](docs/architecture.md#11-detailed-subsystem-proposals) links
the deep dives. The [expansion](docs/expansion.md) and
[storage](docs/storage.md) designs are the foundation for the execution path.
The [MVP](docs/mvp.md) and its [expanded primitive set](docs/expansion-mvp.md)
implement a parsing/expansion interpreter in
[crates/umber](crates/umber/), with [in-memory checkpoint/resume](docs/checkpoints.md)
for unchanged input. The [layout foundation](docs/layout-foundation.md) supplies
scaled dimensions, glue, and versioned node storage. [Fonts and text](docs/fonts-text.md)
load host-supplied TFM metrics and construct horizontal lists.
[Paragraph construction](docs/paragraphs.md) implements TeX line breaking and
packing into an owned contribution list. [Page construction](docs/pages.md)
implements incremental TeX page breaks and publishes immutable box, glyph, and
rule records through `Outcome.pages`, with retained font resources and checkpoint
rollback. Output backends, custom output routines, formats, and restart after
source edits remain design proposals.

[Native measurements](docs/mvp-results.md) compare nine core workloads and
nine broader macro workloads, including selected kernels from Gentle.
The implementation builds new data in place and reuses captured token ranges,
token-register values, and invocation storage.
The [benchmark guide](benchmarks/README.md) also provides an optional native
profile build; its measurements are recorded separately from ordinary release.

```bash
cargo build --release
target/release/umber fixtures/mvp/macros.tex
python3 scripts/oracle.py
python3 benchmarks/mvp.py
python3 benchmarks/expansion.py
cargo build --release --lib --target wasm32-unknown-unknown
```

The oracle and benchmark runners use native `pdftex -ini -etex`, without a
format or fonts. The CLI accepts explicit metric mappings as
`umber FILE.tex --tfm NAME=PATH`; TeX input never opens a font path itself. Install pdfTeX or pass `--pdftex /path/to/pdftex`.

Repository guidance is in [AGENTS.md](AGENTS.md). The copied and adapted
[coordinate](.agents/skills/coordinate/SKILL.md) and
[parallel](.agents/skills/parallel/SKILL.md) skills support future issue-driven
implementation. This checkout initially has no GitHub remote.
