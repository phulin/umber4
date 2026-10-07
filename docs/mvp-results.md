# Native expansion measurements

The [original MVP](mvp.md) and [broader expansion slice](expansion-mvp.md) are
implemented, with 63 primitive names. At the largest sizes, Umber takes
0.15–0.76 times native pdfTeX's median time on the original eight workloads and
0.64–0.99 times on seven broader macro workloads. The paragraph-delimited Gentle
case is near parity; the other larger macro cases are faster on this host.
These measurements cover parsing, expansion, and macro/scalar execution.
Layout, shipout, formats, checkpoints, and Wasm runtime timing come later.

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

Medians at 50,000 records; ratios below 1 favor Umber:

| Workload                           | Umber (s) | pdfTeX (s) | Time ratio |
| ---------------------------------- | --------: | ---------: | ---------: |
| Gentle spelling and text templates |     0.218 |      0.314 |      0.693 |
| Gentle calendar formatter          |     0.184 |      0.286 |      0.645 |
| Gentle contents serialization      |     0.902 |      1.023 |      0.882 |
| Gentle paragraph-delimited records |     1.230 |      1.247 |      0.987 |
| Token-register pipeline            |     1.826 |      2.392 |      0.763 |
| Dynamic name table                 |     0.227 |      0.325 |      0.697 |
| Optional-argument parser           |     1.127 |      1.573 |      0.716 |

The growth ratios between 10,000 and 50,000 records range from 0.65 to 0.99,
so these conclusions hold after removing small-case startup advantage.
Contents serialization improves from 1.052 s in the first expanded candidate
measured in the same run to 0.902 s, 14.3% less time. Other changes against that
candidate range from 2.2% less time to 3.4% more time; their sample ranges overlap.
All samples, source hashes, corpus provenance, compiler identity, and binary
hashes are retained in [expansion-native.json](../benchmarks/results/expansion-native.json).

## Original core regression set

Medians at one million work iterations:

| Workload                       | Umber (s) | pdfTeX (s) | Time ratio | Time vs previous Umber |
| ------------------------------ | --------: | ---------: | ---------: | ---------------------: |
| Raw byte dispatch              |     0.250 |      0.563 |      0.444 |                  −1.5% |
| Stored macro bodies            |     0.199 |      0.264 |      0.755 |                 +18.7% |
| Integer assignments            |     0.196 |      0.359 |      0.545 |                 +11.7% |
| Repeated/nested parameters     |     0.484 |      0.635 |      0.763 |                 +18.3% |
| Arguments tokenized from bytes |     0.553 |      0.794 |      0.697 |                  +9.0% |
| Repeated 256-token arguments   |     1.294 |      8.640 |      0.150 |                  −9.8% |
| Overlapping delimiters         |     0.179 |      0.281 |      0.638 |                  −4.1% |
| Local/global grouped writes    |     0.379 |      0.559 |      0.677 |                 +23.0% |

The broader command set adds measurable cost to several original paths, even
though all remain faster than pdfTeX. The comparison binary is the committed
32-primitive interpreter at `993107b`, timed in the same run. These regressions
remain visible rather than being hidden by the new workloads.

The [core generator](../benchmarks/mvp.py) uses compact macro fanout except for
raw dispatch and arguments tokenized from source bytes. Its 256-token argument
case discards the payload after argument scanning; the broader suite consumes
its text. The CLI reads the source into memory before execution. The historical
`file-parameters` key measures tokenization over resident bytes, with full-process
input reading still included in timing.

The growth ratios from 100,000 to one million iterations range from 0.15 to 0.77.
The current startup probe is 1.46 ms in Umber and 5.15 ms in pdfTeX. Full samples
are retained in [mvp-native.json](../benchmarks/results/mvp-native.json).
The preceding run is preserved in
[mvp-before-expansion.json](../benchmarks/results/mvp-before-expansion.json);
earlier reports in `benchmarks/results/` retain the original optimization passes.

## Allocation and representation

