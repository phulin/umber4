# Native producer experiment

The producer now uses Rust functions for both macro execution and primitive
scanning. `Consumer`, `Finish`, the suspended-operation vector, and the separate
bounded scalar parser are gone. The cutover uses 11.5% more CPU than the previous
producer across the original eight kernels, while using 19.0% less than the
ordinary interpreter at `29266eb`. It remains on `experiment/producer-driven`.

## Execution and storage

Integer accumulation, optional `=` and `by`, prefixes, definitions, aliases,
messages, and conditional headers keep their progress in function locals.
Assignments commit through the ordinary interpreter's group-aware scalar setter
after scanning and arithmetic succeed. Definitions and messages build directly
into their destination storage.

Execution-time macro calls keep an immutable body owner, token cursor, and one
backed-up token in a native frame. A scanner can read beyond that body's end
through its borrowed caller. Returns discard exhausted frames; tail calls
replace the current body and loop. Current meanings are resolved when tokens are
read.

A macro expanded by a pull scanner can have unread input when the scanner
returns. A reusable vector retains those body ranges, packed conversion digits,
and backed-up tokens. Each entry has its native caller's scope label. This
preserves ordering when scanners consume input from several callers. On return
to execution, an unread body transfers into a native frame without cloning its
owner. Macro tokens stay in their original storage.

The caller link is a pointer to an exclusively borrowed stack frame. The child
finishes before that borrow ends and accesses the parent only through this link.
Pending input contains owners and scope labels, with no parent pointers. A label
comes from the frame address and is used only for equality. Calls drain their
input before a normal return; `\end` and errors terminate reading before any
frame disappears. Owners retain initialized immutable token windows across
definition replacement and arena reuse. The pointer operations and their
validity conditions live in [producer_input.rs](../crates/umber/src/producer_input.rs).

Scanners batch digits and literal operators directly from retained windows or
source bytes, continuing through the same scanner at boundaries. Decimal
conversion packs an `i32` into a `u64`, including its sign, without allocating
output tokens. Native macro depth and pending-input entries are bounded at 128;
recursive scanning is bounded at 256, including nested `\number` and `\the`.
Depth travels in call arguments.

## Implemented slice

The slice covers parameter-free `\def`/`\gdef`, `\let`, `\global`, `\long`,
braces and explicit groups, `\count`, `\catcode`, `\endlinechar`, count
arithmetic, signed decimal operands, internal count reads, `\number`,
`\the\count`, `\message`, `\iftrue`, `\iffalse`, `\ifnum`, `\else`, `\fi`,
`\relax`, `\par`, and `\end`.

Parameter matching, expanded definitions, expressions, token registers, deferred
tokens, other number syntaxes, and other MVP primitives remain outside this
experiment. Unsupported commands return an error. This prototype implements its
supported slice's semantics and checked failures; the full MVP has a broader
command set and error behavior.

`Engine::run_producer` exposes the portable entry point. The `producer` Cargo
feature selects it in the CLI and allocation adapter; `Engine::run` retains the
ordinary interpreter. Checkpoints remain outside this executable experiment.
Their design boundary is paragraph end with no open groups.

## Measurements

Ordinary release builds use fat LTO, one codegen unit, no PGO, and unchanged
dependencies. Each kernel has one warm round and nine measured rounds on one AMD
EPYC 7642 CPU. The runner rotates/reverses the current producer, preceding
producer at `6a6ca50`, ordinary interpreter at `29266eb`, and native pdfTeX
1.40.25. Every engine receives identical source and validates its checksum.
Median CPU times include process startup, file input, execution, and output;
startup is also measured separately without subtraction. Tests, builds, Miri,
and heap diagnostics finish before timing.

The unchanged five core kernels use 2,000,000 iterations; handoff, tail-loop, and
nested-body kernels use 1,000,000.

| Kernel                        | CPU vs. `29266eb` | CPU vs. `6a6ca50` | CPU / pdfTeX |
| ----------------------------- | ----------------: | ----------------: | -----------: |
| Literal execution             |            -19.7% |             -9.6% |        0.288 |
| Macro bodies                  |            -21.6% |            +26.9% |        0.362 |
| Stored integer operands       |            -20.9% |            +14.6% |        0.268 |
| Groups                        |            -17.2% |            +28.1% |        0.343 |
| Source integer operands       |            -16.3% |             +2.9% |        0.332 |
| Scanner completes inside body |            -10.9% |            +10.9% |        0.377 |
| Conditional tail loop         |            -17.3% |             -1.9% |        0.334 |
| Eleven nested non-tail bodies |            -26.9% |            +26.8% |        0.513 |

