# Native expansion benchmarks

`mvp.py` retains the original eight core workloads and adds `source-integers`.
That workload reads varied one- to ten-digit decimal operands directly from
source bytes. Each value is compared with an independently stored expected
operand before incrementing the checksum. The original `integers` workload
reads its repeated decimal operands from macro tokens. `expansion.py` exercises
nine larger macro workloads, including excerpts from Michael Doob's
[gentle.tex](https://ctan.net/info/gentle/gentle.tex). Both runners execute the
same complete input in Umber and native `pdftex -ini -etex`, validate every run,
alternate engine order, pin a CPU on Linux, and retain raw timings and identities.
On Unix, reports also retain each child's user-plus-system CPU time. Wall time
includes scheduling waits; the CPU samples help identify interference from other
work on a shared host. Console summaries use CPU time when available and label
the selected clock. Both clocks include the complete engine process.

```bash
cargo build --release
python3 benchmarks/mvp.py --report target/benchmark/core.json
python3 benchmarks/expansion.py --report target/benchmark/expansion.json
```

Use `--cases`, `--sizes`, and `--repeats` for focused runs. Both accept
`--pdftex /path/to/pdftex` and `--baseline /path/to/previous/umber`; the expanded
suite's baseline must support its primitive set. Default reports live under
`target/`. Reviewed measurements live in `results/`.

The [shared-token-store comparison](results/unified-token-storage.json) measures
one pool for definitions, captures, and generated output against `dc64762`.
It retains largest-size CPU samples, longer confirmations, both memory workloads
at three sizes, and allocation diagnostics, with source and executable identities.
The paired builds use the restored custom name table and unchanged dependencies.

The [structural cleanup comparison](results/structural-simplification.json)
retains largest-size timings, same-LTO controls, longer confirmations, and
allocation diagnostics. The release profile now uses fat LTO; earlier reports
retain the settings of their measured builds.

The [name/scanner comparison](results/name-scanner-simplification.json) retains
the `c154e34` experiment against `e0007e5`. Its timings include the subsequently
reverted `hashbrown` table and describe that historical build.

An optional native profile build uses the same generators for compiler training:

```bash
rustup component add llvm-tools
python3 benchmarks/pgo.py
python3 benchmarks/mvp.py --umber target/pgo/use/release/umber
python3 benchmarks/expansion.py --umber target/pgo/use/release/umber
```

Training checks every workload's result at 75,031 core iterations and 7,003
expanded records, then runs the native oracle. The final build passes the oracle
again. Artifacts and input/compiler identities stay under `target/pgo`; the
ordinary release and Wasm builds use their existing paths. The profile builder
also produces `target/pgo/use/release/examples/allocations` for diagnostics.
Its LLVM inlining settings and 64-byte function alignment are measured choices
for the recorded compiler. The builder enables `native-profile`, which lets the
trained compiler choose whether to inline physical-line loading. Ordinary and
Wasm builds keep that loader in a separate function. The feature also allows
compiler-selected argument-reset inlining and keeps name lookup behind a measured
Rust call boundary. It reads published spelling entries directly; ordinary
builds retain the measured reset call and checked spelling reads.
Stored-list scans in profile builds classify spaces, letters, and other
characters through one contiguous token range, with the existing checks for
all remaining words. Ordinary builds retain their previous classification path.
Grouped stored-argument scans test four immutable words together before
advancing, falling back to their scalar tail for structural words and window
ends. Native release code uses SIMD comparisons for that read. The retained
[definition-storage comparison](results/definition-storage-followup.json) includes
allocation diagnostics and nine paired repetitions against the preceding profile
build, including its CPU regressions.
Profile delimiter matching caches the immutable failure-table cursor in its Rust
frame, retaining checked fallback reads. The
[delimiter comparison](results/profile-delimiter-cursor-followup.json) records
nine paired repetitions, unchanged allocation samples, and execution-text
identities for the profile change and ordinary rebuild.
Interned spelling bytes share stable session-owned chunks. The
[spelling-store comparison](results/name-spelling-arena-followup.json) retains
nine paired repetitions for all eighteen workloads, 21-repetition confirmations
for selected gains and regressions, and ordinary/profile allocation diagnostics.
The current full sweeps in `results/` compare reclaimable immutable values against
the spelling-store build; the earlier follow-ups retain their original identities.
Compare both suites when changing these settings or updating Rust.

| Expanded workload    | Measured behavior                                                                                                                                                |
| -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gentle-text`        | Original spelling switches/aliases and `\talks` template, varied names and both branches of every spelling definition.                                           |
| `gentle-calendar`    | Original `\today`, all twelve month branches, varied days and years.                                                                                             |
| `gentle-contents`    | Original contents write payload evaluated in `\edef`; positive and negative page numbers, varied expanded titles, string/roman conversion and detokenization.    |
| `gentle-records`     | Original bracketed-title and paragraph-delimited signature; adapted consumer retains and expands both full payloads, including nested groups.                    |
| `stored-records`     | The same Gentle record consumer, alternating input from immutable macro bodies and forwarded captured arguments.                                                 |
| `token-pipeline`     | Shared token-register values, group restoration, a deferred assignment hook, opaque list insertion into `\edef`, and later expansion after a field redefinition. |
| `name-table`         | 64–4,096 dynamically defined names, defined/missing tests, lookups, and periodic definition replacement.                                                         |
| `optional-arguments` | `\futurelet`/`\unless` dispatch between optional and default arguments, delimiter matching, integer expressions, and source/stored argument routes.              |
| `numeric-references` | 256 numbered names, generated decimal input in `\ifcsname` and `\csname`, and direct decimal emission in `\edef`.                                                |

`--cases register-meanings` selects a supplemental conversion workload. It varies
count and token-register alias indices across 0–255 and uses normal, changed,
and disabled escape characters. Every `\meaning` serialization is compared with
an independently generated token literal. This case is separate from the default
nine-workload sweep and profile training.

The [ordinary report](results/register-meanings-native.json),
[profile report](results/register-meanings-native-pgo.json), and
[profile coverage comparison](results/register-meaning-coverage-followup.json)
retain this baseline. Instrumented oracle runs also feed the profile builder;
the added all-index fixture changes those frequencies without changing runtime
sources or the default eighteen benchmark training inputs.

`numeric-references` uses 256 numbered names with positive and negative register
values from three to ten digits. Each record forms a name in `\ifcsname` and
`\csname`, expands its definition, and appends decimal text. The name scanners
consume generated numeric input; the trailing number is emitted directly into
`\edef`. Both paths are checked against independently generated expected text.
This synthetic case supplements the Gentle excerpts and other macro kernels.

These kernels consume text in expanded definitions. Fonts, boxes, mathematics,
paragraph building, and output are outside the measured slice. The full manual
requires those subsystems. There are no placeholder typesetting primitives.

Every expanded record is compared with independently generated expected tokens
using `\ifx`. Contents serialization compares detokenized text; dedicated native
fixtures separately check conversion categories. A record increments the final
checksum only after equality succeeds. Expected-definition construction and
comparison are included in timing. Payloads vary from 8 to 1,024 characters;
`expanded_text_bytes` counts final validated text, not intermediate storage or
total interpreter work. Umber reclaims replaced definitions and token lists after their last semantic
owner disappears. Allocation diagnostics report the backing chunks retained by
live values and the bounded empty-chunk cache.

The selected definitions are retained in
[gentle-kernels.tex](corpus/gentle-kernels.tex). [gentle.json](corpus/gentle.json)
records the original URL, SHA-256, exact line ranges, and each adaptation.
The generators work offline. To inspect the original source separately:

```bash
mkdir -p .tools/corpus
curl -fL https://ctan.net/info/gentle/gentle.tex -o .tools/corpus/gentle.tex
sha256sum .tools/corpus/gentle.tex
```

Expected source hash:
`b3e1acf4a6feb6a13b0ace279a5be7f1546ac2198452188f6fb22b4eb590ba3f`.
Each report also records the selected-kernel, generator, measurement-runner,
generated-input, and executable hashes. Exact input files remain under
`target/benchmark/expansion/` after running the generator.

`memory.py` adds compact-source lifetime workloads. `churn` repeatedly replaces
fixed names and token registers, retains aliases, and saves/restores local values.
`live-growth` retains a new definition on each iteration. Both validate their
results in TeX and time the same source in both interpreters. Churn defaults to
1,000, 10,000, and 50,000 iterations; live growth uses 100, 1,000, and 2,500 to fit
the native INITEX oracle's configured memory capacity. The input stays compact
while execution grows. Its allocation adapter excludes the host input buffer and
records requested engine heap, not process RSS or pdfTeX heap.

```bash
cargo build --release --example allocations
python3 benchmarks/memory.py --report target/benchmark/memory.json
```

Use `--baseline` and `--baseline-allocations` for a paired earlier interpreter and
its diagnostic adapter. `--sizes` overrides the workload defaults. Memory pools
retain at most sixteen empty 4,096-token chunks plus the active bump chunk;
live immutable values retain their occupied chunks. Small surviving values may
retain unused parts of those chunks, so live-growth measurements also expose
fragmentation and per-value metadata overhead.

Measure allocations with the separate adapter; its counters stay out of timing:

```bash
cargo build --release --example allocations
target/release/examples/allocations target/benchmark/expansion/gentle-records-50000.tex
```

This counts requests during engine construction and execution. Host input bytes,
Rust stack, allocator overhead, and native pdfTeX memory are outside that counter.
