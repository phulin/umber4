# Native expansion measurements

The [original MVP](mvp.md) and [broader expansion slice](expansion-mvp.md) are
implemented, with 63 primitive names. At the largest sizes, ordinary release
Umber uses 0.07–0.49 times native pdfTeX's CPU time on nine core
workloads and 0.30–0.44 times on nine broader macro workloads.
Definition headers shrink from 88 to 32 native bytes, saving 7–14 MiB of peak
requested engine heap across the expanded workloads at 50,000 records.
The tables retain CPU gains and regressions against the preceding build.
These measurements cover parsing, expansion, and macro/scalar execution. Layout,
shipout, formats, checkpoints, and Wasm runtime timing come later.

An [optional native profile build](#optional-native-profile-build) uses
0.30–0.38 times pdfTeX's CPU time on the broader suite. Its
immutable delimiter-table cursor reduces overlapping-delimiter CPU time by
6.3% against the preceding profile build.
The tables below retain ordinary release results; the profile comparison appears
later. Ordinary release's execution text is identical to `b8d6991`.

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
variant of the record workload, and four synthetic workloads for token lists, a
larger name table, optional arguments, and numbered references. All nine cases
retain their input identities; each appears at three sizes in the current
ordinary and profile sweeps.
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
| Gentle spelling and text templates |     0.143 |      0.325 |          0.439 |                    -5.0% |
| Gentle calendar formatter          |     0.123 |      0.293 |          0.420 |                    +0.5% |
| Gentle contents serialization      |     0.417 |      1.054 |          0.395 |                    -1.9% |
| Gentle paragraph-delimited records |     0.450 |      1.289 |          0.349 |                    -3.6% |
| Gentle records from stored tokens  |     0.480 |      1.512 |          0.317 |                    +3.1% |
| Token-register pipeline            |     0.778 |      2.422 |          0.321 |                    -1.1% |
| Dynamic name table                 |     0.132 |      0.331 |          0.400 |                    -6.6% |
| Optional-argument parser           |     0.471 |      1.597 |          0.295 |                    -1.7% |
| Numbered references                |     0.146 |      0.338 |          0.433 |                    -1.3% |

The broader workloads differ by -6.6% to +3.1% from the preceding build.
Their smaller definition headers retain parameter metadata only when a macro
needs it. Whole-binary comparisons include compiler code placement; small
differences need the control-run variation context above.

The stored variant alternates between immutable macro bodies and forwarded
captured arguments, while retaining the same consumer and expected text as the
source-input variant.

The growth ratios between 10,000 and 50,000 records range from 0.30 to 0.44,
so the advantage over pdfTeX holds after removing small-case startup advantage.
The comparison binary is the preceding interpreter at `e2ee588`,
timed in the same run. Full samples, source hashes, corpus provenance, compiler
identity, and executable hashes are retained in
[expansion-native.json](../benchmarks/results/expansion-native.json).
The earlier report at `087205e` is preserved in
[expansion-before-token-windows.json](../benchmarks/results/expansion-before-token-windows.json).

The numbered-reference workload forms 256 control names from signed three- to
ten-digit register values. `\ifcsname` and `\csname` consume generated decimal
input sources; trailing decimal text is emitted directly into `\edef`.
Every reference and number is compared with independently generated expected
tokens. Its earlier coverage-only measurements at `157e7be` remain in the
[ordinary supplemental report](../benchmarks/results/numeric-references-native.json)
and [profile supplemental report](../benchmarks/results/numeric-references-native-pgo.json).
The current tables include this case in the complete nine-workload suite.

## Core regression set

CPU medians at one million work iterations, with the same comparison binary:

| Workload                          | Umber (s) | pdfTeX (s) | CPU time ratio | Change vs previous Umber |
| --------------------------------- | --------: | ---------: | -------------: | -----------------------: |
| Raw byte dispatch                 |     0.229 |      0.584 |          0.393 |                    +4.3% |
| Stored macro bodies               |     0.103 |      0.275 |          0.375 |                    -3.4% |
| Integer assignments               |     0.120 |      0.377 |          0.318 |                    -2.4% |
| Repeated/nested parameters        |     0.321 |      0.665 |          0.483 |                    -5.2% |
| Arguments tokenized from bytes    |     0.409 |      0.840 |          0.486 |                    -3.7% |
| Repeated 256-token arguments      |     0.564 |      8.638 |          0.065 |                   -24.8% |
| Overlapping delimiters            |     0.117 |      0.271 |          0.432 |                   -11.3% |
| Local/global grouped writes       |     0.219 |      0.534 |          0.410 |                    -7.1% |
| Varied integers from source bytes |     0.733 |      1.743 |          0.420 |                    -1.2% |

The largest core cases differ by -24.8% to +4.3%.
The driver keeps token reading, expansion, and primitive dispatch together in a
borrowed engine loop; outcome construction and engine release stay in its owning
caller. Grouped stored-character scans test four immutable words together before
advancing, with a scalar tail for structural words and window ends. These
whole-binary timings include compiler code placement. The comparison build is
`e2ee588`.

The `source-integers` workload scans varied one- to ten-digit fields directly
from loaded source bytes and checks each against a stored macro operand.
The original integer case reads its repeated operands from macro tokens.
Its earlier [baseline report](../benchmarks/results/source-integers-baseline.json)
retains seven samples at each of three sizes from the preceding interpreter.
Both cases now appear in the full core and profile sweeps.

An earlier [fifteen-sample grouped-write follow-up](../benchmarks/results/grouped-write-followup.json)
recorded a 4.4% slowdown at `0cf14d8` against `8023c8a` despite 7.0% fewer
simulated instructions. That report retains the historical timing samples and
instruction diagnostic with input and executable hashes. The current table
compares the compact-definition build against `e2ee588`.

Earlier optimization and follow-up reports remain in `benchmarks/results/` and
Git history, including the
[fifteen-sample parameter follow-up](../benchmarks/results/short-parameter-followup.json).

The [core generator](../benchmarks/mvp.py) uses compact macro fanout except for
raw dispatch and arguments tokenized from source bytes. Its 256-token argument
case discards the payload after scanning; the broader suite consumes its text.
The CLI reads the source into memory before execution. The historical
`file-parameters` key measures tokenization over resident bytes, with full-process
input reading still included in timing.

The growth ratios from 100,000 to one million iterations range from 0.07 to 0.49.
Startup probes use 1.05–1.07 ms CPU time in Umber and
4.51–4.62 ms in pdfTeX. Current samples are retained in
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

Conditional skipping scans ordinary stored characters directly in their retained
window. Control sequences and parameter markers use the ordinary reader for
meaning lookup and substitution. File runs stop before special categories and
line transitions and preserve lexical state. The inspected release loops have
no per-character reader call or resolved-read construction, and add no storage.

Argument reset clears each completed sequence through a small Rust call in
ordinary builds. The inspected packet-release function shrinks from 467 to 243
bytes, avoiding the unrolled loop's count/setup work for common one-argument
packets. Definition and argument ownership, retained capacities, and release
order stay the same. The profile build lets the trained compiler choose inlining.

The earlier packed-bucket build's instruction diagnostics remain in Git
history. The current profile instruction counts and paired comparison are
recorded below and in
[definition-storage-followup.json](../benchmarks/results/definition-storage-followup.json).
Simulated instruction counts describe work; CPU medians measure performance.

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

Stored grouped arguments keep progress in Rust pointer locals. A small Rust
leaf tests four initialized immutable words together and advances only when all
are ordinary characters. The inspected native loop compares all four with SIMD
instructions and combines their result through an integer mask. Its scalar tail
stops at the first structural word or window end. Peeking reads only the retained
window and changes no interpreter state; brace depth, paragraph checks and
parameter substitution remain in the caller. The initial one-word argument
optimization and discard path keep their existing ownership behavior. This read
adds no token destination or allocation.

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
Rust loop borrowed from the owning engine. Operand scanners retain
`next_expanded`. The inspected release loop reads sources directly, avoiding the
expanded-reader call and return on each main-loop token. Its owner constructs
the outcome and drops the engine after the loop returns, keeping cleanup outside
the hot function.

Definition prefixes, delimiters, and bodies use eight-byte arena coordinate pairs.
Their general input span adds the definition store ID when installing a source.
The signature is derived from the prefix start through the body end, including
parameter markers, the body separator, and any final `#{` brace. A definition
keeps these bounds, flags, a parameter count, and an optional owned parameter
record: 32 native bytes in place of 88. Only parameterized definitions allocate
that record; the existing delimiter and failure vectors move into it. The count
and matching record are set together at construction. All definitions retain
their session lifetime with the token arena, so active sources and restored
meanings keep their stable IDs and backing.

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
Packed buckets remove the hash vector's allocation and growth; compact
definitions reduce header storage. At one million iterations, the five original
capture/body workloads use 86–99 allocations and
51,188–52,457 peak requested heap bytes.
Invocation storage follows maximum live nesting and argument size.

The broader suite deliberately replaces definitions and token-register values.
Their arena storage and definition headers accumulate until the run ends:

| Expanded workload                  | Allocations at 50,000 | Peak engine heap (MiB) | Previous heap (MiB) |
| ---------------------------------- | --------------------: | ---------------------: | ------------------: |
| Gentle spelling and text templates |                 1,175 |                   24.1 |                38.1 |
| Gentle calendar formatter          |                   522 |                   10.1 |                17.1 |
| Gentle contents serialization      |                 4,798 |                   80.6 |                94.6 |
| Gentle paragraph-delimited records |                 8,698 |                  137.6 |               144.6 |
| Gentle records from stored tokens  |                10,760 |                  169.7 |               176.7 |
| Token-register pipeline            |                22,168 |                  352.1 |               366.1 |
| Dynamic name table                 |                 5,474 |                   22.6 |                29.6 |
| Optional-argument parser           |                10,349 |                  163.1 |               170.1 |
| Numbered references                |                   798 |                   10.3 |                17.3 |

The 50,000-record cases save 7 or 14 MiB of peak requested heap. Moving
parameter metadata into its own record adds 3–6 allocation calls per document
in these workloads. The vectors move by ownership and argument ranges keep
sharing their existing backing.

Common dynamic names use 64 bytes of stack scratch space, spilling once for
longer names. Fixed symbol spellings use a static byte table, eliminating their
separate heap allocations. Spelling handles retain direct ID indexing.
Each 64-bit bucket holds a stable
name ID and a 32-bit hash fragment; matching fragments enter full spelling
comparison. With 24-bit IDs and a half-full table, growth needs at most 25 hash
bits, all retained in the fragment. Rehashing moves bucket words and preserves
IDs and spelling ownership. Name assembly, static spellings and packed buckets
reduce allocation calls in the largest name-table case from 187,563 to
5,474. Interned spellings retain their session lifetime.

Source control words compute their hash while consuming classified letters,
then enter that shared lookup. Superscript translation rewrites the consumed
byte before hashing; translated nonletter lookahead stays outside the spelling.
Dynamic name construction uses the same byte operation. One-byte names retain
their fixed IDs. The inspected release letter loop contains the hash XOR and
multiply followed by one lookup call, eliminating the second hashing walk.

These are requested heap peaks, not process RSS. The token pipeline grows from
70.9 MiB at 10,000 records to 352.1 MiB at 50,000. Replaced field, expected,
pipeline, and result definitions remain in the arena. Fast timings do not
establish bounded memory for a long editing session. Checkpoint-aware store
reclamation remains with the [storage design](storage.md). Both reports retain
the diagnostic binary hash and counts at both sizes.

## Optional native profile build

The recorded [profile build](../benchmarks/results/native-pgo-build.json) trains
the compiler with all eighteen workloads at 75,031 core iterations and 7,003
expanded records. The [builder](../benchmarks/pgo.py) checks every training result.
Native oracle fixtures run against both the instrumented interpreter and the
final build. The build record identifies the exact measured interpreter and
allocation diagnostic; they contain no profiling counters. Ordinary release
and Wasm builds retain their existing settings.

This experiment uses Rust 1.98.1 and LLVM 22.1.8. LLVM uses recorded branch
frequencies with its inlining cost-benefit model disabled and the hot-callsite
threshold set to 1,000. Native functions are aligned to 64 bytes. With the cached
arena write cursor, that alignment removed a 35% long-argument slowdown in a focused
comparison without changing the Rust path or allocation behavior. These are
compiler choices for this measured native build. The builder enables `native-profile`,
which lets its trained compiler choose line-loader inlining; ordinary and Wasm
builds retain the measured call boundary. Argument reset likewise keeps a small
call boundary in ordinary builds, with compiler-selected inlining in profile
builds. These compiler hints preserve the execution paths and ownership rules.
Profile list scans first test the contiguous token range for spaces, letters,
and other characters. The common stored-character loop avoids extracting and
classifying the category again; all other words retain the existing structural
and parameter checks. Ordinary release keeps its previous classification path.
Profile lookup stays behind a measured Rust call boundary. Its unchecked spelling
read relies on bucket publication: an occupied bucket refers to an initialized
spelling entry that remains present through table growth. The shared read ends
before insertion can grow the spelling vector. Ordinary builds retain checked
spelling reads.
Profile delimiter matching caches immutable failure-table raw parts in its Rust
frame. Checked fallback reads take a short shared borrow; reader and capture
calls mutate other stores. The table remains allocated with its definition,
including when the definition vector grows. The inspected discard matcher shrinks
from 2,923 to 2,756 native bytes and the retained matcher from 4,913 to 4,816.
Ordinary release retains direct metadata lookup; these choices come from timing
the surrounding execution paths.
The default profile-guided model enlarged the main loop and slowed long argument
scanning; the selected settings improve that case too. Updating the compiler or
expanding the interpreter warrants another comparison.

The full comparison uses the same seven-run, three-size protocol as ordinary
release measurements, with the current ordinary build as the paired comparison
binary. Training sizes are outside the timing sweep. CPU medians at 50,000 records:

| Workload                           | Profile Umber (s) | Ordinary Umber (s) | pdfTeX (s) | Profile/pdfTeX | Change vs ordinary |
| ---------------------------------- | ----------------: | -----------------: | ---------: | -------------: | -----------------: |
| Gentle spelling and text templates |             0.121 |              0.141 |      0.314 |          0.384 |             -14.3% |
| Gentle calendar formatter          |             0.097 |              0.114 |      0.276 |          0.353 |             -14.9% |
| Gentle contents serialization      |             0.389 |              0.411 |      1.026 |          0.379 |              -5.4% |
| Gentle paragraph-delimited records |             0.457 |              0.461 |      1.267 |          0.361 |              -0.8% |
| Gentle records from stored tokens  |             0.495 |              0.469 |      1.539 |          0.322 |              +5.5% |
| Token-register pipeline            |             0.804 |              0.776 |      2.407 |          0.334 |              +3.6% |
| Dynamic name table                 |             0.117 |              0.125 |      0.326 |          0.358 |              -6.7% |
| Optional-argument parser           |             0.482 |              0.462 |      1.598 |          0.301 |              +4.2% |
| Numbered references                |             0.122 |              0.148 |      0.338 |          0.361 |             -17.6% |

At one million core iterations, profile ratios span 0.06–0.40 times
pdfTeX's CPU time. The paired comparison with ordinary release is:

| Workload                          | Profile Umber (s) | Ordinary Umber (s) | pdfTeX (s) | Profile/pdfTeX | Change vs ordinary |
| --------------------------------- | ----------------: | -----------------: | ---------: | -------------: | -----------------: |
| Raw byte dispatch                 |             0.170 |              0.218 |      0.576 |          0.296 |             -21.9% |
| Stored macro bodies               |             0.098 |              0.106 |      0.266 |          0.370 |              -7.5% |
| Integer assignments               |             0.107 |              0.116 |      0.362 |          0.295 |              -8.0% |
| Repeated/nested parameters        |             0.251 |              0.307 |      0.633 |          0.397 |             -18.1% |
| Arguments tokenized from bytes    |             0.302 |              0.389 |      0.801 |          0.377 |             -22.3% |
| Repeated 256-token arguments      |             0.547 |              0.557 |      8.543 |          0.064 |              -1.7% |
| Overlapping delimiters            |             0.108 |              0.121 |      0.270 |          0.398 |             -11.1% |
| Local/global grouped writes       |             0.209 |              0.220 |      0.536 |          0.391 |              -4.7% |
| Varied integers from source bytes |             0.576 |              0.717 |      1.743 |          0.330 |             -19.6% |

The profile build remains optional: its workload tradeoffs appear in these tables.
The [nine-repetition paired comparison](../benchmarks/results/profile-delimiter-cursor-followup.json)
uses the retained profile interpreter at `b8d6991` at the largest sizes. Core
changes range from -6.3% to +2.0% across all nine cases; broader changes
range from -2.7% to +2.0% across all nine cases. Overlapping delimiters
use 6.3% less CPU time. All 36 allocation
samples match the preceding build in both ordinary and profile adapters.
Whole-binary comparisons include code placement; small differences need the
control-run variation context above.

The timed comparison's profile execution text matches the final profile build;
ordinary execution text matches the preceding ordinary binary. Both identities
and each measured binary's source hashes are retained with the results. The
[definition-storage comparison](../benchmarks/results/definition-storage-followup.json)
retains the earlier header-size change, paired timings, and instruction counts.

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

43 Rust tests cover the core plus token-register retention, protected
and opaque insertion, conditional selection, deferred token order, case changes,
conversion categories, expression rounding/scaling/error order, generated-buffer
reuse, numeric and expansion-depth boundaries, fixed symbol names, and long/nested
dynamic names. Forced hash collisions also check distinct spellings at lengths
2–33, every differing byte, first-bucket matches, and lookup after table growth.
The forced-collision fixture and real hash-fragment collision fixture both pass
Miri with `native-profile` enabled. The real fixture retains two spellings whose
64-bit hashes differ but share a 32-bit fragment, checking source and dynamic
lookup, table growth, local replacement, and restoration against native pdfTeX.
Serialization also checks one-byte, ordinary, and 4,097-byte spellings while
its emitter interns 1,500 new names. The output retains the original spelling
and trailing space through descriptor and bucket growth. This fixture passes
Miri with `native-profile` enabled.
The native oracle compares 484 marked messages and 26 rejected
programs. Expected extended-fixture messages were captured from native pdfTeX.

Conditional character-run fixtures compare 72 generated messages across four
shifted arena positions and lengths 0, 1, 4,093, 4,095, 4,096, and 8,193. They
exercise nested conditionals in substituted parameters, aliases for conditional
boundaries, translated active characters, and a blank line whose paragraph token
is an alias for `\else`. Both stored and source-byte routes match native pdfTeX.
The Rust fixture also passes Miri across these shifted windows.

Short-number fixtures compare singleton windows, spaces and spacer aliases,
case-changed spacer characters, empty and parameterized macro continuations,
and an unexpandable control-sequence terminator with native pdfTeX. Operand
cursor tests cross shifted arena-window ends, retain nonspace terminators, and
check digit categories, signs, multiple digits, and failed-scan progress. This
integer boundary and progress test also passes Miri. File-operand tests start
with loaded and unread lines, retain terminating tokens, check overflow progress,
and exercise changed digit categories and inactive or digit end-line characters.
They also check signed multi-digit fields, category changes inside a field,
superscript-translated digits, and a terminating character whose category changes
after the scan. Its already-read token keeps its original category.

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
The stored chunk/capture test and failure-position test also pass Miri,
including the profile failure-table cursor and its short fallback borrows.
One-word group cases move the opener, word, and closing brace through a chunk
boundary, substitute an inline parameter, and retire captured backing at the end
of its source. Native pdfTeX agrees on their nine generated comparisons. These
cases, inline-word promotion, and discarded-argument scanning also pass Miri.
Grouped character-run fixtures check lengths 0–12 and 255–257 at four shifted
arena positions. Their 64 native comparisons cover retained and discarded
arguments from source bytes and stored tokens, nested forwarding, and the tokens
following each closing brace. They verify that structural and following tokens
stay available when a character run ends at a batch boundary. The Rust test also
passes Miri with the four-word window reads, including scalar tails and
structural words within the peeked window.

A decimal-source test covers both signs, every power-of-ten boundary, and the
full signed 32-bit range. It stacks sources before reading them and repeats the
sequence after retirement, checking that pending sources retain their backing
while subsequent numbers are generated and that retired buffers can be reused.
This test also passes Miri.

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

Measured on 2026-10-08, Linux x86-64, AMD EPYC 7642, pinned to CPU 0:

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
