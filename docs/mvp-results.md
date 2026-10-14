# Native expansion measurements

The full three-size sweeps below measure immutable-value reclamation at
`a7f8be0`. A later representation simplification is measured separately in the
[follow-up report](../benchmarks/results/simplification-followup.json). The
structural simplification below compares the next cleanup with `d8a8c28`.
The historical name/scanner comparison records `c154e34` before its name-table
change was reverted.

The [original MVP](mvp.md) and [broader expansion slice](expansion-mvp.md) implement
63 primitive names, covering parsing, expansion, macros, groups, integer state,
and token registers. Immutable definitions and token lists now follow semantic
ownership and reuse their backing storage. The reclamation build peaked at
96 KiB through 50,000 replacements, versus 83 MiB before reclamation.

In the reclamation sweeps, ordinary release uses 0.08–0.62 times native pdfTeX's CPU
time across nine core workloads and 0.22–0.44 times across nine broader macro
workloads. Reclamation changes aggregate CPU time by +21.6% on the core
suite and -10.0% on the broader suite, using geometric means of paired
ratios. Individual costs and gains appear below. Layout, fonts, mathematics,
shipout, format files, checkpoints, and Wasm runtime timing remain outside this
implemented slice.

## Name and scanner simplification

The custom packed name table is restored, with no added dependencies. The dense
spelling directory still removes 514 placeholder descriptors. Short and long
expanded names share one scanner; long names spill once from stack storage.
Macro equality and meaning conversion read the complete definition value directly,
while body cursors retain their cached executable range.

These retained changes remove 23 production Rust lines relative to `e0007e5`:
3,916 to 3,893. Across both simplification passes, application-owned Rust and
Python drop from 5,301 to 4,852 physical non-test lines, a reduction of 449 (8.5%).

The [historical comparison report](../benchmarks/results/name-scanner-simplification.json)
measures `c154e34`, which used `hashbrown`. Its CPU and heap measurements describe
that earlier build; the restored implementation has not been retimed.

## Structural simplification

The next cleanup reduces library/CLI Rust from 4,285 to 3,916 physical lines,
excluding separate test modules. The unchanged allocation adapter adds 70 lines
to each. Benchmark and oracle tooling drops from 946 to 889 Python lines:
5,301 to 4,875 lines across the reviewed non-test code, a reduction of 426 (8.0%).

One command inventory now supplies primitive registration, meaning text, and
assignment/conditional classification. Grouped and delimited arguments share a
scanner specialized at compile time. Generated output retains its buffer
directly, removing the argument packets previously needed for conversions and
decimal output. One token cursor serves generated, value-backed, and argument
input, reducing native source records from 80 to 72 bytes. Immutable parameter
slices supply their own arity and eliminate the optional metadata wrapper;
delimiter ranges come from the stored signature. Numeric scanners share their
suffix loop and decimal emitter. Benchmark suites share setup and reporting,
with unchanged generated inputs and expected results.

The release profile changes from thin to fat LTO, keeping one codegen unit and
the same debug setting. The retained measurements distinguish that complete
before/after comparison from a control rebuilding `d8a8c28` with fat LTO. These
are ordinary native builds without PGO. The profile tables later in this document
remain measurements of the older reclamation build.

The [comparison report](../benchmarks/results/structural-simplification.json)
retains seven repetitions at 1,000,000 core iterations and 50,000 broader records,
21-repetition confirmations, nine paired repetitions for the same-LTO control,
and separate allocation diagnostics. These runs use the same host and compiler
described below. CPU changes are geometric means of the nine cases in each suite;
the supplemental register-meaning case is reported separately.

| CPU comparison                    | Core suite | Broader suite |
| --------------------------------- | ---------: | ------------: |
| Complete change, thin to fat LTO  |      -8.7% |         -1.5% |
| Source change, both using fat LTO |      -3.3% |         -1.8% |