Token registers share immutable arena spans, including copies and local saves.
Their active sources keep reading the original span after replacement. Generated
conversion output writes directly into a reusable, uniquely owned buffer and
publishes one range at completion. Singleton output stays inline. Expanded list
scans splice `\the` token lists and `\unexpanded` directly into their destination;
messages write bytes directly without constructing a temporary token list.

Scalar tables share integer values and group levels in one indexed array.
Assignments resolve one slot for reading, saving, and writing. Prefix scans live
in an ordinary Rust helper; the common unprefixed path carries a small flag word.
Deferred assignment classification runs only when a hook is pending. Release
instructions were inspected with `objdump`; generated-byte emission is inlined.
This host denies hardware performance counters.

The separate allocation diagnostic counts engine construction and execution,
excluding host input bytes, Rust stack, allocator overhead, and pdfTeX memory.
At one million iterations, the five sampled original capture/body workloads use
615–625 allocations and 56,836–58,198 peak requested heap bytes. Tenfold more work
adds at most two allocations. Invocation storage still follows maximum live
nesting and argument size.

The broader suite deliberately replaces definitions and token-register values.
Their arena storage and definition headers accumulate until the run ends:

| Expanded workload     | Allocations at 50,000 | Peak requested engine heap (MiB) |
| --------------------- | --------------------: | -------------------------------: |
| Gentle text templates |                 1,696 |                             38.1 |
| Gentle calendar       |                 1,050 |                             17.1 |
| Gentle contents       |                 5,326 |                             94.7 |
| Gentle records        |                 9,219 |                            144.7 |
| Token pipeline        |                22,692 |                            366.4 |
| Name table            |               187,563 |                             29.6 |
| Optional arguments    |                10,869 |                            170.2 |

These are requested heap peaks, not process RSS. The token pipeline's peak grows
from 74.4 MiB at 10,000 records to 366.4 MiB at 50,000. Its replaced field, expected,
pipeline, and result definitions remain in the session arena. Fast timings do
not establish bounded memory for a long editing session. Name assembly also
allocates temporary vectors for dynamic lookups; the name workload makes that
cost visible. Checkpoint-aware store reclamation remains with the storage work.
Both reports retain the diagnostic binary hash and counts at both sizes.

## Compatibility and reproduction

Twenty-one Rust tests cover the original core plus token-register retention,
protected and opaque list insertion, conditional selection, deferred token order,
case changes, conversion categories, expression rounding/scaling/error order,
and generated-buffer reuse. The native oracle compares 160 marked messages and
23 rejected programs. Expected extended-fixture messages were captured from
native pdfTeX. Storage tests cross 4,096-token chunks, replace active sources,
exercise 5,000 nested captures, and produce multi-chunk conversion text.
Two independent adversarial reviews checked semantics, storage, release paths,
and workload consumers. Debug/release tests, rustfmt, and clippy pass; the portable
library builds for `wasm32-unknown-unknown`.

Measured on 2026-10-07, Linux x86-64, AMD EPYC 7642, pinned to CPU 0:

- pdfTeX 3.141592653-2.6-1.40.25, TeX Live 2023/Debian, with
  `-ini -etex -no-shell-escape -interaction=nonstopmode -halt-on-error`.
- Rust 1.98.1, release profile with thin LTO, one codegen unit, debug info,
  and no additional `RUSTFLAGS`.
- One warmup and seven measured runs per engine at each size: 10,000, 100,000,
  and 1,000,000 core iterations; 1,000, 10,000, and 50,000 expanded records.
- Rotating and reversing the order of Umber, pdfTeX, and a comparison binary.
  Times include startup, input reading, initialization, execution, and result
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

The runners accept `--pdftex /path/to/pdftex` and optional `--baseline` binaries.
They need native pdfTeX, with no format or font installation. The expanded
comparison candidate has the broader primitives and is identified by its
retained binary hash. The native oracle is the checkout-local Ubuntu Noble
TeX Live 2023 installation under `.tools/pdftex`; a system installation works too.
The MVP clock parameters use fixed epoch defaults unless supplied through
`Engine::set_calendar`; corpus fixtures assign deterministic values explicitly.
