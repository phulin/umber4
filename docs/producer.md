# Producer-driven expansion experiment

This experiment replaces the MVP's token-source vector with recursive token
production for a limited primitive set. It works, including scanner handoffs
across macro boundaries, but usually takes more CPU time than the existing
interpreter. It remains isolated on `experiment/producer-driven`.

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
scanning and its destination, messages, aliases, and conditional skipping. A
small vector allows these operations to nest. This moves scanner progress into
data; it does not put both independent control flows on the Rust stack. Integer
consumers batch literal digits from source bytes or stored token windows.
Definition construction is kept out of the hot token dispatcher.

The producer uses no `Engine.sources` pushes or pops. Non-tail body nesting has a
checked limit of 128; the consumer limit is 256. Diagnostic counters use a
separate entry point and are compiled out of timing builds.

## Implemented slice

The initial slice covers parameter-free definitions, aliases, groups, count and
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
1,000,000 iterations and the three added kernels use 100,000.

| Kernel                        | CPU change vs. existing engine |
| ----------------------------- | -----------------------------: |
| Literal execution             |                         +22.9% |
| Macro bodies                  |                         +44.1% |
| Stored integer operands       |                         +69.8% |
| Groups                        |                         +56.7% |
| Source integer operands       |                          +2.2% |
| Scanner completes inside body |                          +9.2% |
| Conditional tail loop         |                         +47.3% |
| Eleven nested non-tail bodies |                          -1.6% |

Source integers and nested bodies are approximately tied. The other kernels
regress, despite batching substantially improving the first token-at-a-time
implementation. All selected kernels still use less CPU than native pdfTeX, but
these are expansion/register kernels within a smaller supported subset; they
establish neither full-MVP equivalence nor whole-document performance.

The consumer record is 32 bytes on the measured native build. Generated code
uses 96 bytes per recursive producer activation and a transient 144-byte token
dispatcher frame, including saved registers and return addresses. Other callees
add transient stack use. The 100,000-call tail loop uses one producer activation;
the explicitly nested kernel reaches eleven. The explicit input-source record
has disappeared from each call, but consumer dispatch and native register
saving still cost time and space.

Each kernel makes two more heap allocation requests than the ordinary engine,
from constructing and growing the reusable consumer vector, with no allocation
per macro call. Both entries still construct the original `Engine`, whose
preallocated source vector goes unused in the producer. Heap counters exclude
the native stack, so they do not demonstrate lower total memory consumption.

The [retained report](../benchmarks/results/producer-experiment.json) contains
all samples, source and binary hashes, allocation measurements, depth diagnostics,
and the initial token-at-a-time measurements. These results measure this
implementation; they do not rule out a faster producer design. They do show that
moving body cursors onto the native stack alone is insufficient.

## Validation and reproduction

All 67 Rust tests pass in debug and release, including seven producer tests.
Three ownership/handoff tests pass under Miri. The shared handoff fixture matches
all nine native pdfTeX messages. Rustfmt, clippy across all targets/features, and
the release Wasm library build pass. Wasm runtime performance is unmeasured.

Build a baseline binary from `29266eb` without the `producer` feature in a
separate checkout. Then, from this experimental checkout:

```bash
cargo build --locked --release --features producer --bin umber --examples
python3 scripts/oracle.py fixtures/mvp/producer-handoff.tex
target/release/umber fixtures/mvp/producer-handoff.tex
target/release/examples/producer_stats fixtures/mvp/producer-handoff.tex
python3 benchmarks/mvp.py --cases raw bodies integers groups source-integers --sizes 1000000 --repeats 9 --baseline /path/to/baseline/umber
python3 benchmarks/producer.py --repeats 9 --baseline /path/to/baseline/umber
```
