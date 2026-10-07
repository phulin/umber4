# Native expansion measurements

The [original MVP](mvp.md) and [broader expansion slice](expansion-mvp.md) are
implemented, with 63 primitive names. At the largest sizes, Umber uses about
0.14–0.71 times native pdfTeX's CPU time on the original eight workloads and
0.57–0.84 times on seven broader macro workloads. Fusing the main driver reduces
stored-body CPU time by 26.6% versus the preceding build. These measurements
cover parsing, expansion, and macro/scalar execution. Layout, shipout, formats,
checkpoints, and Wasm runtime timing come later.

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
| Gentle spelling and text templates |     0.194 |      0.309 |          0.625 |                    -1.7% |
| Gentle calendar formatter          |     0.155 |      0.273 |          0.567 |                    -6.5% |
| Gentle contents serialization      |     0.665 |      0.987 |          0.674 |                    -3.2% |
| Gentle paragraph-delimited records |     1.029 |      1.232 |          0.835 |                     0.0% |
| Token-register pipeline            |     1.465 |      2.364 |          0.620 |                    -1.6% |
| Dynamic name table                 |     0.191 |      0.323 |          0.592 |                    +4.0% |
| Optional-argument parser           |     0.928 |      1.548 |          0.599 |                    -1.5% |

The calendar formatter uses 6.5% less CPU time; contents serialization uses
3.2% less. Other broader cases change by -1.7% to +4.0%, with the increase in the
name-table workload. All changes are shown above.

The growth ratios between 10,000 and 50,000 records range from 0.57 to 0.85,
so these conclusions hold after removing small-case startup advantage.
The comparison binary is the preceding 63-primitive interpreter at `e4343ab`,
timed in the same run. Full samples, source hashes, corpus provenance, compiler
identity, and executable hashes are retained in
[expansion-native.json](../benchmarks/results/expansion-native.json).
The earlier report at `087205e` is preserved in
[expansion-before-token-windows.json](../benchmarks/results/expansion-before-token-windows.json).

## Original core regression set

CPU medians at one million work iterations, with the same comparison binary:

| Workload                       | Umber (s) | pdfTeX (s) | CPU time ratio | Change vs previous Umber |
| ------------------------------ | --------: | ---------: | -------------: | -----------------------: |
| Raw byte dispatch              |     0.251 |      0.555 |          0.452 |                    -3.4% |
| Stored macro bodies            |     0.144 |      0.258 |          0.558 |                   -26.6% |
| Integer assignments            |     0.167 |      0.357 |          0.467 |                   -17.7% |
| Repeated/nested parameters     |     0.450 |      0.634 |          0.710 |                    -1.0% |
| Arguments tokenized from bytes |     0.521 |      0.795 |          0.656 |                    +0.3% |
| Repeated 256-token arguments   |     1.195 |      8.432 |          0.142 |                    -1.4% |
| Overlapping delimiters         |     0.148 |      0.266 |          0.558 |                   -16.6% |
| Local/global grouped writes    |     0.311 |      0.528 |          0.589 |                   -13.4% |

Stored bodies, integer assignments, delimiter matching, and grouped writes
improve by 13.4–26.6%. Raw dispatch and stored-argument cases improve by 1.0–3.4%.
Byte-tokenized arguments take 0.3% more CPU time. This recovers the stored-body
and delimiter regressions from the preceding conversion pass. The earlier
expansion pass added 9–23% to several paths relative to the 32-primitive
interpreter at `993107b`; that comparison remains in
[mvp-before-token-windows.json](../benchmarks/results/mvp-before-token-windows.json).

The [core generator](../benchmarks/mvp.py) uses compact macro fanout except for
raw dispatch and arguments tokenized from source bytes. Its 256-token argument
case discards the payload after scanning; the broader suite consumes its text.
The CLI reads the source into memory before execution. The historical
`file-parameters` key measures tokenization over resident bytes, with full-process
input reading still included in timing.

The growth ratios from 100,000 to one million iterations range from 0.14 to 0.71.
Startup probes in the two runs use 1.03–1.07 ms CPU time in Umber and 4.16–4.29 ms
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
Fixed-size arena chunks allow a small inline append path with allocation in a
separate growth helper. Chunk allocation has no 16 KiB stack temporary in the
inspected native release code.

The main driver fuses token reading, expansion dispatch, and execution in one
Rust loop. Operand scanners retain `next_expanded`. The inspected release loop
reads sources directly, avoiding the expanded-reader call and return on each
main-loop token while keeping the same macro and primitive helpers.

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

Twenty-six Rust tests cover the core plus token-register retention, protected
and opaque insertion, conditional selection, deferred token order, case changes,
conversion categories, expression rounding/scaling/error order, generated-buffer
reuse, numeric and expansion-depth boundaries, fixed symbol names, and long/nested
dynamic names. The native oracle compares 183 marked messages and 23 rejected
programs. Expected extended-fixture messages were captured from native pdfTeX.

Symbol fixtures check NUL, bytes 127/128/255, empty names, and distinct ordinary
and active meanings; they retain early names and compare conversion output after
several hash-table growths. Number fixtures cross the inline-output boundaries,
retain results in expanded definitions, and feed them into scanners and captures.
They compare generated token categories after changing digit and sign catcodes,
include both integer extremes, and use numeric output as a parameter operand.
Malformed conversion operands retain priority over destination-capacity errors.

Storage tests cross 4,096-token chunks, replace active sources, exercise 5,000
nested captures, and produce multi-chunk conversion text. New list-scan tests
retain long source arguments while conversion/case-change output grows, repeat
those arguments, and substitute parameter words at chunk boundaries. Independent
adversarial reviews checked semantics, source ownership, release paths, and
workload consumers. Debug/release tests, rustfmt, and clippy pass; the portable
library builds for `wasm32-unknown-unknown`.

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
