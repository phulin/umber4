# Native expansion measurements

The [original MVP](mvp.md) and [broader expansion slice](expansion-mvp.md) are
implemented, with 63 primitive names. At the largest sizes, Umber uses
0.14–0.74 times native pdfTeX's CPU time on the original eight workloads and
0.59–0.82 times on seven broader macro workloads. The broader cases use 6–18%
less CPU time than the previous implementation. These measurements cover parsing,
expansion, and macro/scalar execution. Layout, shipout, formats, checkpoints,
and Wasm runtime timing come later.

Both runners retain wall time and child user-plus-system CPU time for the complete
engine process. Other builds on this shared host introduce scheduling delays;
the tables below use CPU medians. For example, the largest optional-argument case
has 0.966 s CPU time and 1.438 s wall time in Umber, versus 1.627 s and 1.927 s in
pdfTeX. Both clocks and all samples remain in the reports.

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
| Gentle spelling and text templates |     0.201 |      0.314 |          0.638 |                    -9.9% |
| Gentle calendar formatter          |     0.169 |      0.278 |          0.609 |                    -6.3% |
| Gentle contents serialization      |     0.794 |      1.039 |          0.765 |                   -15.3% |
| Gentle paragraph-delimited records |     1.043 |      1.266 |          0.824 |                   -14.1% |
| Token-register pipeline            |     1.482 |      2.361 |          0.628 |                   -18.4% |
| Dynamic name table                 |     0.196 |      0.324 |          0.603 |                    -8.5% |
| Optional-argument parser           |     0.966 |      1.627 |          0.594 |                   -14.4% |

The growth ratios between 10,000 and 50,000 records range from 0.59 to 0.82,
so these conclusions hold after removing small-case startup advantage.
The comparison binary is the preceding 63-primitive interpreter at `087205e`,
timed in the same run. Full samples, source hashes, corpus provenance, compiler
identity, and executable hashes are retained in
[expansion-native.json](../benchmarks/results/expansion-native.json).
The preceding report is preserved in
[expansion-before-token-windows.json](../benchmarks/results/expansion-before-token-windows.json).

## Original core regression set

CPU medians at one million work iterations, with the same comparison binary:

| Workload                       | Umber (s) | pdfTeX (s) | CPU time ratio | Change vs previous Umber |
| ------------------------------ | --------: | ---------: | -------------: | -----------------------: |
| Raw byte dispatch              |     0.286 |      0.589 |          0.485 |                    +8.4% |
| Stored macro bodies            |     0.187 |      0.268 |          0.695 |                    -6.2% |
| Integer assignments            |     0.198 |      0.370 |          0.536 |                    -2.6% |
| Repeated/nested parameters     |     0.472 |      0.639 |          0.738 |                    -5.7% |
| Arguments tokenized from bytes |     0.553 |      0.826 |          0.670 |                    -1.7% |
| Repeated 256-token arguments   |     1.212 |      8.659 |          0.140 |                    -7.9% |
| Overlapping delimiters         |     0.185 |      0.269 |          0.688 |                    +5.6% |
| Local/global grouped writes    |     0.355 |      0.528 |          0.672 |                    -0.4% |

Stored bodies, retained arguments, and long discarded arguments improve. Raw
byte dispatch and overlapping delimiters regress by 8.4% and 5.6%, respectively;
they remain in the optimization set. Grouped writes are essentially unchanged.
The preceding expansion pass added 9–23% to several paths relative to the
32-primitive interpreter at `993107b`; that comparison remains in
[mvp-before-token-windows.json](../benchmarks/results/mvp-before-token-windows.json).

The [core generator](../benchmarks/mvp.py) uses compact macro fanout except for
raw dispatch and arguments tokenized from source bytes. Its 256-token argument
case discards the payload after scanning; the broader suite consumes its text.
The CLI reads the source into memory before execution. The historical
`file-parameters` key measures tokenization over resident bytes, with full-process
input reading still included in timing.

