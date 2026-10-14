# Producer-driven expansion experiment

This experiment replaces the MVP's token-source vector with recursive token
production for a limited primitive set. It uses 22.3% less CPU across eight
supported kernels than the ordinary `29266eb` baseline and 12.1% less than the
previous optimized producer at `989b62f`. Every measured kernel beats the original
baseline. Handoffs regress 6.9% against the previous producer. The experiment
remains isolated on `experiment/producer-driven`.

## Execution model

Immutable macro bodies remain in the existing value store. A producer keeps its
body owner, read pointer, and position in Rust locals. Child bodies recurse when
the parent has unread tokens; calls at the end of a body replace the current
owner and loop. This bounds tail chains without relying on Rust tail-call
optimization. Bodies are read in place, and active owners retain their storage
even if execution replaces their definitions.

Consumers receive tokens as production proceeds. A number consumer completes its
assignment and exposes the previous consumer before the same producer reads its
remaining body. A consumer can also remain pending when a body ends and continue
with its caller's input. This directly exercises both directions of the lifetime
mismatch discussed in the original pull-reader design. For example:

```tex
\def\value{123\relax\message{\the\count0}}
\count0=\value
```

The integer consumer receives `123`, completes the assignment at `\relax`, and
returns that terminator to execution. The live producer then feeds `\message`
to execution. Conversely, `\def\set{\count0=}\set123` leaves the integer consumer
waiting after the body returns, so file input supplies the operand.

Consumer records hold unfinished operations: definition scanning, integer
scanning and its destination, messages, aliases, and conditional skipping. The
active record lives beside the engine; a reusable vector retains suspended
consumers when operations nest. Consecutive phases of one operation replace its
active record directly: register index, optional `=` or `by`, then operand.
Nested internal integer completion unwinds consumers in a loop.

This moves scanner progress into data while keeping body production on the Rust
stack. Integer consumers accumulate contiguous digits in Rust locals and publish
their progress at the window boundary. Common delivery, execution, and scanner
transitions inline together; definition construction and messages use outlined
handlers. Token delivery returns only an optional definition ID, with replay
inside delivery. It acquires a body owner on entry to a new producer frame and
retains the existing owner for a self tail call.

Definition scanning validates the supported parameter-free shape when building
the immutable value. Aliases preserve that property, so delivery needs no second
validation. A live owner reserves its value ID and storage through redefinition;
matching IDs therefore identify the same immutable body even if its control
sequence has changed meaning.

A bounded scalar parser recognizes complete assignments and arithmetic inside
one retained token window. It holds the target and operand in Rust locals and
uses the shared group-aware scalar setter. It handles signed decimal operands
and direct count reads with literal indices. Recognition reads immutable tokens
without advancing production or changing engine state. Known nonexpanding
integer terminators permit completion; incomplete windows, expansions, deeper
internal reads, and invalid operands continue through ordinary delivery.

The parser returns the number of consumed words, with zero leaving production
unchanged. Arithmetic and range checks precede the write, and rejection lets the
ordinary scanner produce its usual error and progress. The shared arithmetic
calculation runs once on success. Its constant-result setter callback is
infallible. Meanings are resolved at execution, so aliases and redefinitions
retain their current behavior. This keeps complete operations in native control
flow while retaining consumer records for operations that cross input boundaries.

The producer uses no `Engine.sources` pushes or pops. Non-tail body nesting has a
checked limit of 128; the consumer limit is 256. Diagnostic counters use a
separate entry point and are compiled out of timing builds.

## Implemented slice

The slice covers parameter-free definitions, aliases, groups, count and
category-code assignments, integer arithmetic, decimal conversion, messages, and
simple conditionals. Specifically: `\def`, `\gdef`, `\let`, `\global`, braces and
explicit groups, `\count`, `\catcode`, `\endlinechar`, count arithmetic, signed
decimal operands, internal count reads, `\number`, `\the\count`, `\message`,
`\iftrue`, `\iffalse`, `\ifnum`, `\else`, `\fi`, `\relax`, `\par`, and `\end`.

Parameter matching, expanded definitions, expressions, token registers, deferred
tokens, other number syntaxes, and other MVP primitives remain outside this
experiment. Unsupported commands return an error. This is a control-flow
prototype, not an implementation of the full MVP or its complete error behavior.
The portable library exposes `Engine::run_producer`; the `producer` Cargo feature
selects it in the CLI and allocation adapter. `Engine::run` retains the ordinary
interpreter.

Checkpoints are outside this executable experiment. The boundary remains
paragraph end with no open groups; retaining pending native producer
positions at that boundary would need separate implementation.

