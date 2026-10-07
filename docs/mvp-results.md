# Native expansion measurements

The [original MVP](mvp.md) and [broader expansion slice](expansion-mvp.md) are
implemented, with 63 primitive names. At the largest sizes, Umber uses about
0.15–0.70 times native pdfTeX's CPU time on the original eight workloads and
0.43–0.78 times on seven broader macro workloads. Grouped file-argument runs
append directly into capture storage, reducing contents serialization CPU time
by 26.5% and optional-argument CPU time by 18.4% versus the preceding build.
These measurements cover parsing, expansion, and macro/scalar execution. Layout,
shipout, formats, checkpoints, and Wasm runtime timing come later.

The tables use child user-plus-system CPU medians for the complete engine process.
Reports also retain wall time and all samples. Wall time includes scheduling
delays from other work on this shared host.

## Broader macro workloads

The [expanded generator](../benchmarks/expansion.py) uses four workloads drawn
from Michael Doob's [gentle.tex](https://ctan.net/info/gentle/gentle.tex), plus
three workloads for token lists, a larger name table, and optional arguments.
The [benchmark guide](../benchmarks/README.md) describes every consumer and
adaptation. The full manual needs the later typesetting subsystems.

Each record expands its full payload into a definition and compares it with
independently generated expected tokens. Only a successful comparison increments
the final checksum. Expected-definition construction and comparison are timed.
Contents serialization compares detokenized text; native fixtures separately
check conversion categories. Argument text varies from 8 to 1,024 characters,
with nested groups, field macros, different branches, and source/stored routes.

CPU medians at 50,000 records; ratios below 1 favor Umber:

| Workload                           | Umber (s) | pdfTeX (s) | CPU time ratio | Change vs previous Umber |
| ---------------------------------- | --------: | ---------: | -------------: | -----------------------: |
| Gentle spelling and text templates |     0.186 |      0.319 |          0.583 |                    -3.3% |
| Gentle calendar formatter          |     0.153 |      0.270 |          0.567 |                    +1.8% |
| Gentle contents serialization      |     0.488 |      1.016 |          0.480 |                   -26.5% |
| Gentle paragraph-delimited records |     0.952 |      1.227 |          0.776 |                    -0.3% |
| Token-register pipeline            |     1.147 |      2.341 |          0.490 |                    -0.1% |
| Dynamic name table                 |     0.177 |      0.320 |          0.554 |                    +0.5% |
| Optional-argument parser           |     0.659 |      1.533 |          0.430 |                   -18.4% |

Contents serialization and optional arguments use 26.5% and 18.4% less CPU time.
Text templates improve by 3.3%; records and token pipelines change by less than
0.4%. Calendar formatting takes 1.8% more CPU time and name tables take 0.5% more.

The growth ratios between 10,000 and 50,000 records range from 0.43 to 0.78,
so these conclusions hold after removing small-case startup advantage.
The comparison binary is the preceding 63-primitive interpreter at `0336934`,
timed in the same run. Full samples, source hashes, corpus provenance, compiler
identity, and executable hashes are retained in
[expansion-native.json](../benchmarks/results/expansion-native.json).
The earlier report at `087205e` is preserved in
[expansion-before-token-windows.json](../benchmarks/results/expansion-before-token-windows.json).

## Original core regression set

CPU medians at one million work iterations, with the same comparison binary:

| Workload                       | Umber (s) | pdfTeX (s) | CPU time ratio | Change vs previous Umber |
| ------------------------------ | --------: | ---------: | -------------: | -----------------------: |
| Raw byte dispatch              |     0.242 |      0.562 |          0.432 |                    -2.5% |
| Stored macro bodies            |     0.129 |      0.266 |          0.486 |                    -2.6% |
| Integer assignments            |     0.162 |      0.355 |          0.457 |                    -2.8% |
| Repeated/nested parameters     |     0.428 |      0.612 |          0.700 |                    +2.6% |
| Arguments tokenized from bytes |     0.497 |      0.798 |          0.623 |                    -1.8% |
| Repeated 256-token arguments   |     1.298 |      8.472 |          0.153 |                    +1.2% |
| Overlapping delimiters         |     0.154 |      0.277 |          0.556 |                    -0.7% |
| Local/global grouped writes    |     0.305 |      0.522 |          0.585 |                    -1.7% |

Six core workloads improve by 0.7–2.8%. Repeated/nested parameters take 2.6%
more CPU time, and repeated 256-token arguments take 1.2% more. The earlier
expansion pass added 9–23% to several paths relative to the 32-primitive
interpreter at `993107b`; that comparison remains in
[mvp-before-token-windows.json](../benchmarks/results/mvp-before-token-windows.json).