The growth ratios from 100,000 to one million iterations range from 0.14 to 0.74.
Startup probes in the two runs use 1.13–1.62 ms CPU time in Umber and 4.77–5.80 ms
in pdfTeX. Current samples are retained in
[mvp-native.json](../benchmarks/results/mvp-native.json).
Earlier reports in `benchmarks/results/` retain the original optimization passes,
including [mvp-before-expansion.json](../benchmarks/results/mvp-before-expansion.json).

## Allocation and representation

Token registers share immutable arena spans, including copies and local saves.
Their active sources keep reading the original span after replacement. Generated
conversion output writes directly into a reusable, uniquely owned buffer and
publishes one range at completion. Singleton output stays inline. Decimal digits
are generated from low to high and reversed in place before publication; release
code uses reciprocal multiplication and shifts for division by ten.

Stored list scans consume ordinary character runs using Rust pointer locals and
append directly to their destinations. Braces, parameter words, control names,
and source changes pass through the surrounding scanner. The destination keeps
input backing alive while it grows. Expanded scans splice `\the` token lists and
`\unexpanded` directly into their destination; messages write bytes directly.
Fixed-size arena chunks allow a small inline append path with allocation in a
separate growth helper. Chunk allocation has no 16 KiB stack temporary in the
inspected native release code.

Scalar tables share integer values and group levels in one indexed array.
Assignments resolve one slot for reading, saving, and writing. Prefix scans live
in an ordinary Rust helper; deferred assignment classification runs only when a
hook is pending. Discarded delimited arguments retain matching state while
skipping bookkeeping used solely to trim retained text. Release instructions
were inspected with `objdump`; this host denies hardware performance counters.

The separate allocation diagnostic counts engine construction and execution,
excluding host input bytes, Rust stack, allocator overhead, and pdfTeX memory.
At one million iterations, the five sampled original capture/body workloads use
615–625 allocations and 56,804–58,166 peak requested heap bytes. Tenfold more work
adds at most two allocations. Invocation storage follows maximum live nesting
and argument size.

The broader suite deliberately replaces definitions and token-register values.
Their arena storage and definition headers accumulate until the run ends:

| Expanded workload                  | Allocations at 50,000 | Peak requested engine heap (MiB) |
| ---------------------------------- | --------------------: | -------------------------------: |
| Gentle spelling and text templates |                 1,696 |                             38.1 |
| Gentle calendar formatter          |                 1,050 |                             17.1 |
| Gentle contents serialization      |                 5,326 |                             94.6 |
| Gentle paragraph-delimited records |                 9,219 |                            144.6 |
| Token-register pipeline            |                22,692 |                            366.2 |
| Dynamic name table                 |                 6,005 |                             29.6 |
| Optional-argument parser           |                10,869 |                            170.1 |

Common dynamic names use 64 bytes of stack scratch space, spilling once for
longer names. Name assembly therefore reduces allocation calls in the largest
name-table case from 187,563 to 6,005, a 96.8% reduction. Interned spellings retain
their session lifetime.

These are requested heap peaks, not process RSS. The token pipeline grows from
74.4 MiB at 10,000 records to 366.2 MiB at 50,000. Replaced field, expected,
pipeline, and result definitions remain in the arena. Fast timings do not
establish bounded memory for a long editing session. Checkpoint-aware store
reclamation remains with the [storage design](storage.md). Both reports retain
the diagnostic binary hash and counts at both sizes.

## Compatibility and reproduction

Twenty-four Rust tests cover the core plus token-register retention, protected
and opaque insertion, conditional selection, deferred token order, case changes,
conversion categories, expression rounding/scaling/error order, generated-buffer
reuse, and long/nested dynamic names. The native oracle compares 168 marked
messages and 23 rejected programs. Expected extended-fixture messages were
captured from native pdfTeX.

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
