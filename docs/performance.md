# Performance and compatibility methodology

Native full compilation should perform comparably to native pdfTeX. The same Rust
interpreter runs on Wasm, and editing sessions should reuse useful work without
excessive snapshot or retention costs. Retained
[expansion measurements](../benchmarks/results/expansion.md) and
[paragraph/page measurements](../benchmarks/results/layout.md) describe their named
builds and workloads; neither establishes browser runtime or broad TeX/LaTeX
performance.

Architecture comes first: preserve exact TeX semantics, clear ownership,
semantic-lifetime allocation of headers and buffers, in-place construction, and
quiescent checkpoint boundaries. Synthetic microbenchmarks diagnose costs; they
do not determine architectural acceptance or require eliminating every per-case
CPU regression. Retain useful allocator and ownership improvements while deferring
extensive tuning until representative document paths and the architecture mature.

## 1. Execution costs

Use direct command dispatch, compact values, and ordinary Rust calls. Host access
belongs at resource and result boundaries. Share already-resolved meanings between
readers and dispatch, and expose scoped token/node windows to bulk operations.
Inspect release instructions when changing hot loops: duplicate classification,
meaning resolution, chunk lookup, ownership traffic, and allocator calls are the
main avoidable costs. Measure the complete affected workload before specializing.
Keep engine state organized by ownership and subsystem boundaries; field layout
and compiler-specific annotations are implementation choices, not storage contracts.

Line-local lookahead and hashing are bounded input optimizations. Cross-line regime
caches and worker lexing in [fast-lexing.md](fast-lexing.md) remain experiments until
full-engine profiles show that their benefit exceeds invalidation, metadata, and
coordination costs. SIMD acceleration keeps a portable scalar path.

## 2. Allocation and retention

The allocation requirement is in [storage.md](storage.md): ordinary object creation
and release use lifetime-based reusable stores, including metadata. System calls
are for backing capacity growth, not each TeX object. Count allocations in a
separate diagnostic run; production token/node loops need no tracing objects.

Use three complementary cases:

- Repeat a fixed live working set after warmup. Backing allocation requests should
  stop growing with iteration count while the work continues to execute.
- Mix transient work with small durable survivors, then release those survivors.
  Account for payload, metadata, reserved capacity, and empty-block caches. Check
  that lifetime separation avoids retaining mostly dead transient blocks.
- Grow live state deliberately. Storage should track live demand, with bounded
  metadata and fragmentation overhead; a flat fixed-state test alone is insufficient.

For snapshots, include capture, mutation after capture, restoration, and root
release in timing. Count copied table/node bytes and retained descriptor blocks as
well as payload. A shared byte prefix does not justify copying its whole descriptor
history at every checkpoint. Compare sparse/no-checkpoint execution with an editing
session using the same interpreter.

## 3. Comparable runs

Use the same TeX input, format semantics, resources, output mode, and extensions in
Umber and native pdfTeX. Time full execution, including startup and output. Report
source-built format preparation, cold format loading, and warm baseline reuse
separately. Start with ordinary release; retain native PGO results separately from
it and use cases outside the training inputs. The existing
[profile builder](../benchmarks/pgo.py) documents that optional path.

A useful result needs the revision/build, engine versions, machine/runtime, workload,
cache conditions, repeated timings, and the command to reproduce it. Retain raw
samples with the result. No universal timing gate, permanent dashboard, or additional
provenance protocol is required. Historical comparisons stay in measurement reports;
subsystem contracts should describe the resulting behavior rather than compiler
tuning history.

Real workloads should include package loading, ordinary paragraphs, math and
alignments, insertions/output routines, and PDF resources. Smaller cases isolate
costs within those paths. Extensive tuning of an expansion microbenchmark cannot
substitute for the paragraph/page/checkpoint slice.

### Implemented layout measurement scope

The [layout runner](../benchmarks/layout.py) exercises real TFM loading, character
construction, nonhyphenating paragraph breaking, line packing, page construction,
final flushing, and detached output retention. It extracts distinct prose
paragraphs from six repository design documents, then repeats their sequence to
600 paragraphs per case by default. Book, narrow, and dense geometries use
`ae/aer10`, `latex-fonts/lcmss8`, and `latex-fonts/icmtt8`, respectively, at 10 pt.
The runner compares every page and glyph character/x/y with native DVI through an
independent reader and raw-TFM width scaling before timing.

The [Gentle layout generator](../benchmarks/corpus/gentle-layout.py) supplies a
second workload with authentic local excerpts, grouped roman/italic/monospace
font changes, macro arguments, ligatures, and rules. Its full local source and
selected ranges are identified in [the provenance record](../benchmarks/corpus/gentle.json);
additional prose excerpts are not distributed with measurement reports. Native
checks compare ordered font identities, glyph placements, and rule geometry.