The [core generator](../benchmarks/mvp.py) uses compact macro fanout except for
raw dispatch and arguments tokenized from source bytes. Its 256-token argument
case discards the payload after scanning; the broader suite consumes its text.
The CLI reads the source into memory before execution. The historical
`file-parameters` key measures tokenization over resident bytes, with full-process
input reading still included in timing.

The growth ratios from 100,000 to one million iterations range from 0.15 to 0.70.
Startup probes in the two runs use 1.04–1.13 ms CPU time in Umber and 4.27–4.91 ms
in pdfTeX. Current samples are retained in
[mvp-native.json](../benchmarks/results/mvp-native.json).
Earlier reports in `benchmarks/results/` retain the original optimization passes,
including [mvp-before-expansion.json](../benchmarks/results/mvp-before-expansion.json).

## Allocation and representation

Token registers share immutable arena spans, including copies and local saves.
Their active sources keep reading the original span after replacement. Expanded
definitions and messages emit number and text conversions directly into their
destination. The generated character tokens need no further expansion or brace
and parameter handling. Text conversion shares its byte emitter with general
expansion; its body stays outside the common list loop.

When general expansion needs a token source, conversion output writes into a
reusable, uniquely owned buffer and publishes one range at completion. Numbers
that fit in the existing two-token inline source use it directly, including the
sign for negative single digits. Digits for larger values are generated from low
to high and reversed in place before publication. Release code uses reciprocal
multiplication and shifts for division by ten.

Stored list scans consume ordinary character runs using Rust pointer locals and
append directly to their destinations. Braces, parameter words, control names,
and source changes pass through the surrounding scanner. The destination keeps
input backing alive while it grows. Expanded scans splice `\the` token lists and
`\unexpanded` directly into their destination; messages write bytes directly.
Source-line scans likewise emit letter and other-character tokens directly, using
the live category table. The inspected release loop advances the file cursor and
lexical state before appending, with no per-character reader call, meaning lookup,
or name-table growth check. Special categories and line transitions return to the
complete reader. Destination emitters preserve input and category state.
Fixed-size arena chunks allow a small inline append path with allocation in a
separate growth helper. Chunk allocation has no 16 KiB stack temporary in the
inspected native release code.

Grouped arguments from source bytes likewise scan letter and other-character
runs. The first word uses the existing inline capture path; subsequent words
append directly under one borrow of the uniquely owned buffer. Buffer lookup and
sequence bookkeeping sit outside the inspected release character loop. One range
records the run, coalescing with adjacent capture storage. Publication retains
the buffer once per invocation. Discarded arguments scan runs without storage.

The main driver fuses token reading, expansion dispatch, and execution in one
Rust loop. Operand scanners retain `next_expanded`. The inspected release loop
reads sources directly, avoiding the expanded-reader call and return on each
main-loop token while keeping the same macro and primitive helpers.