Individual regressions remain. The longer confirmations measure +3.0% for Gentle
records, +8.0% for the token-register pipeline, and +5.0% for optional arguments.
The seven-run sweep measures +10.4% for optional arguments; both sets of raw
samples are retained. Integer assignments improve 5.7%, nested parameters 7.5%,
and register-meaning conversion 5.5% in the confirmations. All nineteen kernels
still use less CPU than native pdfTeX in the full comparison. These measurements
cover the implemented expansion/register slice.

Fixed-working-set churn retains exactly the baseline allocation counts, requested
bytes, and 98,396-byte heap peak through 50,000 replacements; CPU increases 7.4%
at that size. At 2,500 retained definitions, live-growth CPU decreases 3.2%, with
a 2,038,732-byte peak versus 2,039,164 bytes. Register-meaning conversion drops
from 200,889 to 100,883 allocation requests at 50,000 records by emitting alias
indices directly. Heap figures exclude host input, stack, and allocator overhead.

The final source passes 55 debug/release Rust tests, rustfmt, Clippy with warnings
denied, and the release Wasm library build. The native oracle matches 747 messages
and 26 rejection cases. Seven selected arena, cursor-retention, and failure-progress
tests pass Miri with `native-profile`. Generated inputs, expected results, and
workload metadata match the preceding generators for all nineteen cases at three
sizes.

## Representation simplification follow-up

The 2026-10-13 follow-up removes cached token-register spans, register levels from
internal token-list quantities, and the redundant macro-body source variant.
Macro-body installation takes its range directly from the retained definition;
argument-source construction drops an unused parameter. Expression scanning uses
the existing expansion-depth guard. These changes preserve storage ownership and
build tokens in the same destinations.

Seven paired repetitions at the largest size of all eighteen cases measure 2.2%
less core CPU and 0.6% more broader CPU, using geometric means against `a7f8be0`.
The broader change is within the earlier control-run variation. Individual gains
and regressions remain in the report: stored macro calls improve 11.4%, while
21-repetition follow-ups measure 7.8% more raw-dispatch CPU, 6.7% more discarded
long-argument CPU, and 3.5% more calendar CPU. All cases still use less CPU than
pdfTeX in the seven-repetition comparison. Allocation counts match the baseline
in all eighteen diagnostics; register and group-save records carry less data.

The follow-up covers ordinary release only. The native profile tables below
remain measurements of the preceding reclamation build. The simplified source
passes the same 53 debug/release tests, 747 native messages, 26 rejection cases,
Clippy, and Wasm build. Active-definition/forwarded-range and register-replacement
fixtures also pass Miri.

## What is measured

The reclamation sweeps were measured on 2026-10-09, Linux x86-64, AMD EPYC 7642,
pinned to CPU 0.
The oracle is pdfTeX 3.141592653-2.6-1.40.25 (TeX Live 2023/Debian), running INITEX
with e-TeX enabled.
Ordinary release uses Rust 1.98.1, thin LTO, one codegen unit, debug information,
and no extra Rust flags.

Every engine receives the same complete source. Each size has one warmup and
seven measured runs, with rotating/reversing engine order. CPU figures are child
user-plus-system medians for the complete process: startup, input reading,
initialization, execution, result output, and cleanup. pdfTeX also writes its
normal log. Wall time and raw samples remain in the reports; the shared host can
add scheduling delays. The measurement driver kept builds, Miri, and allocation
diagnostics outside timed executions.

The CLI reads its input into memory before interpretation. The historical
`file-parameters` case therefore measures tokenization over resident bytes,
including the initial read in process timing. Core workloads use compact macro
fanout except for raw dispatch, source arguments, and source integers. The
256-token argument case discards its payload after scanning; it is particularly
favorable to range-based input and predicts little about full document speed.

The broader suite contains selected/adapted Gentle macros and synthetic consumers,
not the complete manual. Each record builds an expanded definition, constructs
independently generated expected tokens, and compares them with `\ifx` before
incrementing the checksum. Those validation operations are timed. Contents
serialization compares detokenized text; native fixtures separately verify token
categories. Payloads vary from 8 to 1,024 characters. The
[benchmark guide](../benchmarks/README.md) and [corpus provenance](../benchmarks/corpus/gentle.json)
record the consumers and adaptations.