The ordinary release CLI retains detached pages; native pdfTeX runs INITEX with
e-TeX, writes DVI and a log, and suppresses underfull diagnostics through startup
settings. The typesetting input is identical. One warmup and interleaved samples
measure complete-process wall and user-plus-system CPU time with warm input and
resource caches. This comparison covers text composition with different output
endpoints; it excludes package/format loading, hyphenation patterns, math, custom
output routines, and PDF serialization.

The process helper also retains user CPU, system CPU, and minor page faults from
the same child usage record. These include startup, input, output, and teardown;
they do not isolate interpreter phases. Per-field medians need not sum to the
median total CPU. RSS uses the separate grandchild measurement described in the
benchmark runner rather than the child usage record's inherited high-water mark.

The [latest cumulative measurement](../benchmarks/results/layout-current.json)
records the current measured release, preserved original prose inputs, the mixed
workload, and paired ordinary CLI/native samples. The [report](../benchmarks/results/layout.md)
retains the result summary and earlier measurements. These short shared-machine
runs describe their named builds and workloads, not performance guarantees.
Separate Callgrind instruction attribution describes executed work rather than
CPU-time shares.

The layout examples also measure same-source checkpoint replay while retaining
old output prefixes. Early/middle/late suffixes must equal uninterrupted execution
in counts, messages, boxes/glue, placements, penalties, and font resources. Separate
allocation adapters count successful system allocation/reallocation requests and
requested payload bytes; timing binaries contain no counters. Host input/TFM
buffers, allocator headers, stack, and OS mappings are excluded. Requested live
heap includes reserved pooled backing. Warmed capture/release and unchanged-root
restore batches allocate no additional backing in the retained measurements;
fixed suffix replay still allocates, and partial shared output chunks can retain
invisible later records. Same-source replay does not cover source edits, resource
rebinding, or a production scheduler.

The fixed prose fixtures emit a final counter message, including
`UMBER:600:END` for book. Late-suffix execution and `outcome` publication therefore
include message-buffer growth and owned-message publication as well as output
header and node backing. Restore alone is a separate allocation boundary. The
latest measurement retains the audited request attribution; these fixtures are
not message-free.

The [benchmark guide](../benchmarks/README.md) supplies reproduction commands.
The runner regenerates documentation-derived inputs, so paired revision comparisons
must preserve and reuse the same `.tex` files and verify their hashes. Regenerating
a corpus after editing documentation changes the workload.

## 4. Editing sessions and resource replay

Measure edits near the beginning, middle, and end of a document. Include input
validation/rebinding, abandoned-root release, suffix execution, COW work, and peak
retention in edit-to-result latency. Exercise ordinary open groups, saved boxes,
unfinished pages, generated-file readback, observed EOF changes, and changed
font/image resources. Compare completed output with a fresh run on the edited view.

Try several sequential missing resources before an eligible checkpoint and during
shipout. Prepared bundles and prefetch should reduce those replays without changing
TeX observations. Separate session names, cached resources, checkpoints, and old
host results in memory accounting; all contribute to the process/module footprint.
Long editing sessions must release names at session teardown and obsolete branches
when their owners disappear.

## 5. Native and Wasm

Compile and run the same engine on both targets. Measure Wasm in a named runtime,
including startup, resource copies, and result transfer. A reused worker/session
and a fresh worker are different workloads. Browser workers normally yield at
eligible outer-driver boundaries; cancellation or work-limit interruption unwinds
to the last checkpoint independently of eligibility. Include paragraph-free loops
and long grouped work when checking interruption behavior.

Normal macro chains iterate. Truly nested expansion/scanner calls use the Rust
stack with controlled depth limits at the relevant call sites. Check available
stack on both targets without adding a depth test to every literal token. Compact
indices, alignment, and backing chunk sizes must fit each target. Measure Wasm
retained linear memory as well as live Rust owners. Target services are described
in the [Rust Wasm target guide](https://doc.rust-lang.org/stable/rustc/platform-support/wasm32-unknown-unknown.html).

## 6. Correctness checks

Use upstream TeX/pdfTeX and independent fixtures as semantic references. TRIP/
e-TRIP-style cases cover primitive and error behavior; real formats and documents
exercise interactions. Umber3 is reference material, not the correctness oracle.

Compare consumed tokens/current meanings for scanners, restored values for groups,
exact dimensions and breaks for layout, and ordered effects plus output structure
for shipout. A resumed run must match a fresh run on the same edited inputs and
retained run environment, including supplied time/random/terminal answers. Keep
subsystem-specific edge cases in their documents and ordinary focused tests.

Run the repository's relevant tests, formatting, clippy, oracle fixtures, and target
builds for an implementation change. Benchmark only after behavior matches. Broaden
checks when new behavior warrants it rather than rerunning every historical
experiment for every edit.