Arena windows derive their pointers from the whole chunk with
[`UnsafeCell::raw_get`](https://doc.rust-lang.org/std/cell/struct.UnsafeCell.html#method.raw_get).
Macro equality compares complete signatures through these immutable windows.
Each step compares the shorter available window as transparent 32-bit token
words, then advances both ranges. The inspected native release code calls
`bcmp` for these contiguous ranges. Meaning, command, flag, and length shortcuts
remain ahead of the comparison; no tokens are copied or allocated.

Scalar tables share integer values and group levels in one indexed array.
Assignments resolve one slot for reading, saving, and writing. Prefix scans live
in an ordinary Rust helper; deferred assignment classification runs only when a
hook is pending. Discarded delimited arguments retain matching state while
skipping bookkeeping used solely to trim retained text. Their matcher compares
one expected token per match or fallback. Release instructions were inspected
with `objdump`; this host denies hardware performance counters.

The separate allocation diagnostic counts engine construction and execution,
excluding host input bytes, Rust stack, allocator overhead, and pdfTeX memory.
At one million iterations, the five sampled original capture/body workloads use
88–99 allocations and 55,764–57,417 peak requested heap bytes. Tenfold more work
adds at most two allocations. Invocation storage follows maximum live nesting
and argument size.

The broader suite deliberately replaces definitions and token-register values.
Their arena storage and definition headers accumulate until the run ends:

| Expanded workload                  | Allocations at 50,000 | Peak requested engine heap (MiB) |
| ---------------------------------- | --------------------: | -------------------------------: |
| Gentle spelling and text templates |                 1,174 |                             38.1 |
| Gentle calendar formatter          |                   521 |                             17.1 |
| Gentle contents serialization      |                 4,796 |                             94.6 |
| Gentle paragraph-delimited records |                 8,697 |                            144.6 |
| Token-register pipeline            |                22,166 |                            366.2 |
| Dynamic name table                 |                 5,476 |                             29.6 |
| Optional-argument parser           |                10,345 |                            170.1 |

Common dynamic names use 64 bytes of stack scratch space, spilling once for
longer names. Fixed symbol spellings use a static byte table, eliminating their
separate heap allocations. Spelling handles and hashes retain direct ID indexing;
the native name-lookup instructions match the preceding implementation.
Name assembly and static spellings reduce allocation calls in the largest
name-table case from 187,563 to 5,476, a 97.1% reduction. Interned spellings retain
their session lifetime.

These are requested heap peaks, not process RSS. The token pipeline grows from
74.4 MiB at 10,000 records to 366.2 MiB at 50,000. Replaced field, expected,
pipeline, and result definitions remain in the arena. Fast timings do not
establish bounded memory for a long editing session. Checkpoint-aware store
reclamation remains with the [storage design](storage.md). Both reports retain
the diagnostic binary hash and counts at both sizes.

## Compatibility and reproduction

Twenty-eight Rust tests cover the core plus token-register retention, protected
and opaque insertion, conditional selection, deferred token order, case changes,
conversion categories, expression rounding/scaling/error order, generated-buffer
reuse, numeric and expansion-depth boundaries, fixed symbol names, and long/nested
dynamic names. The native oracle compares 202 marked messages and 23 rejected
programs. Expected extended-fixture messages were captured from native pdfTeX.

Symbol fixtures check NUL, bytes 127/128/255, empty names, and distinct ordinary
and active meanings; they retain early names and compare conversion output after
several hash-table growths. Number fixtures cross the inline-output boundaries,
retain results in expanded definitions, and feed them into scanners and captures.
They compare generated token categories after changing digit and sign catcodes,
include both integer extremes, and use numeric output as a parameter operand.
Malformed conversion operands retain priority over destination-capacity errors.
Macro-equality fixtures distinguish category codes, control-sequence identities,
parameter marker spellings, and macro flags. Generated equality tests cross
shifted chunk boundaries and change signature prefixes, lengths, and body words;
pdfTeX independently agrees on their 37 comparisons.
File-argument fixtures check whitespace and comments across physical lines,
translated braces, active characters, multiple arguments sharing a builder, and
captured categories surviving executed category changes.

Storage tests cross 4,096-token chunks, replace active sources, exercise 5,000
nested captures, and produce multi-chunk conversion text. List-scan tests
retain long source arguments while conversion/case-change output grows, repeat
those arguments, and substitute parameter words at chunk boundaries. Independent
adversarial reviews checked semantics, source ownership, release paths, and
workload consumers. Debug/release tests, rustfmt, and clippy pass; the portable
library builds for `wasm32-unknown-unknown`.
The arena-window test also passes Miri, covering neighboring-cell access, disjoint
appends, overlapping chunk comparisons, and empty ranges.

Measured on 2026-10-07, Linux x86-64, AMD EPYC 7642, pinned to CPU 0:

- pdfTeX 3.141592653-2.6-1.40.25, TeX Live 2023/Debian, with
  `-ini -etex -no-shell-escape -interaction=nonstopmode -halt-on-error`.
- Rust 1.98.1, release profile with thin LTO, one codegen unit, debug info,
  and no additional `RUSTFLAGS`.
- One warmup and seven measured runs per engine at each size: 10,000, 100,000,
  and 1,000,000 core iterations; 1,000, 10,000, and 50,000 expanded records.
- Rotating and reversing the order of Umber, pdfTeX, and the comparison binary.
  Both clocks cover startup, input reading, initialization, execution, and result
  output. pdfTeX also writes its normal log. Sources are warm in the file cache.
  Every engine receives the same complete source. Variation is retained in JSON.

```bash
cargo test
cargo test --release
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo build --release
python3 scripts/oracle.py
python3 benchmarks/mvp.py --report target/benchmark/core.json
python3 benchmarks/expansion.py --report target/benchmark/expanded.json
cargo build --release --lib --target wasm32-unknown-unknown
cargo build --release --example allocations
target/release/examples/allocations target/benchmark/expansion/token-pipeline-50000.tex
```

The runners accept `--pdftex /path/to/pdftex`, `--umber /path/to/umber`, and optional
`--baseline` binaries. They need native pdfTeX, with no format or font installation.
The oracle here is the checkout-local Ubuntu Noble TeX Live 2023 installation
under `.tools/pdftex`; a system installation works too. The MVP clock parameters
use fixed epoch defaults unless supplied through `Engine::set_calendar`; corpus
fixtures assign deterministic values explicitly.
