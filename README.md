# Umber4

A fresh design for a TeX interpreter in Rust that runs as a native binary or
WebAssembly module, targets performance comparable to native pdfTeX, and supports
incremental compilation through resumable checkpoints.

Start with the [high-level architecture proposal](docs/architecture.md).
The project is in design review; no interpreter has been implemented.

Repository guidance is in [AGENTS.md](AGENTS.md). The copied and adapted
[coordinate](.agents/skills/coordinate/SKILL.md) and
[parallel](.agents/skills/parallel/SKILL.md) skills support future issue-driven
implementation. This checkout initially has no GitHub remote.
