# Producer-driven expansion experiment

This experiment replaces the MVP's token-source vector with recursive token
production for a limited primitive set. After optimization it uses 11.2% less
CPU across eight supported kernels than the ordinary `29266eb` baseline. Stored
integer operands remain 8.9% slower. The experiment remains isolated on
`experiment/producer-driven`.

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

Checkpoints are outside this executable experiment. The proposed boundary
remains paragraph end with no open groups; retaining pending native producer
positions at that boundary would need separate implementation.

## Measurements

The comparison uses ordinary release builds against base commit `29266eb`, fat
LTO, one codegen unit, no PGO, and unchanged dependencies. Each kernel has nine
paired repetitions on one AMD EPYC 7642 CPU, with the same source and validated
checksum in both interpreters and native pdfTeX 1.40.25. Timings include process
startup and file input; the table reports median CPU time. Core kernels use
2,000,000 iterations and the three added kernels use 1,000,000.

| Kernel                        | CPU change vs. existing engine |
| ----------------------------- | -----------------------------: |
| Literal execution             |                          -9.3% |
| Macro bodies                  |                         -13.5% |
| Stored integer operands       |                          +8.9% |
| Groups                        |                          +2.1% |
| Source integer operands       |                         -16.2% |
| Scanner completes inside body |                         -17.4% |
| Conditional tail loop         |                          +0.7% |
| Eleven nested non-tail bodies |                         -36.2% |

The geometric mean of median CPU ratios is 0.939 for the five core kernels,
0.810 for the three added kernels, and 0.888 across all eight. Groups and the
tail loop are approximately tied; stored integers retain a measurable regression.
All selected kernels use less CPU than native pdfTeX. These measurements cover
expansion/register kernels within the supported subset; whole-document
performance and the full MVP remain unmeasured on this producer path.

The consumer record is 24 bytes on the measured native build, down from 32.
Generated code uses 176 bytes per recursive producer activation, including its
inlined token delivery, saved registers, and return address. The first version
used a 96-byte producer frame and a transient 144-byte dispatcher frame. Inlining
increases retained bytes per nested body while removing the common callback
boundary. Other callees add transient stack use. The million-call tail loop uses
one producer activation; the explicitly nested kernel reaches eleven.

Each kernel makes one more heap allocation request than the ordinary engine,
from the reusable consumer vector, with no allocation per macro call. The first
version made two extra requests. Both entries construct the shared `Engine`, whose
preallocated source vector goes unused in the producer. Heap counters exclude
the native stack, so they do not demonstrate lower total memory consumption.

The [optimized report](../benchmarks/results/producer-optimized.json) retains
all confirmation samples, source and binary hashes, allocation measurements,
depth diagnostics, and fourteen exploratory candidates. The
[original report](../benchmarks/results/producer-experiment.json) retains the
first version's regressions. Compact consumer layout, in-place phase changes,
and the hot/cold split produced the gains. Inlining the entire dispatcher and
adding a larger literal-phase scanner both regressed in exploratory timing.

## Validation and reproduction

All 69 Rust tests pass in debug and release, including nine producer tests.
Five ownership/handoff tests pass under Miri. The shared handoff fixture matches
all twelve native pdfTeX messages. Rustfmt, clippy across all targets/features, and
the release Wasm library build pass. Wasm runtime performance is unmeasured.

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
