# Producer-driven expansion experiment

This experiment replaces the MVP's token-source vector with recursive token
production for a limited primitive set. It uses 26.8% less CPU across eight
supported kernels than the ordinary `29266eb` baseline and 5.0% less than the
preceding producer at `4116226`. Every measured kernel beats the original
baseline. Handoffs now beat `989b62f` by 2.9%; source integer operands regress
4.3% against `4116226`. The experiment remains isolated on
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
Nested internal integer completion reads parents directly from the suspended
records and unwinds in a loop. Only a parent that resumes receives an active
record again. Completed scalar operands assign inline during delivery; the
general integer handler covers the remaining scanning and completion cases.

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
and direct count reads with literal indices. Literal digits enter accumulation
before internal-value dispatch. Recognition reads immutable tokens without
advancing production or changing engine state. Known nonexpanding integer
terminators permit completion; incomplete windows, expansions, deeper internal
reads, and invalid operands continue through ordinary delivery.

The parser returns the number of consumed words, with zero leaving production
unchanged. Arithmetic and range checks precede the write, and rejection lets the
ordinary scanner produce its usual error and progress. The shared arithmetic
calculation runs once on success. Its constant-result setter callback is
infallible. Meanings are resolved at execution, so aliases and redefinitions
retain their current behavior. This keeps complete operations in native control
flow while retaining consumer records for operations that cross input boundaries.
After the write, the parser consumes an immediately following resolved `\relax`,
which is inert in the resumed executor. This keeps a common integer terminator
in the operation that has already classified it. Index terminators retain their
ordinary scanning behavior.

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
rounds on one AMD EPYC 7642 CPU, rotating/reversing five engines: the original
baseline, producers at `989b62f` and `4116226`, the current producer, and native
pdfTeX 1.40.25. Every engine receives the same source and validates its checksum.
Timings include process startup and file input; the table reports median CPU
time. Core kernels use 2,000,000 iterations and the three added kernels use
1,000,000.

| Kernel                        | CPU vs. `29266eb` | CPU vs. `4116226` |
| ----------------------------- | ----------------: | ----------------: |
| Literal execution             |            -12.6% |             -1.6% |
| Macro bodies                  |            -37.3% |             -6.0% |
| Stored integer operands       |            -28.2% |             -5.9% |
| Groups                        |            -35.1% |            -10.8% |
| Source integer operands       |            -18.8% |             +4.3% |
| Scanner completes inside body |            -18.4% |             -5.7% |
| Conditional tail loop         |            -14.3% |             -6.4% |
| Eleven nested non-tail bodies |            -43.1% |             -7.1% |

The geometric mean of median CPU ratios against the original baseline is 0.730
for the five core kernels, 0.735 for the three added kernels, and 0.732 across all
eight. Against `4116226`, those ratios are 0.958, 0.936, and 0.950. Across all
eight, CPU use is 17.2% below `989b62f`; handoffs are 2.9% below that version,
recovering the preceding scalar parser's regression. Source integer operands
cost 4.3% more than `4116226` while remaining 18.8% below the original baseline.

All selected kernels use less CPU than native pdfTeX. These measurements cover
expansion/register kernels within the supported subset; whole-document
performance and the full MVP remain unmeasured on this producer path.

The consumer record remains 24 bytes on the measured native build. Generated
code uses 208 bytes per recursive producer activation, including inlined token
delivery, saved registers, and return address; `4116226` used 192. The bounded
scalar parser adds a transient 128-byte frame, up from 112, and returns its word
count in a register. The outlined number handler uses 48 bytes, down from 96;
other callees add transient stack use. The million-call tail loop uses one
producer activation; the explicitly nested kernel reaches eleven.

All eight kernels match `4116226` in heap allocation requests, requested bytes,
and peak live bytes. Each makes one fixed allocation beyond the ordinary engine
for the reusable consumer vector, with no allocation per macro call. Both
entries construct the shared `Engine`, whose preallocated source vector goes
unused in the producer. Heap counters exclude the host input and native stack.

The [handoff report](../benchmarks/results/producer-handoff.json) retains the
five-way confirmation, source/binary hashes, allocation and depth diagnostics,
and eleven exploratory candidates. The shared measurement helper and exact
comparison driver accompany the samples. Earlier measurements remain in the
[scalar-parser report](../benchmarks/results/producer-scalars.json),
[delivery report](../benchmarks/results/producer-optimized.json), and
[original report](../benchmarks/results/producer-experiment.json).

## Validation and reproduction

All 75 Rust tests pass in debug and release, including fifteen producer tests.
Four selected tests pass under Miri, covering suspended parent completion,
aliased inert terminators, translated input characters, and operations split
across retained pages. The shared handoff fixture matches all sixteen native
pdfTeX messages. Rustfmt, clippy across all targets/features, and the release
Wasm library build pass. Wasm runtime performance is unmeasured.

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
comparison against the preceding producer, build `4116226` with `producer` and
pass its binary to `--baseline`. The retained five-way run also includes
`989b62f`, using the same measurement helper to time all three Rust baselines
in each round.
