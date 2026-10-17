# Native producer experiment

Macro execution and primitive scanning use Rust functions. Native body execution
keeps its cursor local between calls; scanners keep operand progress in their
stack locals. This follow-up batches source digits, executes unread scanner
suffixes in place, and caches the first body window at publication. It remains
on `experiment/producer-driven`. Across the original eight kernels it uses
3.1% less CPU than `8b7f789`, bringing aggregate CPU within 0.2% of the earlier
producer with consumers at `6a6ca50`.

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
and backed-up tokens. Each entry has its native caller's scope label. Execution
continues through the range in that storage. Inert prefixes use a local cursor
within the retained window; active commands use the shared dispatcher and
native operand functions. This removes the transfer into a resumption frame.
Macro tokens stay in their original storage.

The caller link is a pointer to an exclusively borrowed stack frame. The child
finishes before that borrow ends and accesses the parent only through this link.
Pending input contains owners and scope labels, with no parent pointers. A label
comes from the frame address and is used only for equality. Calls drain their
input before a normal return; `\end` and errors terminate reading before any
frame disappears. Owners retain initialized immutable token windows across
definition replacement and arena reuse. Pointer operations and their lifetime
conditions are documented in [producer_input.rs](../crates/umber/src/producer_input.rs)
and [values.rs](../crates/umber/src/values.rs).

Definition publication caches the first body window's start and logical end.
The existing owner retains its backing page. New calls and self-tail restarts
load that cursor directly; later windows use the existing token store lookup.
The producer feature adds 8 bytes to each native value header for this cache.
Header reuse recomputes it from the newly completed definition.

Integer input selects a native or pending body once, batches its digits, then
uses the observed token in that same range. Source digit runs keep position and
accumulation local, committing the lexer position and state at the end of the
run. Overflow consumes the offending digit. Source lookahead retains the token's
already classified category across a subsequent assignment; immutable stored
tokens already preserve that identity. Literal operators can consume a backed
first character. Their separate backed-token path leaves the common stored-word
comparisons fixed in size. Ordinary decimal prefixes share a bounded arithmetic
path; values near the limit retain checked multiplication and addition.

Decimal conversion packs an `i32` into a `u64`, including its sign, without
allocating output tokens. Native macro depth and pending-input entries are
bounded at 128; recursive scanning is bounded at 256, including nested `\number`
and `\the`. Depth travels in call arguments. The integer scanner is outlined;
its progress and recursive internal quantities still use ordinary Rust calls.

Native body execution keeps its window cursor local. It publishes that cursor
before entering a scanner or child macro, then reloads it after a child can have
consumed caller input. Ordinary child calls drain their own pending input and
create no input in their caller's scope. Body execution therefore continues
directly after those calls, skipping inert tokens in the same loop. General
input routing runs after a primitive can have introduced pending input or a
backed-up token. Root file execution keeps its existing loop; the choice is a
compile-time parameter. Scanners retain one implementation across window and
caller boundaries. `Consumer`, `Finish`, the suspended-operation vector, and
the separate bounded scalar parser remain removed.

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
producer at `8b7f789`, producer with consumers at `6a6ca50`, ordinary interpreter
at `29266eb`, and native pdfTeX 1.40.25. Every engine receives identical source
and validates its checksum. Median CPU times include process startup, file
input, execution, and output. Startup is measured separately without
subtraction. Tests, builds, Miri, and heap diagnostics finish before timing.

The unchanged five core kernels use 2,000,000 iterations; handoff, tail-loop, and
nested-body kernels use 1,000,000.

| Kernel                        | CPU vs. `8b7f789` | CPU vs. `6a6ca50` | CPU / pdfTeX |
| ----------------------------- | ----------------: | ----------------: | -----------: |
| Literal execution             |             +7.5% |             -6.3% |        0.300 |
| Macro bodies                  |             -6.8% |             -9.9% |        0.255 |
| Stored integer operands       |             -3.8% |             +3.2% |        0.249 |
| Groups                        |             -1.0% |            +20.2% |        0.327 |
| Source integer operands       |             -4.3% |             -3.9% |        0.321 |
| Scanner completes inside body |             -7.6% |             +9.3% |        0.379 |
| Conditional tail loop         |             -3.2% |             -9.1% |        0.314 |
| Eleven nested non-tail bodies |             -5.0% |             +1.0% |        0.409 |

The geometric mean of the eight CPU ratios is 0.969 against `8b7f789`, 1.001
against the producer with consumers, 0.731 against the original interpreter,
and 0.315 against native pdfTeX. Literal source execution regresses 7.5% against
`8b7f789`; macro bodies improve 6.8%, source integers 4.3%, handoffs 7.6%, and
nested bodies 5.0%. Grouped assignments still use 20.2% more CPU than the
producer with consumers, and handoffs use 9.3% more. These kernels retain the
same macro-call and tail-call counts.

