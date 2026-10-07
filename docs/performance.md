# Native/Wasm execution and performance evidence

This proposal explains how to turn the subsystem designs into measurable Rust
without letting measurement machinery take over the architecture. The target is
full native execution comparable to native pdfTeX, with useful incremental restart
and the same interpreter on Wasm. The first expansion/register implementation
has [native measurements](mvp-results.md); layout, checkpoint, and Wasm runtime
measurements will follow those implementations.

## 1. One execution path

Keep token reading, expansion, assignment, character construction, and layout in
ordinary Rust modules. Use static command tags and direct dispatch. Calls to the
host occur at resource and result boundaries, so choosing a native or browser
adapter does not add a virtual call or Wasm host crossing to each token or node.
An adapter trait can be generic or dynamically dispatched on its cold operations;
the interpreter hot path does not depend on that choice.

Pass compact read results and already-resolved meanings between the reader and
dispatch. Token cursors and scoped node views expose straightforward loads.
Split subsystem borrows where a small helper needs only a table, cursor, or arena,
so repeated whole-engine coordination does not obscure the loop or force copies.
Inline the small measured helpers; retain readable functions for scans and
algorithms whose ordinary calls naturally encode their work.

Inspect actual release instructions for the token/list and character paths.
Look for duplicate meaning resolution, tag tests, bounds checks, chunk-directory
lookups, ownership operations, and allocator calls that survive compilation.
Change their representation or call boundary when that extra work dominates.
An elaborate specialization is useful only when it improves the execution path
we actually run.

The [native profile builder](../benchmarks/pgo.py) uses the core and Gentle-derived
workloads to guide compiler inlining and code placement. Build the instrumented interpreter
in its own checkout-local target directory, validate every training checksum and
the native oracle fixtures, then rebuild with the merged profile. The resulting
interpreter executes ordinary compiled Rust without profiling counters. Keep
the source and Wasm build shared; the profile is a compiler build artifact.

Compare that native build with the ordinary release build at sizes outside the
training set. Record the training inputs, compiler options, profile identity,
executable identities, and both suites' samples. Compiler inlining choices need
the same full-workload evidence as source changes; a faster character path can
otherwise enlarge unrelated scanners and slow their stored-token loops.

## 2. Allocation is part of the experiment

Construct new tokens and nodes in destination storage. Retain immutable ranges
and move list/byte-chunk ownership rather than flattening them into new buffers.
Allocate paragraph candidates and word scratch in bulk, retaining capacity for
reuse. Snapshot sharing operates at tables, chunks, and semantic owner boundaries.

The first executable storage comparison should cover contiguous versus segmented
token sequences, node-cell size, table-block/node-chunk size, and direct/scoped
node lookup. Measure allocation frequency, materialized token words, copied bytes,
and retained owners as well as time. Include I/O tail capacity, retained range
descriptors, and bytes materialized by prefix overwrites. A compact representation
that adds another indirection on every token may cost more than the memory it saves.

Checkpointing can be disabled or sparsely retained in a one-shot session while
running the same interpreter. Compare that path with paragraph checkpoints
enabled to see their actual cost. Include mutation after a checkpoint, because
the cost of making a shared chunk writable appears in subsequent execution,
not just inside snapshot creation.

## 3. Native and Wasm constraints

Compile the portable engine for native and `wasm32-unknown-unknown`. Wasm adapters
provide input bytes and result ownership; browser execution lives in a worker.
Scheduler yields and cooperative cancellation occur only at the fixed paragraph/
no-open-groups checkpoints. A document with long grouped work simply has a long
interval between those boundaries.
Time representative full compilation and edits in a named browser/runtime,
including worker/module startup and adapter copies. Distinguish a fresh worker
from a reused module/session rather than inferring Wasm speed from native results.

Use checked compact indices where they fit the profile and address space. Arena
chunk sizes, alignment, and growth can be adjusted by target without changing
semantic ownership. Prefer reuse of allocated capacity on both targets; measure
the Wasm module's retained memory as well as the Rust owners' live bytes.

