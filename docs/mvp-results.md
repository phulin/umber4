# Native expansion measurements

The [original MVP](mvp.md) and [broader expansion slice](expansion-mvp.md) are
implemented, with 63 primitive names. At the largest sizes, ordinary release
Umber uses 0.09–0.51 times native pdfTeX's CPU time on the original eight
workloads and 0.30–0.51 times on eight broader macro workloads. Token
chunks allocate uninitialized cells; appending constructs each word directly in
its destination. The broader suite's CPU changes range from -5.1% to +2.9%
relative to the preceding build; the full comparison appears below.
These measurements cover parsing, expansion, and macro/scalar execution. Layout,
shipout, formats, checkpoints, and Wasm runtime timing come later.

An [optional native profile build](#optional-native-profile-build) uses
0.31–0.43 times pdfTeX's CPU time on the broader suite. The tables below
retain ordinary release results; the profile comparison appears later.

The tables use child user-plus-system CPU medians for the complete engine process.
Reports also retain wall time and all samples. Wall time includes scheduling
delays from other work on this shared host.
Nine paired runs of copies of the same binary differ by 1.5% on raw dispatch and
2.3% on stored-body dispatch in their CPU medians. The
[control report](../benchmarks/results/identical-binary-control.json) retains their
matching executable hashes and samples; small changes need that context.

## Broader macro workloads

The [expanded generator](../benchmarks/expansion.py) uses four workloads drawn
from Michael Doob's [gentle.tex](https://ctan.net/info/gentle/gentle.tex), a stored
variant of the record workload, and three workloads for token lists, a larger
name table, and optional arguments.
The [benchmark guide](../benchmarks/README.md) describes every consumer and
adaptation. The full manual needs the later typesetting subsystems.

Each record expands its full payload into a definition and compares it with
independently generated expected tokens. Only a successful comparison increments
the final checksum. Expected-definition construction and comparison are timed.
Contents serialization compares detokenized text; native fixtures separately
check conversion categories. Argument text varies from 8 to 1,024 characters,
with nested groups, field macros, different branches, and source/stored routes.

CPU medians at 50,000 records; ratios below 1 favor Umber:

| Workload                           | Umber (s) | pdfTeX (s) | CPU time ratio | Change vs previous Umber |
| ---------------------------------- | --------: | ---------: | -------------: | -----------------------: |
| Gentle spelling and text templates |     0.156 |      0.314 |          0.497 |                    -5.1% |
| Gentle calendar formatter          |     0.142 |      0.278 |          0.511 |                    -1.4% |
| Gentle contents serialization      |     0.445 |      1.061 |          0.419 |                    +1.2% |
| Gentle paragraph-delimited records |     0.480 |      1.321 |          0.363 |                    -0.9% |
| Gentle records from stored tokens  |     0.518 |      1.664 |          0.311 |                    +0.9% |
| Token-register pipeline            |     0.791 |      2.481 |          0.319 |                    -1.9% |
| Dynamic name table                 |     0.160 |      0.328 |          0.487 |                    +2.9% |
| Optional-argument parser           |     0.477 |      1.585 |          0.301 |                    +0.3% |

The broader workloads differ by -5.1% to +2.9% from the preceding build.
Stored Gentle records differ by +0.9% CPU time and the token
pipeline by -1.9%. Whole-binary comparisons include changes
in code placement alongside the removed chunk initialization; small differences
need the control-run variation context above.

The stored variant alternates between immutable macro bodies and forwarded
captured arguments, while retaining the same consumer and expected text as the
source-input variant.

The growth ratios between 10,000 and 50,000 records range from 0.30 to 0.52,
so the advantage over pdfTeX holds after removing small-case startup advantage.
The comparison binary is the preceding 63-primitive interpreter at `cbc910f`,
timed in the same run. Full samples, source hashes, corpus provenance, compiler
identity, and executable hashes are retained in
[expansion-native.json](../benchmarks/results/expansion-native.json).
The earlier report at `087205e` is preserved in
[expansion-before-token-windows.json](../benchmarks/results/expansion-before-token-windows.json).

## Original core regression set

CPU medians at one million work iterations, with the same comparison binary:

| Workload                       | Umber (s) | pdfTeX (s) | CPU time ratio | Change vs previous Umber |
| ------------------------------ | --------: | ---------: | -------------: | -----------------------: |
| Raw byte dispatch              |     0.242 |      0.580 |          0.417 |                    -2.9% |
| Stored macro bodies            |     0.103 |      0.273 |          0.379 |                    -0.1% |
| Integer assignments            |     0.119 |      0.379 |          0.315 |                    -5.6% |
| Repeated/nested parameters     |     0.338 |      0.662 |          0.510 |                    -1.6% |
| Arguments tokenized from bytes |     0.412 |      0.804 |          0.513 |                     0.0% |
| Repeated 256-token arguments   |     0.778 |      8.701 |          0.089 |                    +0.7% |
| Overlapping delimiters         |     0.122 |      0.263 |          0.465 |                    +0.7% |
| Local/global grouped writes    |     0.219 |      0.528 |          0.415 |                   -13.5% |

The largest core cases differ by -13.5% to +0.7%. Repeated/nested parameters
use 1.6% less CPU time; source arguments are unchanged to the displayed precision,
and long arguments use 0.7% more. These core workloads allocate little arena
storage after their setup. Core timings also reflect code and heap placement
and need the control-run variation context above.
Earlier optimization and follow-up reports remain in `benchmarks/results/` and
Git history, including the
[fifteen-sample parameter follow-up](../benchmarks/results/short-parameter-followup.json).

The [core generator](../benchmarks/mvp.py) uses compact macro fanout except for
raw dispatch and arguments tokenized from source bytes. Its 256-token argument
case discards the payload after scanning; the broader suite consumes its text.
The CLI reads the source into memory before execution. The historical
`file-parameters` key measures tokenization over resident bytes, with full-process
input reading still included in timing.

The growth ratios from 100,000 to one million iterations range from 0.09 to 0.51.
Startup probes in the two runs use 1.04–1.13 ms CPU time in Umber and
4.12–4.60 ms
in pdfTeX. Current samples are retained in
[mvp-native.json](../benchmarks/results/mvp-native.json).
Earlier reports in `benchmarks/results/` retain the original optimization passes,
including [mvp-before-expansion.json](../benchmarks/results/mvp-before-expansion.json).

## Allocation and representation

Every macro invocation installs its replacement body directly from the definition
arena. Parameterized calls attach their existing argument packet to that cursor;
retiring the body releases the packet. This shares the same arena cursor path
with calls that need no packet and preserves empty-body handling, source limits,
and argument backing lifetimes.

Operand keyword scanning compares complete literal words directly within the
current retained token window. It advances the cursor once after a full match.
Expansion, partial matches, and window crossings use the general scanner, which
retains the expanded token identities for rollback. Character identity is tested
before discarding a leading spacer, preserving TeX's behavior when case changes
alter a space token's character while retaining its category.

Literal decimal operands consume their first digit directly from the current
retained token window, then share the existing digit-window scanner for the
suffix. Window changes and expandable continuations use the ordinary reader;
spaces are consumed and other terminators retain their token identities. Signs,
radix prefixes, internal quantities, and leading spaces enter the general scan.
The suffix loop has its own Rust call boundary, keeping the recursive general
scanner's stack frame small. The existing numeric recursion limits still pass.

Physical-line loading uses [memchr2](https://docs.rs/memchr/2.8.3/memchr/fn.memchr2.html)
to find the first CR or LF in the remaining source bytes. The lexer keeps its
reusable normalized line buffer, trailing-space trimming, CRLF handling, and
live endline/category behavior. The byte search adds no token materialization or
buffer copy. Ordinary builds keep line loading behind a Rust call boundary;
profile builds let the trained compiler choose inlining.

Callgrind counts 52,407,174 instructions for 1,000 Gentle records, compared
with 54,956,690 in the preceding build: 4.6% fewer. The token pipeline uses
73,316,752 instead of 79,890,738: 8.2% fewer. The nested-parameter
probe uses 31,903,675 versus 31,920,157 instructions.
The reports retain these diagnostics with input hashes and binary identities.
Simulated instruction counts identify removed work; the CPU tables measure
execution performance. All twenty-six sampled allocation counts, requested
bytes, and heap peaks remain unchanged.

Token registers share immutable arena spans, including copies and local saves.
Their active sources keep reading the original span after replacement. Expanded
definitions and messages emit number and text conversions directly into their
destination. The generated character tokens need no further expansion or brace
and parameter handling. Text conversion shares its byte emitter with general
expansion; its body stays outside the common list loop.

When general expansion needs a token source, conversion output writes into a
reusable, uniquely owned buffer and publishes one range at completion. Numbers
that fit in the existing two-token inline source use it directly, including the
sign for negative single digits. Digits for larger values are generated from low
to high and reversed in place before publication. Release code uses reciprocal
multiplication and shifts for division by ten.

Stored list scans consume ordinary character runs using Rust pointer locals and
append directly to their destinations. Braces, parameter words, control names,
and source changes pass through the surrounding scanner. The destination keeps
input backing alive while it grows. Expanded scans splice `\the` token lists and
`\unexpanded` directly into their destination; messages write bytes directly.
Source-line scans likewise emit letter and other-character tokens directly, using
the live category table. The inspected release loop advances the file cursor and
lexical state before appending, with no per-character reader call, meaning lookup,
or name-table growth check. Special categories and line transitions return to the
complete reader. Destination emitters preserve input and category state.
Fixed-size arena chunks allow a small inline append path with allocation in a
separate growth helper. The arena caches the next write address. Appending writes
one unpublished token, advances that address, and updates the published length;
chunk growth refreshes the address. Chunks contain `UnsafeCell<MaybeUninit<Token>>`
cells with token size and alignment. The cell array is valid with uninitialized
contents; reads and cursor windows cover only initialized, published words.
The [storage design](storage.md#2-three-allocation-primitives) documents the layout
and lifetime of these cells.
Published tokens remain immutable, and boxed chunk addresses stay valid as the
descriptor vector grows. The existing length limit and chunk-boundary checks
preserve capacity behavior. Release instructions allocate the chunk with `malloc`
in place of `calloc`, with no chunk-sized stack temporary or eager zero filling.
The cursor adds one pointer to arena metadata and no heap allocation.

Grouped arguments from source bytes likewise scan letter and other-character
runs. The first word uses the existing inline capture path; subsequent words
append directly under one borrow of the uniquely owned buffer. Buffer lookup and
sequence bookkeeping sit outside the inspected release character loop. One range
records the run, coalescing with adjacent capture storage. Publication retains
the buffer once per invocation. Discarded arguments scan runs without storage.

Stored grouped arguments likewise keep progress in Rust pointer locals. An
ordinary character word advances after one unsigned range comparison; braces,
control sequences, paragraph checks, and parameter substitution use the structural
path. An initial character followed by a closing brace in the same window writes
directly into the retained argument's inline slot. An inline parent word can
supply that character as well. Discarded arguments use the window loop directly,
keeping parent-packet lookup out of their initial scan. The inspected native loop
keeps both pointers in registers and performs no per-word capture bookkeeping.

Delimited arguments share this append loop. With no partial match at top level,
it stops before a token identical to the first delimiter word. Inside groups,
ordinary characters can pass through together. Prepared-line position deltas
update the counters used to recognize and trim one outer group. The surrounding
matcher handles candidates, prefix fallback, braces, and source transitions.
Retained stored input scans ordinary token runs through immutable windows,
publishing one shared range before its source can retire. Its stop word includes
the category and identity, so equal bytes with different categories remain
distinct. Braces, control sequences, parameter substitution, partial prefixes,
and window transitions use the surrounding matcher. An inline captured word
passes through scalar promotion before looking ahead, preserving failure order.
The release run advances a pointer without per-token capture bookkeeping.
Newly interned names grow the dense meaning table through a separate cold helper;
ordinary reader loops keep that allocation and initialization out of line.

The main driver fuses token reading, expansion dispatch, and execution in one
Rust loop. Operand scanners retain `next_expanded`. The inspected release loop
reads sources directly, avoiding the expanded-reader call and return on each
main-loop token while keeping the same macro and primitive helpers.

Arena windows derive their pointers from the whole chunk with
[`UnsafeCell::raw_get`](https://doc.rust-lang.org/std/cell/struct.UnsafeCell.html#method.raw_get).
Macro equality compares complete signatures through these immutable windows.
Each step compares the shorter available window as transparent 32-bit token
words, then advances both ranges. The inspected native release code calls
`bcmp` for these contiguous ranges. Meaning, command, flag, and length shortcuts
remain ahead of the comparison; no tokens are copied or allocated.

Scalar tables share integer values and group levels in one indexed array.
Assignments resolve one slot for reading, saving, and writing. Prefix scans live
in an ordinary Rust helper; deferred assignment classification runs only when a
hook is pending. Discarded delimited arguments retain matching state while
skipping bookkeeping used solely to trim retained text. Their matcher compares
one expected token per match or fallback. Release instructions were inspected
with `objdump`; this host denies hardware performance counters.

The separate allocation diagnostic counts engine construction and execution,
excluding host input bytes, Rust stack, allocator overhead, and pdfTeX memory.
This pass leaves the sampled allocation counts and requested heap peaks unchanged
from the comparison build, including both stored-record sizes.
At one million iterations, the five sampled original capture/body workloads use
88–99 allocations and 55,764–57,417 peak requested heap bytes. Tenfold more work
adds at most two allocations. Invocation storage follows maximum live nesting
and argument size.

The broader suite deliberately replaces definitions and token-register values.
Their arena storage and definition headers accumulate until the run ends:

| Expanded workload                  | Allocations at 50,000 | Peak requested engine heap (MiB) |
| ---------------------------------- | --------------------: | -------------------------------: |
| Gentle spelling and text templates |                 1,174 |                             38.1 |
| Gentle calendar formatter          |                   521 |                             17.1 |
| Gentle contents serialization      |                 4,796 |                             94.6 |
| Gentle paragraph-delimited records |                 8,697 |                            144.6 |
| Gentle records from stored tokens  |                10,758 |                            176.7 |
| Token-register pipeline            |                22,166 |                            366.2 |
| Dynamic name table                 |                 5,476 |                             29.6 |
| Optional-argument parser           |                10,345 |                            170.1 |

Common dynamic names use 64 bytes of stack scratch space, spilling once for
longer names. Fixed symbol spellings use a static byte table, eliminating their
separate heap allocations. Spelling handles and hashes retain direct ID indexing;
the shared lookup compares the completed spelling after its stored hash matches.
Name assembly and static spellings reduce allocation calls in the largest
name-table case from 187,563 to 5,476, a 97.1% reduction. Interned spellings retain
their session lifetime.

Source control words compute their hash while consuming classified letters,
then enter that shared lookup. Superscript translation rewrites the consumed
byte before hashing; translated nonletter lookahead stays outside the spelling.
Dynamic name construction uses the same byte operation. One-byte names retain
their fixed IDs. The inspected release letter loop contains the hash XOR and
multiply followed by one lookup call, eliminating the second hashing walk.

These are requested heap peaks, not process RSS. The token pipeline grows from
74.4 MiB at 10,000 records to 366.2 MiB at 50,000. Replaced field, expected,
pipeline, and result definitions remain in the arena. Fast timings do not
establish bounded memory for a long editing session. Checkpoint-aware store
reclamation remains with the [storage design](storage.md). Both reports retain
the diagnostic binary hash and counts at both sizes.

## Optional native profile build

The [profile builder](../benchmarks/pgo.py) trains the compiler with all sixteen
workloads at 75,031 core iterations and 7,003 expanded records. Each training
result is checked. Native oracle fixtures run against both the instrumented
interpreter and the final build. The build record identifies the exact measured
interpreter and allocation diagnostic; they contain no profiling counters. The ordinary release and Wasm builds retain their existing settings.

This experiment uses Rust 1.98.1 and LLVM 22.1.8. LLVM uses recorded branch
frequencies with its inlining cost-benefit model disabled and the hot-callsite
threshold set to 1,000. Native functions are aligned to 64 bytes. With the cached
arena write cursor, that alignment removed a 35% long-argument slowdown in a focused
comparison without changing the Rust path or allocation behavior. These are
compiler choices for this measured native build. The builder enables `native-profile`,
which lets its trained compiler choose line-loader inlining; ordinary and Wasm
builds retain the measured call boundary. These compiler hints preserve the
execution paths and ownership rules.
The default profile-guided model enlarged the main loop and slowed long argument
scanning; the selected settings improve that case too. Updating the compiler or
expanding the interpreter warrants another comparison.

The full comparison uses the same seven-run, three-size protocol as ordinary
release measurements, with the current ordinary build as the paired comparison
binary. Training sizes are outside the timing sweep. CPU medians at 50,000 records:

| Workload                           | Profile Umber (s) | Ordinary Umber (s) | pdfTeX (s) | Profile/pdfTeX | Change vs ordinary |
| ---------------------------------- | ----------------: | -----------------: | ---------: | -------------: | -----------------: |
| Gentle spelling and text templates |             0.135 |              0.156 |      0.312 |          0.434 |             -13.3% |
| Gentle calendar formatter          |             0.115 |              0.139 |      0.276 |          0.415 |             -17.3% |
| Gentle contents serialization      |             0.405 |              0.421 |      1.018 |          0.397 |              -3.9% |
| Gentle paragraph-delimited records |             0.464 |              0.464 |      1.272 |          0.365 |              -0.1% |
| Gentle records from stored tokens  |             0.513 |              0.488 |      1.522 |          0.337 |              +5.1% |
| Token-register pipeline            |             0.820 |              0.763 |      2.381 |          0.344 |              +7.5% |
| Dynamic name table                 |             0.134 |              0.144 |      0.328 |          0.409 |              -6.9% |
| Optional-argument parser           |             0.485 |              0.469 |      1.556 |          0.312 |              +3.6% |

At one million core iterations, profile ratios span 0.08–0.41 times
pdfTeX's CPU time. The paired comparison with ordinary release is:

| Workload                       | Profile Umber (s) | Ordinary Umber (s) | pdfTeX (s) | Profile/pdfTeX | Change vs ordinary |
| ------------------------------ | ----------------: | -----------------: | ---------: | -------------: | -----------------: |
| Raw byte dispatch              |             0.188 |              0.243 |      0.581 |          0.323 |             -22.7% |
| Stored macro bodies            |             0.105 |              0.103 |      0.263 |          0.398 |              +1.7% |
| Integer assignments            |             0.114 |              0.126 |      0.380 |          0.300 |              -9.8% |
| Repeated/nested parameters     |             0.245 |              0.329 |      0.630 |          0.388 |             -25.7% |
| Arguments tokenized from bytes |             0.301 |              0.407 |      0.804 |          0.375 |             -25.9% |
| Repeated 256-token arguments   |             0.710 |              0.785 |      8.627 |          0.082 |              -9.5% |
| Overlapping delimiters         |             0.110 |              0.122 |      0.266 |          0.414 |              -9.2% |
| Local/global grouped writes    |             0.210 |              0.216 |      0.530 |          0.396 |              -2.6% |

The profile build remains optional: its workload tradeoffs appear in these tables.
All twenty-six sampled allocation counts, requested bytes, and heap peaks match
the ordinary build and preceding interpreter.

[Core samples](../benchmarks/results/mvp-native-pgo.json),
[broader samples](../benchmarks/results/expansion-native-pgo.json), and the
[build record](../benchmarks/results/native-pgo-build.json) retain input and
executable hashes, both timing clocks, training provenance, compiler options,
profile identities, and the exact build record.
The smaller improvements still need the control-run variation context above.

```bash
rustup component add llvm-tools
python3 benchmarks/pgo.py
python3 benchmarks/mvp.py --umber target/pgo/use/release/umber
python3 benchmarks/expansion.py --umber target/pgo/use/release/umber
```

## Compatibility and reproduction

Thirty-five Rust tests cover the core plus token-register retention, protected
and opaque insertion, conditional selection, deferred token order, case changes,
conversion categories, expression rounding/scaling/error order, generated-buffer
reuse, numeric and expansion-depth boundaries, fixed symbol names, and long/nested
dynamic names. The native oracle compares 317 marked messages and 26 rejected
programs. Expected extended-fixture messages were captured from native pdfTeX.

Short-number fixtures compare singleton windows, spaces and spacer aliases,
case-changed spacer characters, empty and parameterized macro continuations,
and an unexpandable control-sequence terminator with native pdfTeX. Operand
cursor tests cross shifted arena-window ends, retain nonspace terminators, and
check digit categories, signs, multiple digits, and failed-scan progress. This
integer boundary and progress test also passes Miri.

Additional native fixtures cover source-space normalization across tabs,
comments, line changes, delimiters, and live category changes. Expression
fixtures retain e-TeX's second expansion of a `\noexpand` operand. Macro-operand
tests cross arena-window ends with single digits, split numbers, empty arguments,
signs, and macro or internal-integer values.

Arithmetic-keyword fixtures compare mixed case, character categories, expanded
letters, leading expanded spaces, and parameter substitution with native pdfTeX.
Case-changed spacer tokens enter keywords through macros, captured arguments,
and stored definitions. The scanner test crosses arena-window boundaries and
checks that failed matches restore token identities and order, including character
aliases. This boundary and rollback test also passes Miri.

Symbol fixtures check NUL, bytes 127/128/255, empty names, and distinct ordinary
and active meanings; they retain early names and compare conversion output after
several hash-table growths. Number fixtures cross the inline-output boundaries,
retain results in expanded definitions, and feed them into scanners and captures.
They compare generated token categories after changing digit and sign catcodes,
include both integer extremes, and use numeric output as a parameter operand.
Malformed conversion operands retain priority over destination-capacity errors.
Control-word fixtures compare source and dynamic names, translated letters at
the beginning/middle/end, recursive translation, case distinctions, long
spellings, and NUL/high bytes classified as letters. A translated terminator
remains committed when its macro changes the superscript category. Character
input and scanner-edge tests have also passed Miri.
Macro-equality fixtures distinguish category codes, control-sequence identities,
parameter marker spellings, and macro flags. Generated equality tests cross
shifted chunk boundaries and change signature prefixes, lengths, and body words;
pdfTeX independently agrees on their 37 comparisons.
File-argument fixtures check whitespace and comments across physical lines,
translated braces, active characters, multiple arguments sharing a builder, and
captured categories surviving executed category changes.
Delimiter fixtures cover overlapping prefixes, whole-group trimming, fallback
after a partial paragraph delimiter, translated delimiters, physical lines and
comments, and the final `#{` parameter syntax.
Stored delimiter fixtures add capture-buffer forwarding, parameter boundaries,
literal parameter characters, and identical bytes with different categories.
The oracle generates 36 comparisons across source and delimiter chunk boundaries,
checking complete retained payloads with independently constructed definitions.
Failure tests check paragraph and closing-brace errors in stored runs, including
the reader position after the offending token.
The stored chunk/capture test and failure-position test also pass Miri.
One-word group cases move the opener, word, and closing brace through a chunk
boundary, substitute an inline parameter, and retire captured backing at the end
of its source. Native pdfTeX agrees on their nine generated comparisons. These
cases, inline-word promotion, and discarded-argument scanning also pass Miri.

Storage tests cross 4,096-token chunks, replace active sources, exercise 5,000
nested captures, and produce multi-chunk conversion text. List-scan tests
retain long source arguments while conversion/case-change output grows, repeat
those arguments, and substitute parameter words at chunk boundaries. Earlier independent
adversarial reviews checked semantics, source ownership, release paths, and
workload consumers. Debug/release tests, rustfmt, and clippy pass; the portable
library builds for `wasm32-unknown-unknown`.
The arena-window, register-retention, and stored-list tests pass Miri with
uninitialized chunks and the cached write cursor, covering neighboring-cell access, disjoint appends, descriptor growth,
active sources surviving register replacement, overlapping chunk comparisons,
and empty ranges.
The integer-overflow test also passes Miri: decimal, octal, and hexadecimal
scans consume the offending digit and retain the next token across arena windows.

Measured on 2026-10-07, Linux x86-64, AMD EPYC 7642, pinned to CPU 0:

- pdfTeX 3.141592653-2.6-1.40.25, TeX Live 2023/Debian, with
  `-ini -etex -no-shell-escape -interaction=nonstopmode -halt-on-error`.
- Ordinary release: Rust 1.98.1, thin LTO, one codegen unit, debug info,
  and no additional `RUSTFLAGS`.
- One warmup and seven measured runs per engine at each size: 10,000, 100,000,
  and 1,000,000 core iterations; 1,000, 10,000, and 50,000 expanded records.
- Rotating and reversing the order of Umber, pdfTeX, and the comparison binary.
  Both clocks cover startup, input reading, initialization, execution, and result
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

The runners accept `--pdftex /path/to/pdftex`, `--umber /path/to/umber`, and optional
`--baseline` binaries. They need native pdfTeX, with no format or font installation.
The oracle here is the checkout-local Ubuntu Noble TeX Live 2023 installation
under `.tools/pdftex`; a system installation works too. The MVP clock parameters
use fixed epoch defaults unless supplied through `Engine::set_calendar`; corpus
fixtures assign deterministic values explicitly.
