# Native producer experiment

Macro execution and primitive scanning use Rust functions; `Consumer`, `Finish`,
the suspended-operation vector, and the separate bounded scalar parser are gone.
Native body execution now keeps its cursor local between calls. This follow-up
uses 8.6% less CPU than the scanner cutover at `36e8db9` across the original eight
kernels, leaving a 3.1% increase against the preceding producer at `6a6ca50`.
It uses 24.5% less CPU than the ordinary interpreter at `29266eb` and remains on
`experiment/producer-driven`.

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

Native body execution keeps its window cursor local. It publishes that cursor
before entering a scanner or child macro, then reloads it after a child can have
consumed caller input. Ordinary child calls drain their own pending input and
create no input in their caller's scope. Body execution therefore continues
directly after those calls, skipping inert tokens in the same loop. General
input routing runs after a primitive can have introduced pending input or a
backed-up token. Root file execution keeps its existing loop; the choice is a
compile-time parameter. Scanners retain one implementation across window and
caller boundaries.

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
EPYC 7642 CPU. The runner rotates/reverses five engines: current producer,
scanner cutover at `36e8db9`, producer with consumers at `6a6ca50`, ordinary
interpreter at `29266eb`, and native pdfTeX 1.40.25. Every engine receives
identical source and validates its checksum. Median CPU times include process
startup, file input, execution, and output. Startup is measured separately
without subtraction. Tests, builds, Miri, and heap diagnostics finish before
timing.

The unchanged five core kernels use 2,000,000 iterations; handoff, tail-loop, and
nested-body kernels use 1,000,000.

| Kernel                        | CPU vs. `36e8db9` | CPU vs. `6a6ca50` | CPU / pdfTeX |
| ----------------------------- | ----------------: | ----------------: | -----------: |
| Literal execution             |             -5.0% |            -11.7% |        0.278 |
| Macro bodies                  |            -26.6% |             -6.7% |        0.262 |
| Stored integer operands       |             -6.2% |             +6.9% |        0.256 |
| Groups                        |             -6.3% |            +23.0% |        0.328 |
| Source integer operands       |             -2.9% |             +1.3% |        0.312 |
| Scanner completes inside body |             +4.9% |            +16.3% |        0.401 |
| Conditional tail loop         |             -7.8% |             -5.5% |        0.320 |
| Eleven nested non-tail bodies |            -15.0% |             +5.9% |        0.431 |

The geometric mean of the eight CPU ratios is 0.914 against the cutover, 1.031
against the preceding producer, 0.755 against the original interpreter, and
0.318 against native pdfTeX. Macro bodies now beat the preceding producer by
6.7%; nested bodies retain a 5.9% regression. Groups and handoffs remain the
largest regressions against that producer. Handoffs also cost 4.9% more than the
cutover. Local body execution recovers most of the cutover's aggregate CPU cost;
operand scans retain their general input routing.

Three supplemental scanner workloads remain separate from that comparison.
Nested conversion uses 500,000 iterations; repeated definitions and messages
use 100,000 each. Definitions replace a body with 32 inert tokens and execute
its unread suffix. Messages expand nested braces and a changing count value.

| Kernel               | CPU vs. `36e8db9` | CPU vs. `6a6ca50` | CPU / pdfTeX |
| -------------------- | ----------------: | ----------------: | -----------: |
| Nested conversions   |             -3.5% |             +7.8% |        0.395 |
| Repeated definitions |             -8.2% |            -19.2% |        0.335 |
| Expanded messages    |             -1.6% |             -0.7% |        0.453 |

The message workload includes retained output vectors and CLI output; native
pdfTeX writes terminal and log streams. A ten-message control checks complete
text against independently expected messages in all five engines. All eleven
kernels beat native pdfTeX. These measurements cover the supported
expansion/register slice; whole-document, full-MVP, and Wasm runtime performance
remain unmeasured.

The producer contains 972 production lines, up 60 from the scanner cutover and
five from `6a6ca50`. Physical counts include comments/blanks and exclude test
files and `cfg(test)` items. The change adds one direct body loop and a
compile-time choice between native body and root execution. Scanners and input
representation are unchanged.

On the measured native build, pending-input records remain 48 bytes. Native
macro entry/resumption remain 192 bytes each, including saved registers and
return address; the producer with consumers used 208. Outlined integer scanning
uses 96 bytes; expansion and parent reads add transient frames. The million-call
tail loop uses one macro activation; the nested kernel reaches eleven. Generated
code holds the body cursor in a register and publishes it at child/scanner
boundaries. Native text/rodata size grows 0.7% from the cutover.

All eleven workloads exactly match the cutover in allocation requests, requested
bytes, peak live bytes, and diagnostic call/depth/input counts. Ordinary macro
execution allocates no heap per call. The reusable pending-input vector still
adds 96 fixed requested/peak bytes relative to `6a6ca50`. Both entries construct
the shared `Engine`, including unused ordinary source-vector capacity. Heap
counters exclude host input, native stack, and allocator metadata.

The [body cursor report](../benchmarks/results/producer-body-cursor.json) retains
raw samples, exact comparison-driver source, compiler/source/binary identities,
allocation/depth diagnostics, native instructions, and eleven exploratory
variants. The accepted executable exactly matches the selected exploratory
candidate. Earlier results remain in the
[native scanner report](../benchmarks/results/producer-native-scanners.json),
[handoff report](../benchmarks/results/producer-handoff.json),
[scalar-parser report](../benchmarks/results/producer-scalars.json),
[delivery report](../benchmarks/results/producer-optimized.json), and
[original report](../benchmarks/results/producer-experiment.json).

## Validation and reproduction

All 85 Rust tests pass in debug and release, including 25 producer tests. Eight
selected tests pass under Miri, covering caller reads, live-body replacement,
conversion ordering, early end, recursion limits, and operands/keywords crossing
retained pages. The shared fixtures match all 25 native pdfTeX messages.
Rustfmt, clippy across all targets/features, and the release Wasm library build
pass.

Build `36e8db9` with `producer` in a separate checkout, then run from this
experimental checkout:

```bash
cargo build --locked --release --features producer --bin umber --examples
python3 scripts/oracle.py fixtures/mvp/producer-handoff.tex fixtures/mvp/native-scanners.tex
target/release/examples/producer_stats fixtures/mvp/native-scanners.tex
python3 benchmarks/mvp.py --cases raw bodies integers groups source-integers --sizes 2000000 --repeats 9 --baseline /path/to/cutover/umber
python3 benchmarks/producer.py --cases handoff tail-loop nested-bodies --sizes 1000000 --repeats 9 --baseline /path/to/cutover/umber
python3 benchmarks/producer.py --cases conversions --sizes 500000 --repeats 9 --baseline /path/to/cutover/umber
python3 benchmarks/producer.py --cases definitions messages --sizes 100000 --repeats 9 --baseline /path/to/cutover/umber
```

To compare with the producer before consumer removal, build `6a6ca50` with
`producer` and pass its executable as `--baseline`. The original `29266eb`
baseline uses no `producer` feature. The retained five-engine run times all
three Rust baselines together using the same measurement helper.