Normal macro chains iterate through the shared expansion loop. Truly nested
expansion-order and scanner calls still use the Rust stack. Bound actual nested
helper depth at the relevant call sites where a controlled TeX capacity error is
needed, and test both targets' available stack. This does not add a frame object
or depth check to every literal-token read.

Rust target services are documented in the
[Rust Wasm target guide](https://doc.rust-lang.org/stable/rustc/platform-support/wasm32-unknown-unknown.html).
Resource and result crossings follow [resources.md](resources.md).

## 4. Comparable full compilations

Use native pdfTeX as the performance reference with the same TeX inputs, format
semantics, font/image resources, output mode, and enabled extensions. Compare full
compilations, including startup and output work. Report format construction/export
and cache warmup separately so a cached Rust format is compared with the relevant
native format-loading path. Measure cold Umber format reads separately from warm
in-memory baseline reuse.

Use several real workloads: macro-heavy package loading, plain text, formula and
alignment-heavy documents, insertion/output-routine-heavy documents, and PDF
resources. Smaller cases isolate a bottleneck once a full document identifies it.
An expansion-only result cannot establish comparable full pdfTeX performance.

Record the tested engine versions, release configuration, platform, inputs, and
whether caches were warm alongside reported measurements. Repeat enough to
understand variation, then use the differences to choose the next optimization.
No fixed machine, arbitrary ratio gate, or permanent timing dashboard is required
to make that evidence useful.

## 5. Incremental workloads

Measure editing near the beginning, middle, and end of a real document. Record
the chosen checkpoint, prefix work reused, input validation/rebinding,
abandoned-root release, suffix execution, snapshot/COW work, and peak retained
memory. Include all of these in edit-to-result elapsed time. Include grouped
changes that force an
earlier restart, a source append after observed EOF, changed font/image resources,
and generated-file/multiple-pass behavior.

Measure several missing-resource discoveries before the first checkpoint, not
just a single replay. That catches definition-tail amplification and repeated
preamble work. A cached resource reduces later acquisition cost, while the
ordinary interpreter still reexecutes from its valid root.

Separate interner/format/resource caches from retained checkpoint and result
memory. Both count toward process/module footprint, but their useful lifetimes
and reclamation choices differ. Trace a repeated-edit session long enough to see
retention grow or settle; then change the store responsible.

## 6. Compatibility evidence beside timing

Use upstream TeX/pdfTeX and independent fixtures as semantic references. Relevant
TRIP/e-TRIP-style cases exercise primitive and error behavior; actual formats and
documents exercise interactions. Umber3 can provide examples but is not the
correctness oracle.

Compare token identities/consumption when testing input, meaning/restore behavior
for assignments, exact list dimensions and breaks for layout, and ordered effects
and output structure for shipout. For each incremental case, compare resumed
execution with a fresh run on the same edited input view, supplied run environment,
and ordered external answers, including the changed suffix. Date/time, random
seeds, terminal answers, and sampled clocks can otherwise change TeX control flow.
Fix a semantic mismatch
before using its faster execution as performance evidence.

Useful observables can be collected in a diagnostic build or at operation
boundaries. Keep normal token/node loops free of mandatory tracing objects or
counters. Sample/profile the release path we intend to ship.

## 7. First executable decisions

A small input/definition/group interpreter can already compare the expansion
loop, in-place argument capture, and table layouts. Horizontal construction and
paragraph breaking then exercise node stores, working arenas, and checkpoint
mutation. Real format loading and output routines connect those paths before
backend serialization and broader extensions complete the compatibility target.

Those slices should produce actual executable evidence and ordinary focused
tests. They do not require a universal storage abstraction, a second continuation
machine, or speculative benchmark infrastructure. The [MVP](mvp.md) now supplies
the Cargo workspace, focused tests, native oracle, and same-source benchmark
runner. Broaden those tools alongside the next implemented slice.
