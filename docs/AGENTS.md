# Documentation Guidance

Read the root `AGENTS.md` first.

Describe current contracts, capabilities, proposals, and methodology in `docs/`.
Keep implementation histories, before/after reports, and validation logs out of
`docs/`; retained benchmark reports belong in `benchmarks/results/`.

- `architecture.md` is the high-level design proposal. It records goals,
  recommended boundaries, tradeoffs, and unresolved design questions.
- `expansion.md` develops the input and expansion loops, token-source cursors,
  argument capture and lifetime, and their checkpoint behavior.
- `storage.md` proposes stores, allocation primitives, mutable roots, and
  ownership transfers by semantic lifetime.
- `layout-foundation.md` contracts scaled arithmetic, typed dimension/glue
  bindings, and versioned node/list ownership before text layout execution.
- `execution.md` develops primitive dispatch, meanings, assignment restoration,
  modes, and paragraph handoff.
- `input.md` develops lazy character scanning, bounded literal cursor operations,
  name lookup, operand scanners, and retained file cursors.
- `fast-lexing.md` describes a whole-file lexer prototype under fixed category
  codes and explores deferred cross-line lookahead and worker lexing experiments;
  retained measurements live in `benchmarks/results/fast-lexing.md`.
- `checkpoints.md` defines quiescent driver handoff, heterogeneous snapshots,
  implemented expansion/register restore, and proposed conservative prefix restart
  after edits.
- `resources.md` develops native/Wasm byte acquisition, shared input buffers,
  generated-file append storage, stream handles, diagnostics, and replay.
- `fonts-text.md` contracts immutable TFM resources, restored font descriptors, and
  horizontal list construction before paragraph breaking.
- `paragraphs.md` contracts executable TeX paragraph breaking, automatic
  hyphenation and packing, and develops future pdfTeX spacing extensions.
- `math-alignments.md` contracts executable math-code initialization and typed
  atom conversion, and develops math scanning, display math, alignment templates,
  entry measurement, and result-list transfers.
- `pages.md` contracts executable vertical contributions, page breaks, detached
  publication, output routines, unsplit insertions, adjustments, and retained unfinished pages,
  and develops insertion splitting and marks.
- `output.md` records detached page publication and develops ordered backend
  shipout, document state,
  DVI/PDF resources, and direct serialization.
- `formats.md` develops bootstrap, Umber format export/read, shared semantic
  baselines, and document startup.
- `performance.md` describes native/Wasm hot paths, current layout measurement
  scope, allocation methodology, comparable upstream runs, and compatibility checks.
- `mvp.md` defines the first executable expansion/register slice, supported
  primitives, compatibility fixtures, and same-source performance comparison.
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
