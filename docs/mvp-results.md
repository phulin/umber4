# Native expansion measurements

The [original MVP](mvp.md) and [broader expansion slice](expansion-mvp.md) are
implemented, with 63 primitive names. At the largest sizes, ordinary release
Umber uses 0.09–0.52 times native pdfTeX's CPU time on the original eight
workloads and 0.30–0.51 times on eight broader macro workloads. Separating literal
integer entry from the general recursive scanner reduces raw dispatch CPU time
by 6.0% against the preceding build. The broader suite differs by -7.0% to -0.3%.
The full comparison appears below.
These measurements cover parsing, expansion, and macro/scalar execution. Layout,
shipout, formats, checkpoints, and Wasm runtime timing come later.

An [optional native profile build](#optional-native-profile-build) uses
0.31–0.44 times pdfTeX's CPU time on the broader suite. The tables below
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
| Gentle spelling and text templates |     0.154 |      0.304 |          0.506 |                    -3.4% |
| Gentle calendar formatter          |     0.129 |      0.276 |          0.468 |                    -3.5% |
| Gentle contents serialization      |     0.413 |      0.993 |          0.416 |                    -0.3% |
| Gentle paragraph-delimited records |     0.451 |      1.228 |          0.368 |                    -2.6% |
| Gentle records from stored tokens  |     0.478 |      1.523 |          0.314 |                    -2.2% |
| Token-register pipeline            |     0.749 |      2.334 |          0.321 |                    -4.1% |
| Dynamic name table                 |     0.138 |      0.319 |          0.431 |                    -7.0% |
| Optional-argument parser           |     0.463 |      1.527 |          0.303 |                    -2.5% |

The broader workloads use 0.3–7.0% less CPU time than the preceding build.
Stored Gentle records use 2.2% less and the token pipeline 4.1% less. Whole-binary
comparisons include changes in code placement alongside literal entry avoiding
the general scanner's stack frame; small differences need the control-run
variation context above.

The stored variant alternates between immutable macro bodies and forwarded
captured arguments, while retaining the same consumer and expected text as the
source-input variant.

The growth ratios between 10,000 and 50,000 records range from 0.30 to 0.52,
so the advantage over pdfTeX holds after removing small-case startup advantage.
The comparison binary is the preceding 63-primitive interpreter at `8023c8a`,
timed in the same run. Full samples, source hashes, corpus provenance, compiler
identity, and executable hashes are retained in
[expansion-native.json](../benchmarks/results/expansion-native.json).
The earlier report at `087205e` is preserved in
[expansion-before-token-windows.json](../benchmarks/results/expansion-before-token-windows.json).

## Original core regression set

CPU medians at one million work iterations, with the same comparison binary:

| Workload                       | Umber (s) | pdfTeX (s) | CPU time ratio | Change vs previous Umber |
| ------------------------------ | --------: | ---------: | -------------: | -----------------------: |
| Raw byte dispatch              |     0.208 |      0.564 |          0.369 |                    -6.0% |
| Stored macro bodies            |     0.105 |      0.273 |          0.385 |                    -4.1% |
| Integer assignments            |     0.117 |      0.366 |          0.319 |                    -0.1% |
| Repeated/nested parameters     |     0.324 |      0.627 |          0.516 |                    -3.4% |
| Arguments tokenized from bytes |     0.409 |      0.790 |          0.518 |                    +0.0% |
| Repeated 256-token arguments   |     0.734 |      8.396 |          0.087 |                    -7.9% |
| Overlapping delimiters         |     0.120 |      0.264 |          0.455 |                    -1.8% |
| Local/global grouped writes    |     0.222 |      0.517 |          0.430 |                    +4.4% |

The largest core cases differ by -7.9% to +4.4%. Repeated/nested parameters use
3.4% less CPU time; source arguments are effectively unchanged. Grouped writes
use 4.4% more. These core workloads allocate little arena storage after setup.
Core timings also reflect code and heap placement and need the control-run
variation context above.

An additional `source-integers` workload scans varied one- to ten-digit fields
directly from loaded source bytes and checks each against a stored macro operand.
The original integer case reads its repeated operands from macro tokens. At one
million fields, the retained interpreter takes 0.728 s CPU time versus pdfTeX's
1.697 s, a ratio of 0.429. The
[baseline report](../benchmarks/results/source-integers-baseline.json) retains
seven samples at each of three sizes, source identities, and the executable hash.

A [fifteen-sample grouped-write follow-up](../benchmarks/results/grouped-write-followup.json)
reproduces the 4.4% slowdown: 0.219 s CPU time against 0.209 s in the preceding
build and 0.522 s in pdfTeX. For 10,000 iterations, Callgrind counts 22,205,354
instructions against 23,867,664 in that preceding build, 7.0% fewer. The core
report retains this diagnostic with input and executable hashes; the repeated
CPU samples establish the slowdown on this host despite the reduction in work.

Earlier optimization and follow-up reports remain in `benchmarks/results/` and
Git history, including the
[fifteen-sample parameter follow-up](../benchmarks/results/short-parameter-followup.json).

The [core generator](../benchmarks/mvp.py) uses compact macro fanout except for
raw dispatch and arguments tokenized from source bytes. Its 256-token argument
case discards the payload after scanning; the broader suite consumes its text.
The CLI reads the source into memory before execution. The historical
`file-parameters` key measures tokenization over resident bytes, with full-process
input reading still included in timing.

The growth ratios from 100,000 to one million iterations range from 0.09 to 0.52.
Startup probes in the two runs use about 1.02 ms CPU time in Umber and
4.24–4.47 ms in pdfTeX. Current samples are retained in
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
The suffix loop and general scanner have separate Rust call boundaries. A small
integer entry selects them without setting up the general scanner's saved
registers and temporary reads for literal operands. In the inspected ordinary
binary, both choices use tail jumps; the trained native build inlines entry.
The existing numeric recursion limits still pass.
When no token source is installed, the same entry gate reads a first decimal
digit directly from the loaded file line if its current category is 12.
It advances that cursor and sets the middle-of-line lexical state. The existing
suffix scanner reads further digits and the expanded terminator, preserving
line changes, comments, category changes, and overflow progress.

Physical-line loading uses [memchr2](https://docs.rs/memchr/2.8.3/memchr/fn.memchr2.html)
to find the first CR or LF in the remaining source bytes. The lexer keeps its
reusable normalized line buffer, trailing-space trimming, CRLF handling, and
live endline/category behavior. The byte search adds no token materialization or
buffer copy. Ordinary builds keep line loading behind a Rust call boundary;
profile builds let the trained compiler choose inlining.

Callgrind counts 18,678,729 instructions for 10,000 raw iterations,
compared with 19,220,743 in the preceding build: 2.8% fewer.
The nested-parameter probe uses 31,340,489 versus 31,902,944 instructions.
For 1,000 Gentle records, counts are 52,106,448 versus 52,164,498;
for the token pipeline, 72,804,166 versus 72,832,713.
The reports retain these diagnostics with input hashes and binary identities.
Simulated instruction counts describe work; the CPU tables measure execution
performance. All twenty-six sampled allocation counts, requested bytes, and
heap peaks remain unchanged.

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

The recorded [profile build](../benchmarks/results/native-pgo-build.json) trained
the compiler with the original sixteen workloads at 75,031 core iterations and
7,003 expanded records. The [builder](../benchmarks/pgo.py) also trains the new
source-integer workload on subsequent builds. Each training
result is checked. Native oracle fixtures run against both the instrumented
interpreter and the final build. The build record identifies the exact measured
interpreter and allocation diagnostic; they contain no profiling counters.
The ordinary release and Wasm builds retain their existing settings.

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
| Gentle spelling and text templates |             0.132 |              0.147 |      0.300 |          0.441 |             -10.0% |
| Gentle calendar formatter          |             0.116 |              0.131 |      0.270 |          0.428 |             -11.8% |
| Gentle contents serialization      |             0.393 |              0.406 |      0.997 |          0.394 |              -3.2% |
| Gentle paragraph-delimited records |             0.457 |              0.456 |      1.234 |          0.370 |              +0.3% |
| Gentle records from stored tokens  |             0.501 |              0.470 |      1.495 |          0.335 |              +6.5% |
| Token-register pipeline            |             0.831 |              0.768 |      2.349 |          0.354 |              +8.1% |
| Dynamic name table                 |             0.124 |              0.141 |      0.325 |          0.381 |             -12.0% |
| Optional-argument parser           |             0.481 |              0.463 |      1.552 |          0.310 |              +3.8% |

At one million core iterations, profile ratios span 0.08–0.40 times
pdfTeX's CPU time. The paired comparison with ordinary release is:

| Workload                       | Profile Umber (s) | Ordinary Umber (s) | pdfTeX (s) | Profile/pdfTeX | Change vs ordinary |
| ------------------------------ | ----------------: | -----------------: | ---------: | -------------: | -----------------: |
| Raw byte dispatch              |             0.164 |              0.202 |      0.557 |          0.294 |             -19.1% |
| Stored macro bodies            |             0.097 |              0.101 |      0.259 |          0.373 |              -4.0% |
| Integer assignments            |             0.104 |              0.119 |      0.356 |          0.291 |             -12.5% |
| Repeated/nested parameters     |             0.249 |              0.326 |      0.615 |          0.404 |             -23.7% |
| Arguments tokenized from bytes |             0.282 |              0.400 |      0.773 |          0.365 |             -29.4% |
| Repeated 256-token arguments   |             0.640 |              0.740 |      8.383 |          0.076 |             -13.5% |
| Overlapping delimiters         |             0.108 |              0.124 |      0.266 |          0.405 |             -13.4% |
| Local/global grouped writes    |             0.196 |              0.221 |      0.507 |          0.386 |             -11.5% |

The profile build remains optional: its workload tradeoffs appear in these tables.
Seven-sample paired follow-ups against the preceding profile build at `8023c8a`
show 11.7% less CPU time on integer assignments and 8.6% less on source arguments.
Raw dispatch uses 1.1% more, repeated parameters 1.0% more, and grouped writes
3.3% less. Gentle text templates use 3.3% more, the calendar formatter 2.3% more,
and dynamic names 3.4% less. The
[build record](../benchmarks/results/native-pgo-build.json) retains these focused
comparisons separately from the full profile-versus-ordinary sweep.
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

Thirty-nine Rust tests cover the core plus token-register retention, protected
and opaque insertion, conditional selection, deferred token order, case changes,
conversion categories, expression rounding/scaling/error order, generated-buffer
reuse, numeric and expansion-depth boundaries, fixed symbol names, and long/nested
dynamic names. Forced hash collisions also check distinct spellings at lengths
2–33, every differing byte, first-bucket matches, and lookup after table growth.
The native oracle compares 408 marked messages and 26 rejected
programs. Expected extended-fixture messages were captured from native pdfTeX.

Short-number fixtures compare singleton windows, spaces and spacer aliases,
case-changed spacer characters, empty and parameterized macro continuations,
and an unexpandable control-sequence terminator with native pdfTeX. Operand
cursor tests cross shifted arena-window ends, retain nonspace terminators, and
check digit categories, signs, multiple digits, and failed-scan progress. This
integer boundary and progress test also passes Miri. File-operand tests start
with loaded and unread lines, retain terminating tokens, check overflow progress,
and exercise changed digit categories and inactive or digit end-line characters.

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
Prepared-line tests compare LF, CR, and CRLF input, including a control word and
`^^` translation reaching the synthetic end-line character, recursive rewrites,
an inactive end-line character, and an assignment that leaves the loaded line's
end-line character intact. The oracle checks all 27 results against independently
specified expectations as well as native pdfTeX.
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
Grouped character-run fixtures check lengths 0–12 and 255–257 at four shifted
arena positions. Their 64 native comparisons cover retained and discarded
arguments from source bytes and stored tokens, nested forwarding, and the tokens
following each closing brace. They verify that structural and following tokens
stay available when a character run ends at a batch boundary. The Rust test also
passes Miri.

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