The comparison binary is the preceding spelling-store implementation at
`3f48a59`, using ordinary release for ordinary comparisons and its previously
trained native build for profile comparisons. The reports retain input,
generator, executable, and Rust-source hashes. Earlier optimization reports
remain in `benchmarks/results/` and Git history. Copies of an identical binary
previously differed by 1.5–2.3% in CPU medians; see the
[control samples](../benchmarks/results/identical-binary-control.json).

## Ordinary release CPU

Core medians at one million iterations:

| Workload                          | Umber CPU (s) | pdfTeX CPU (s) | Umber/pdfTeX | CPU change vs previous Umber |
| --------------------------------- | ------------: | -------------: | -----------: | ---------------------------: |
| Raw byte dispatch                 |         0.210 |          0.563 |        0.372 |                        +1.9% |
| Stored macro bodies               |         0.158 |          0.267 |        0.590 |                       +47.5% |
| Integer assignments               |         0.149 |          0.370 |        0.403 |                       +30.2% |
| Repeated/nested parameters        |         0.396 |          0.639 |        0.620 |                       +27.1% |
| Arguments tokenized from bytes    |         0.443 |          0.783 |        0.566 |                       +11.2% |
| Repeated 256-token arguments      |         0.666 |          8.645 |        0.077 |                       +25.2% |
| Overlapping delimiters            |         0.170 |          0.276 |        0.614 |                       +36.0% |
| Local/global grouped writes       |         0.259 |          0.540 |        0.480 |                       +16.0% |
| Varied integers from source bytes |         0.756 |          1.722 |        0.439 |                        +6.1% |

Broader medians at 50,000 records:

| Workload                           | Umber CPU (s) | pdfTeX CPU (s) | Umber/pdfTeX | CPU change vs previous Umber |
| ---------------------------------- | ------------: | -------------: | -----------: | ---------------------------: |
| Gentle spelling and templates      |         0.136 |          0.315 |        0.431 |                        -2.7% |
| Gentle calendar formatter          |         0.118 |          0.288 |        0.410 |                        +1.7% |
| Gentle contents serialization      |         0.369 |          1.038 |        0.356 |                        -9.4% |
| Gentle paragraph-delimited records |         0.364 |          1.280 |        0.284 |                       -18.5% |
| Gentle records from stored tokens  |         0.369 |          1.518 |        0.243 |                       -21.2% |
| Token-register pipeline            |         0.532 |          2.439 |        0.218 |                       -28.1% |
| Dynamic name table                 |         0.144 |          0.329 |        0.437 |                       +12.2% |
| Optional-argument parser           |         0.370 |          1.594 |        0.232 |                       -19.1% |
| Numbered references                |         0.146 |          0.331 |        0.442 |                        +2.7% |

Taking differences between the two largest sizes gives Umber/pdfTeX CPU ratios
of 0.08–0.63 for the core suite and 0.22–0.45 for the broader suite.
This removes most fixed startup advantage. These remain expansion/scalar
workloads on one native host; they establish neither full-typesetting speed nor
Wasm runtime speed. The current benchmark families have also received extensive
optimization, so independent document workloads remain valuable.

Full three-size sweeps and paired samples are retained in
[mvp-native.json](../benchmarks/results/mvp-native.json) and
[expansion-native.json](../benchmarks/results/expansion-native.json).

## Reclamation and memory

Bindings, `\let` aliases, group saves, active input sources, and retained argument
ranges own complete immutable values. Replacement releases the old owner; group
save/restore moves it. An executing macro acquires its owner before argument
scanning can retire an enclosing source. Token-register input retains its value
through replacement. Macro tokens contain control-sequence identities, so
recursive definitions do not form owner cycles. Interned spellings and identities
remain available for the engine/session lifetime; process sharing comes with the
later runtime architecture.