## Measurements

The comparison uses ordinary release builds against base commit `29266eb`, fat
LTO, one codegen unit, no PGO, and unchanged dependencies. Each kernel has nine
rounds on one AMD EPYC 7642 CPU, rotating/reversing all four engines: the original
baseline, the producer at `989b62f`, the current producer, and native pdfTeX
1.40.25. Every engine receives the same source and validates its checksum.
Timings include process startup and file input; the table reports median CPU
time. Core kernels use
2,000,000 iterations and the three added kernels use 1,000,000.

| Kernel                        | CPU vs. `29266eb` | CPU vs. `989b62f` |
| ----------------------------- | ----------------: | ----------------: |
| Literal execution             |            -13.4% |             -3.8% |
| Macro bodies                  |            -30.8% |            -19.6% |
| Stored integer operands       |            -21.6% |            -26.5% |
| Groups                        |            -28.6% |            -32.0% |
| Source integer operands       |            -22.4% |             -2.5% |
| Scanner completes inside body |            -11.9% |             +6.9% |
| Conditional tail loop         |             -6.9% |             -8.3% |
| Eleven nested non-tail bodies |            -38.0% |             -3.6% |

The geometric mean of median CPU ratios against the original baseline is 0.764
for the five core kernels, 0.798 for the three added kernels, and 0.777 across all
eight. Against `989b62f`, those ratios are 0.823, 0.981, and 0.879. Handoffs lose
some of the preceding version's gain while remaining ahead of the original
baseline.
All selected kernels use less CPU than native pdfTeX. These measurements cover
expansion/register kernels within the supported subset; whole-document
performance and the full MVP remain unmeasured on this producer path.

The consumer record is 24 bytes on the measured native build, down from 32.
Generated code uses 192 bytes per recursive producer activation, including its
inlined token delivery, saved registers, and return address; `989b62f` used 176.
The bounded scalar parser adds a transient 112-byte frame and returns its word
count in a register. The first version used a 96-byte producer frame and a
transient 144-byte dispatcher frame. Inlining
increases retained bytes per nested body while removing the common callback
boundary. Other callees add transient stack use. The million-call tail loop uses
one producer activation; the explicitly nested kernel reaches eleven.

Each kernel makes one more heap allocation request than the ordinary engine,
from the reusable consumer vector, with no allocation per macro call. The first
version made two extra requests. Both entries construct the shared `Engine`, whose
preallocated source vector goes unused in the producer. Heap counters exclude
the native stack, so they do not demonstrate lower total memory consumption.

The [scalar-parser report](../benchmarks/results/producer-scalars.json) retains
the four-way confirmation, source/binary hashes, allocation and depth diagnostics,
and seven exploratory candidates. Allocation samples match `989b62f` in all
eight kernels. The shared measurement helper and exact comparison driver accompany
the retained samples. The
[delivery report](../benchmarks/results/producer-optimized.json) retains the
preceding optimization and its fourteen exploratory candidates. The
[original report](../benchmarks/results/producer-experiment.json) retains the
first version's regressions. Compact consumer layout, in-place phase changes,
and the hot/cold split produced the gains. Inlining the entire dispatcher and
adding a larger literal-phase scanner both regressed in exploratory timing.

## Validation and reproduction

All 72 Rust tests pass in debug and release, including twelve producer tests.
Four selected ownership/scalar tests pass under Miri, including operations split
across retained pages and rejection before expansion or arithmetic errors.
The shared handoff fixture matches all sixteen native pdfTeX messages.
Rustfmt, clippy across all targets/features, and the release Wasm library build
pass. Wasm runtime performance is unmeasured.

Build a baseline binary from `29266eb` without the `producer` feature in a
separate checkout. Then, from this experimental checkout:

```bash
cargo build --locked --release --features producer --bin umber --examples
python3 scripts/oracle.py fixtures/mvp/producer-handoff.tex
target/release/umber fixtures/mvp/producer-handoff.tex
target/release/examples/producer_stats fixtures/mvp/producer-handoff.tex
python3 benchmarks/mvp.py --cases raw bodies integers groups source-integers --sizes 2000000 --repeats 9 --baseline /path/to/baseline/umber
python3 benchmarks/producer.py --sizes 1000000 --repeats 9 --baseline /path/to/baseline/umber
```

Those commands reproduce the original-baseline comparison. For the same native
comparison against the preceding producer, build `989b62f` with `producer` and
pass its binary to `--baseline`. The retained four-way run uses the same
measurement helper to time both baselines in each round.
