# Documentation Guidance

Read the root `AGENTS.md` first.

- `architecture.md` is the high-level design proposal. It records goals,
  recommended boundaries, tradeoffs, and unresolved design questions.
- `expansion.md` develops the input and expansion loops, token-source cursors,
  argument capture and lifetime, and their checkpoint behavior.
- `storage.md` proposes stores, allocation primitives, mutable roots, and
  ownership transfers by semantic lifetime.
- `execution.md` develops primitive dispatch, meanings, assignment restoration,
  modes, and paragraph handoff.
- `input.md` develops character scanning, line lookahead and its invalidation,
  name lookup, operand scanners, and retained file cursors.
- `fast-lexing.md` measures a whole-file lexer prototype under fixed category
  codes and explores deferred cross-line lookahead and worker lexing experiments.
- `checkpoints.md` defines quiescent driver handoff, heterogeneous snapshots,
  in-memory restore, and conservative prefix restart after edits.
- `resources.md` develops native/Wasm byte acquisition, shared input buffers,
  generated-file append storage, stream handles, diagnostics, and replay.
- `paragraphs.md` develops font state, character/ligature construction, packing,
  hyphenation, exact line breaking, and pdfTeX spacing extensions.
- `math-alignments.md` develops math atoms/conversion, display math, alignment
  templates, entry measurement, and result-list transfers.
- `pages.md` develops vertical contributions, page breaks, insertions, marks,
  output routines, and retained unfinished pages.
- `output.md` develops ordered shipout, detached pages, backend document state,
  DVI/PDF resources, and direct serialization.
- `formats.md` develops bootstrap, Umber format export/read, shared semantic
  baselines, and document startup.
- `performance.md` develops native/Wasm hot paths, allocation experiments,
  comparable upstream measurements, and compatibility checks.
- `mvp.md` defines the first executable expansion/register slice, supported
  primitives, compatibility fixtures, and same-source performance comparison.
- `mvp-results.md` records the implemented slice, native measurements, allocation
  measurements, and reproduction commands.
- `expansion-mvp.md` specifies the broader parsing/expansion slice and corpus
  benchmark contracts.
- Distinguish requirements, proposed choices, measured results, and open questions.
  Do not describe planned facilities as implemented.
- Keep detailed subsystem contracts in focused documents when those designs are
  developed; link them from the architecture and update this map.
- Cite primary sources for compatibility and platform facts. Record oracle
  versions and corpus identities when reporting implementation measurements.
- Use GitHub Issues for execution tracking once the repository has a remote;
  design documents are not task queues or session memory.