Three supplemental workloads remain separate from the eight-kernel comparison.
Nested conversion uses 500,000 iterations; repeated definitions and messages
use 100,000 each. Definitions replace a body with 32 inert tokens and execute
its unread suffix. Messages expand nested braces and a changing count value.

| Kernel               | CPU vs. `8b7f789` | CPU vs. `6a6ca50` | CPU / pdfTeX |
| -------------------- | ----------------: | ----------------: | -----------: |
| Nested conversions   |             -5.0% |             +4.5% |        0.375 |
| Repeated definitions |             +4.2% |            -17.6% |        0.347 |
| Expanded messages    |             -1.3% |             +0.8% |        0.470 |

All eleven kernels beat native pdfTeX. Repeated definitions regress 4.2% against
`8b7f789`; expanded messages use 4.9% more CPU than the original interpreter.
These results cover the limited producer slice; parameter matching, full-MVP,
whole-document, and Wasm runtime performance remain unmeasured.

The producer has 1,005 production lines across four modules, 33 more than
`8b7f789`. The experimental crate has 4,991 production lines, 121 more. Counts
include comments and blanks and exclude separate tests and `cfg(test)` items.
The changes keep scanning in native functions and remove body resumption's
function and transfer. They add source batching, first-window publication,
shared operand routing, and lexical lookahead retention.

On the measured native build, pending-input records remain 48 bytes. Native
macro entry uses 160 bytes including saved registers and return address,
compared with 192 at `8b7f789`. Unread suffix execution uses its existing input
record. Outlined integer scanning uses 96 bytes; expansion uses 112 instead of
128. Parent reads and other calls add transient frames. Native text/rodata
shrinks 2.3%. The million-call tail loop uses one macro activation; the nested
kernel reaches eleven. Handoff and definition kernels' native depth falls from
one to zero.

Allocation request counts match `8b7f789` on all eleven workloads. The entry
cache adds 8 bytes per allocated native value header, reusing existing padding;
requested and peak live bytes grow by 0 to 88 bytes in these kernels. Ordinary
macro execution allocates no heap per call. Lexical lookahead raises pending
input peaks from one to two entries on nine kernels, within the same vector
capacity; conversion and message kernels already reached two. Both entries
construct the shared `Engine`, including unused ordinary source-vector
capacity. Heap counters exclude host input, native stack, and allocator
metadata.

The [operand and call report](../benchmarks/results/producer-operands.json) retains
raw CPU/wall samples, exact orchestration, compiler/source/binary identities,
allocation and depth diagnostics, native instructions, and twenty exploratory
variants. Earlier measurements remain in the
[body cursor report](../benchmarks/results/producer-body-cursor.json),
[native scanner report](../benchmarks/results/producer-native-scanners.json),
[handoff report](../benchmarks/results/producer-handoff.json),
[scalar-parser report](../benchmarks/results/producer-scalars.json),
[delivery report](../benchmarks/results/producer-optimized.json), and
[original report](../benchmarks/results/producer-experiment.json).

## Validation and reproduction

All 88 Rust tests pass in debug and release, including 28 producer tests. Eleven
selected tests pass under Miri, covering caller reads, live-body replacement,
conversion ordering, early end, recursion limits, source lexical progress and
categories, and operands/keywords crossing retained pages. Both shared fixtures
match all 30 native pdfTeX messages. Ten complete generated messages match in
all five engines. Rustfmt, clippy across all targets/features, and the release
Wasm library build pass.

Build `8b7f789` with `producer` in a separate checkout, then run from this
experimental checkout:

```bash
cargo build --locked --release --features producer --bin umber --examples
python3 scripts/oracle.py fixtures/mvp/producer-handoff.tex fixtures/mvp/native-scanners.tex
target/release/examples/producer_stats fixtures/mvp/native-scanners.tex
python3 benchmarks/mvp.py --cases raw bodies integers groups source-integers --sizes 2000000 --repeats 9 --baseline /path/to/before/umber
python3 benchmarks/producer.py --cases handoff tail-loop nested-bodies --sizes 1000000 --repeats 9 --baseline /path/to/before/umber
python3 benchmarks/producer.py --cases conversions --sizes 500000 --repeats 9 --baseline /path/to/before/umber
python3 benchmarks/producer.py --cases definitions messages --sizes 100000 --repeats 9 --baseline /path/to/before/umber
```

To compare with the producer before consumer removal, build `6a6ca50` with
`producer` and pass its executable as `--baseline`. The original `29266eb`
baseline uses no `producer` feature. The retained five-engine run times all
three Rust baselines together using the same measurement helper.