The geometric mean of the eight CPU ratios is 0.810 against the original
interpreter, 1.115 against the previous producer, and 0.346 against native
pdfTeX. Macro bodies and groups regress by approximately 27–28% against the
preceding producer.

Three new scanner workloads are separate from that eight-kernel comparison.
Nested conversion uses 500,000 iterations; repeated definitions and messages
use 100,000 each. Definitions replace a body with 32 inert tokens and execute
its unread suffix. Messages expand nested braces and a changing count value.

| Kernel               | CPU vs. `29266eb` | CPU vs. `6a6ca50` | CPU / pdfTeX |
| -------------------- | ----------------: | ----------------: | -----------: |
| Nested conversions   |             -6.3% |            +10.2% |        0.396 |
| Repeated definitions |             -4.4% |            -12.7% |        0.349 |
| Expanded messages    |             +4.6% |             -0.3% |        0.462 |

The message workload includes retained output vectors and CLI output; native
pdfTeX writes its terminal and log streams. A ten-message control checks each
complete message against independently expected text in all four engines.
All eleven kernels beat native pdfTeX. They exercise the supported
expansion/register slice; whole-document, full-MVP, and Wasm runtime performance
remain unmeasured on this path.

The producer contains 912 production lines, down from 967. Shared input/setter
helpers add 32 lines, leaving the crate 23 production lines smaller overall.
These are physical lines including comments/blanks, excluding test files and
`cfg(test)` items. The main gain is removing scanner phases and their transitions.

On the measured native build, a pending-input record is 48 bytes. Macro entry and
resumption each use a 192-byte native activation, including saved registers and
return address, versus 208 bytes for the preceding producer. Outlined recursive
integer scanning uses 96 bytes; expansion and parent reads add transient frames.
The million-call tail loop uses one macro activation; the nested kernel reaches
eleven. Native text/rodata size grows 4.8% with the inlined scanner code.

All eleven workloads match the preceding producer's allocation request counts.
The reusable pending-input vector adds 96 requested and peak live bytes: its
first allocation holds four 48-byte entries, replacing four 24-byte consumer
records. Ordinary macro execution allocates no heap per call. Both entries
still construct the shared `Engine`, including its unused ordinary source-vector
capacity. Heap counters exclude host input, native stack, and allocator metadata.

The [native scanner report](../benchmarks/results/producer-native-scanners.json)
retains raw samples, exact comparison-driver source, compiler/source/binary
identities, allocation/depth diagnostics, native prologues, and sixteen
exploratory candidates. Earlier implementations remain documented in the
[handoff report](../benchmarks/results/producer-handoff.json),
[scalar-parser report](../benchmarks/results/producer-scalars.json),
[delivery report](../benchmarks/results/producer-optimized.json), and
[original report](../benchmarks/results/producer-experiment.json).

## Validation and reproduction

All 85 Rust tests pass in debug and release, including 25 producer tests. Eight
selected tests pass under Miri, covering caller reads, live-body replacement,
conversion ordering, early end, recursion limits, and operands/keywords crossing
retained pages. The two shared fixtures match all 25 native pdfTeX messages.
Rustfmt, clippy across all targets/features, and the release Wasm library build
pass.

Build `29266eb` without `producer` in a separate checkout. Then run from this
experimental checkout:

```bash
cargo build --locked --release --features producer --bin umber --examples
python3 scripts/oracle.py fixtures/mvp/producer-handoff.tex fixtures/mvp/native-scanners.tex
target/release/examples/producer_stats fixtures/mvp/native-scanners.tex
python3 benchmarks/mvp.py --cases raw bodies integers groups source-integers --sizes 2000000 --repeats 9 --baseline /path/to/baseline/umber
python3 benchmarks/producer.py --cases handoff tail-loop nested-bodies --sizes 1000000 --repeats 9 --baseline /path/to/baseline/umber
python3 benchmarks/producer.py --cases conversions --sizes 500000 --repeats 9 --baseline /path/to/baseline/umber
python3 benchmarks/producer.py --cases definitions messages --sizes 100000 --repeats 9 --baseline /path/to/baseline/umber
```

For a paired comparison against the preceding producer, build `6a6ca50` with
`producer` and pass its executable as `--baseline`. The retained four-engine run
includes both Rust baselines together using the same measurement helper.