Builders write directly into stable 4,096-token chunks. Publication transfers
those chunks and metadata without flattening or copying the token payload.
Unused reservation tails return immediately when no later builder has reserved
space after them. Empty chunks enter a bounded cache of sixteen chunks, with one
additional active bump chunk. Live values retain their occupied chunks. Value IDs
are reused after their last owner disappears; the weak descriptor table retains
its high-water capacity, including weak owner headers until reuse. Non-atomic
reference counts change at ownership transitions, while token loops advance plain
pointers. The inspected native append path stores one token, advances the cached
write pointer, and updates its bounds; exhausted reservations call the separate
growth helper. The
[storage design](storage.md) documents layout, publication, and reclamation.

The compact-source memory workloads execute their loop in TeX, keeping host input
under 1 KiB. `churn` overwrites a fixed set of definitions and token registers,
retaining aliases and exercising local/global restoration. `live-growth` retains
a distinct definition each iteration. Both validate results and time identical
source in Umber and pdfTeX. Live-growth defaults stop at 2,500 definitions to fit
the native INITEX oracle's configured memory capacity.

| Workload    | Iterations | Current peak (KiB) | Previous peak (KiB) | CPU change vs previous | Umber/pdfTeX CPU |
| ----------- | ---------: | -----------------: | ------------------: | ---------------------: | ---------------: |
| churn       |      1,000 |               96.1 |              1815.7 |                  -8.3% |            0.363 |
| churn       |     10,000 |               96.1 |             17294.7 |                 -18.9% |            0.416 |
| churn       |     50,000 |               96.1 |             84582.7 |                 -16.4% |            0.426 |
| live-growth |        100 |              123.5 |               109.9 |                 +14.3% |            0.293 |
| live-growth |      1,000 |              815.3 |               735.0 |                 +20.5% |            0.429 |
| live-growth |      2,500 |             1991.4 |              1883.6 |                 +25.4% |            0.476 |

These are peak requested engine heap bytes measured by a separate allocation
adapter, excluding host input, stack, allocator overhead, and pdfTeX memory.
They are not RSS or a separate measurement of live token payload. CPU samples
come from the ordinary CLI with no counting allocator. The
[memory report](../benchmarks/results/memory-native.json) retains all samples,
allocation totals, and source identities.

The broader suite also shows whether reusing storage changes the original
memory-heavy comparisons:

| Expanded workload, 50,000 records  | Current peak (MiB) | Previous peak (MiB) | Current allocations | Previous allocations |
| ---------------------------------- | -----------------: | ------------------: | ------------------: | -------------------: |
| Gentle spelling and templates      |              0.095 |                24.1 |             152,082 |                1,088 |
| Gentle calendar formatter          |              0.094 |                10.1 |             100,816 |                  436 |
| Gentle contents serialization      |              0.096 |                80.6 |             159,324 |                4,710 |
| Gentle paragraph-delimited records |              0.105 |               137.6 |             117,128 |                8,612 |
| Gentle records from stored tokens  |              0.105 |               169.7 |             146,239 |               10,672 |
| Token-register pipeline            |              0.115 |               352.2 |             394,016 |               22,075 |
| Dynamic name table                 |             19.225 |                22.6 |             110,598 |                1,239 |
| Optional-argument parser           |              0.105 |               163.1 |             145,412 |               10,254 |
| Numbered references                |              0.128 |                10.3 |             101,114 |                  456 |

[Allocation diagnostics](../benchmarks/results/reclamation-allocations.json)
retain all nine cases at 1,000, 10,000, and 50,000 records. Allocation counts can
increase even while peak space falls: the MVP still heap-allocates whole-value
owners and chunk descriptors. Small surviving values can pin partially occupied
chunks, and growing live state pays this metadata overhead. The name-table cases
retain 125, 1,250, and 4,096 bindings across the three sizes. Their peak also
includes chunks pinned by infrequently replaced definitions; the 19.2 MiB result
shows that this allocator still has substantial fragmentation in that workload.
The live-growth
control makes that cost visible rather than attributing every reduction to a
smaller live workload. Checkpoint retention has not yet been implemented or
measured; retained checkpoint roots will keep their values alive.

## Optional native profile build

The profile build uses 0.07–0.52 times pdfTeX CPU on the core suite and
0.22–0.39 on the broader suite. Against the previous profile build, geometric
mean CPU changes are +20.7% and -15.8%, respectively.
These paired comparisons use the same build type on each side.

Core entries use one million iterations; broader entries use 50,000 records:

| Workload                           | Profile CPU (s) | Profile/pdfTeX | CPU change vs previous profile build |
| ---------------------------------- | --------------: | -------------: | -----------------------------------: |
| Raw byte dispatch                  |           0.184 |          0.311 |                                +7.1% |
| Stored macro bodies                |           0.141 |          0.514 |                               +43.6% |
| Integer assignments                |           0.127 |          0.342 |                               +19.7% |
| Repeated/nested parameters         |           0.328 |          0.517 |                               +31.0% |
| Arguments tokenized from bytes     |           0.361 |          0.444 |                               +13.0% |
| Repeated 256-token arguments       |           0.609 |          0.071 |                               +16.5% |
| Overlapping delimiters             |           0.140 |          0.511 |                               +43.0% |
| Local/global grouped writes        |           0.237 |          0.431 |                               +19.2% |
| Varied integers from source bytes  |           0.633 |          0.359 |                                +0.1% |
| Gentle spelling and templates      |           0.121 |          0.385 |                                -1.7% |
| Gentle calendar formatter          |           0.096 |          0.348 |                                +3.0% |
| Gentle contents serialization      |           0.349 |          0.340 |                                -8.3% |
| Gentle paragraph-delimited records |           0.342 |          0.271 |                               -26.7% |
| Gentle records from stored tokens  |           0.362 |          0.230 |                               -30.2% |
| Token-register pipeline            |           0.552 |          0.228 |                               -37.7% |
| Dynamic name table                 |           0.122 |          0.371 |                                +3.2% |
| Optional-argument parser           |           0.351 |          0.222 |                               -30.2% |
| Numbered references                |           0.121 |          0.372 |                                +0.0% |

Training uses these same eighteen workload families at different sizes, plus
instrumented native-oracle executions. It is not a held-out performance result.
The native-only feature, LLVM inlining choices, and function alignment are
recorded in [native-pgo-build.json](../benchmarks/results/native-pgo-build.json).
The final profile binary passes the native oracle. Full timing samples are in
[mvp-native-pgo.json](../benchmarks/results/mvp-native-pgo.json) and
[expansion-native-pgo.json](../benchmarks/results/expansion-native-pgo.json).

## Compatibility and reproduction

At `a7f8be0`, all 53 Rust tests pass in debug and release. Rustfmt, Clippy with warnings denied,
and the release `wasm32-unknown-unknown` library build pass. The native oracle
matches 747 marked messages and 26 rejection cases. Coverage includes active
self-redefinition, forwarded durable ranges after producer replacement,
token-register replacement during input, alias retention, group restoration,
and the existing scanner, category, arithmetic, conversion, and delimiter cases.
Five allocation-primitive tests and the active-definition/forwarded-range test
pass Miri with `native-profile`, including interleaved builders, published raw
windows, page/ID reuse, failed builders, bump-tail return, and capacity boundaries.

```bash
cargo test --locked --lib
cargo test --locked --release --lib --features native-profile
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo build --locked --release --bin umber --example allocations
python3 scripts/oracle.py
python3 benchmarks/mvp.py --report target/benchmark/core.json
python3 benchmarks/expansion.py --report target/benchmark/expanded.json
python3 benchmarks/memory.py --report target/benchmark/memory.json
cargo build --locked --release --lib --target wasm32-unknown-unknown
python3 benchmarks/pgo.py
```

Runners accept `--umber`, `--pdftex`, and optional `--baseline` binaries. The memory
runner also accepts `--allocations` and `--baseline-allocations`. Keep executable
snapshots while rebuilding so each paired run measures the intended binaries.
The oracle needs no imported format or font installation. Calendar fixtures use
explicit values; the MVP otherwise starts with fixed epoch defaults.
