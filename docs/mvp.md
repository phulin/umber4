# Expansion and execution MVP

This slice makes the execution path in [expansion.md](expansion.md) and
[execution.md](execution.md) executable. It runs small TeX programs that expand
macros and change integer state, then report their result with `\message`.
Shipout, fonts, layout, format files, and document checkpoints come later. There
is no paragraph handoff yet, so this slice has no scheduler yield point.

The slice is implemented in [crates/umber](../crates/umber/). Its
[native measurements](mvp-results.md) record the tested behavior, optimization
results, and reproduction commands. The [broader expansion slice](expansion-mvp.md)
adds token registers, conversions, further conditionals, deferred tokens, case
changes, and integer expressions. The table below records the original baseline.

## Scope

Use byte-oriented INITEX input and its initial category table. Programs explicitly
set the category codes of braces and `#` before defining macros, just as the same
source must do in native INITEX. Tokenize lazily so assignments can change later
characters on the current line. Prepare physical lines, handle comments/spaces,
control words/symbols, active characters, and `^^` translation through one reader.

The initial primitives are:

| Area                   | Primitives                                                              |
| ---------------------- | ----------------------------------------------------------------------- |
| Definitions            | `\def`, `\gdef`, `\edef`, `\xdef`, `\long`, `\let`                      |
| Groups and assignments | braces, `\begingroup`, `\endgroup`, `\global`                           |
| Integer state          | `\count`, `\countdef`, `\advance`, `\multiply`, `\divide`               |
| Character input        | `\catcode`, `\endlinechar`                                              |
| Expansion              | `\expandafter`, `\noexpand`, `\csname`, `\endcsname`, `\number`, `\the` |
| Conditionals           | `\ifnum`, `\ifx`, `\iftrue`, `\iffalse`, `\else`, `\fi`                 |
| Driver and result      | `\relax`, `\par` in vertical mode, `\message`, `\end`                   |

Support literal parameter prefixes, up to nine parameters, undelimited arguments,
balanced grouped arguments, and delimited arguments. Repeated substitution reads
the captured ranges again. Expanded definitions use the same expansion functions.
`\let` copies a meaning; `\ifx` compares the defined meanings and complete macro
contents, rather than just definition IDs. Count indices are 0–255 in this slice.
Integer scanning includes signs, decimal/octal/hexadecimal constants, character
constants, and internal quantities. `\the` and internal scans accept count
registers/aliases, category-code entries, and `\endlinechar`.
Support final `#{` parameter syntax and aliases to parameter characters, preserving
the marker spelling for `\ifx`. Brace and `\begingroup` groups have distinct kinds.

This is a register/expansion interpreter. Executing text, constructing lists, or
using an unsupported primitive returns an error. Invalid input fails at the
detected operation; interactive recovery and detailed TeX diagnostics are outside
this slice. Those limits apply equally while optimizing the implementation.

## Execution and storage

The Rust driver reads a token and its current meaning, expands it when needed,
and executes each unexpandable command before continuing. This fused loop keeps
main-loop reads and dispatch in one Rust function. `next_expanded` provides the
same iterative macro expansion to operand scanners. Operand scans, conditional
evaluation, argument matching, and expansion-order operations are Rust calls
with local progress. Pending sources contain tokens and cursors.

Use 32-bit token words and a dense meaning table with assignment levels. An
append-only byte interner supplies stable names for a run. Definition tokens are
built directly in session-owned stable chunks. Common token sources read one
contiguous span; source changes do the ownership work.

Invocation packets own argument sequences and their backing capture chunks.
Existing token ranges are retained; newly scanned source tokens are built in
the packet's inline slot or capture buffer. A captured range retains its backing
directly, so retiring a parent does not invalidate it. Reuse packet and capture-buffer
capacity. Reusing a packet with the same argument count takes a small inline
path; resizing and allocation use a separate helper. Parameter-free invocation
installs the existing body without allocation or reference counting. Retire
exhausted sources before installing a successor.

Definitions record whether their replacement tokens substitute any parameter.
Calls always scan and validate all arguments. A body using no parameters can
consume the arguments without retaining a packet or capture storage. Select this
scanning mode once per invocation; the token loops use the same matching rules.
Delimited matching runs in a separate Rust function, keeping its local matching
state and group trimming out of the common undelimited scanner.
When no delimiter prefix is pending, file input scans character runs up to a
possible first delimiter word. Inside a group, the run follows the group's
ordinary character rules. Runs append into the existing capture buffer; their
consumed length updates group-trimming counters at the boundary. Partial matches,
braces, control sequences, and source transitions use the surrounding matcher.
Retained stored input scans ordinary token runs within an immutable window,
recording one range at each boundary. A run stops before a possible first
delimiter word, a brace, a control sequence, or parameter substitution. An inline
word is promoted through the ordinary capture path before looking ahead, so
promotion precedes later matching errors. Partial prefixes and window transitions
return to the ordinary reader and matcher.

Grouped arguments scan an immutable source window directly, retaining one range
at its boundary. Ordinary character words use one range classification before
advancing the pointer; braces, control sequences, and parameter words take the
structural path. Parameter substitution and source advancement resume this scan
at the next window. One-token arguments stored as ranges return the existing
token and range directly while the body retains their owner, avoiding a temporary
input source.

A single newly read token fits directly in its argument packet as a token value.
Substitution carries that identity without backing ownership. If another token
or range follows, move the first value into capture storage and continue building
there; borrowed ranges retain their existing backing. This keeps single-token
arguments small without changing the construction path for larger arguments.

When grouped capture finishes with exactly one token and an empty destination,
keep that word directly in the packet as well. The source window already supplies
its value; repeated substitution then reads the packet's value. Larger and mixed
captures continue retaining their immutable ranges.
Retained arguments beginning with an ordinary character followed by a closing
brace in the same window can finish capture directly. An inline parameter word
qualifies when it supplies an ordinary character. The invocation keeps the
resulting word in its inline slot. Discarded arguments use the window scanner
directly.

Grouped arguments from source bytes append ordinary character runs directly into
the uniquely owned capture buffer, recording one contiguous range per run.
Buffer lookup and range bookkeeping happen at run boundaries. A first word stays
inline until more content follows; discarded arguments scan runs without storage.
Special categories and line changes retain the ordinary reader's scanning rules.

File bytes are already resident in the native adapter. Ordinary bytes are
classified inline using the live category table. The rare superscript translation
path rewrites the prepared line, including committed control-word lookahead;
token construction remains lazy across executed assignments.
A newly interned name extends the dense meaning table through a separate growth
helper, keeping allocation and initialization out of ordinary reader loops.
Control-word scanning computes the interner hash from each consumed letter byte.
Superscript translation commits that byte to the prepared line before hashing.
Single-byte names use their fixed symbol ID; multi-byte names enter the shared
lookup with the hash of their completed spelling. Dynamic name construction uses
the same hash operation.
List scanners consume ordinary letter and other-character runs directly from the
current source line and emit into their destination. Special categories and line
transitions return to the complete reader. Each emitted byte advances the cursor
and sets the lexical state before emission, preserving consumption on failure.

Keep grouping in a reused save stack, preserving local/global/local restoration
and the extended profile's identical-local-assignment behavior. Character tokens
keep their captured category codes. Backing up a token preserves identity, while
its next read resolves its current meaning and any one-read suppression is spent.

After the first digit, integer scans can walk literal digits in a stored-token
window. They resolve the following token's current command: a terminating space
is consumed, while an unexpandable nonspace token remains at its cursor for the
next operation. Expansion, parameter substitution, suppression, and source
transitions use the ordinary reader. Numeric progress stays on the Rust stack.

The implemented arena uses 4,096-token boxed chunks. Published tokens are
immutable; fresh suffix writes use `UnsafeCell` and chunk addresses remain stable
when the directory grows. A source window covers initialized cells in one chunk.
Capture buffers grow while uniquely owned; publication gives their packets shared
ownership and makes the buffers immutable. A child retains that backing before
the parent source can retire. Source pointers remain valid through those semantic
owners. Checked name and storage indices preserve the compact token layout.

Macro equality keeps the meaning, command, flag, and length shortcuts, then
compares signature ranges by immutable chunk windows. Tokens have a transparent
32-bit word layout, allowing direct word-slice equality. Each step compares the
shorter current window and advances both ranges, without copying or allocating.

Start with ordinary Rust storage. If direct cursor access needs a small unsafe
primitive, document its initialized bounds, stable backing owner, and source
retirement rules. Keep that mechanism independent of TeX dispatch.

## Compatibility before timing

Use upstream pdfTeX with `-ini -etex`, with explicit category bootstrap,
as the oracle. Keep small fixtures for definition redefinition during invocation,
meaning snapshots, repeated/nested arguments, delimiter overlap, expanded
definitions, suppression and expansion ordering, category changes, conditional
skipping, numeric consumption, and group restoration. Compare final registers and
message contents, plus success/failure where the supported subset defines it.
Control-name fixtures exercise source/dynamic identity, translated letters at
each position, recursive translation, committed nonletter lookahead, and byte
spellings containing NUL or high bytes.

Rust tests cover those semantics and storage lifetimes; an oracle runner executes
the same source with both binaries. Build the portable library for native and
`wasm32-unknown-unknown`. The first command-line adapter reads one source and
publishes messages; browser adapters and format startup remain separate work.

The upstream references are the input, macro, scanner, conditional, and save-stack
sections of [tex.web](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web)
and the extended behavior in [pdftex.web](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/pdftexdir/pdftex.web).

## Performance work

Compare release binaries on identical complete source programs, ending with one
checksum message and `\end`. Use several workload sizes and alternating engine
order. Report full-process times and growth with work, with startup visible in
the small cases. Compact macro fanout makes both engines reuse stored tokens;
unrolled source separately measures file scanning and argument materialization.

The workload set covers raw primitive dispatch, parameter-free macro bodies,
integer assignments, repeated/nested parameters, delimiter matching, and grouped
local/global writes. Results must agree before their timing counts. Measure
allocations separately in a diagnostic run so counters stay out of normal loops.

Optimize whichever actual path loses to pdfTeX. Inspect release instructions for
token loads, meaning resolution, cursor push/pop, argument construction, and
assignment dispatch. Remove redundant classification, token copies, allocations,
and oversized source records before adding specialization. Rerun the relevant
semantic fixtures after changing a representation or fast path.

Completion means a working, tested slice and measured pdfTeX-comparable throughput
across these core workloads. Record the oracle/compiler versions, source generator,
machine, timing variation, and remaining costs. These results establish the core
loop's performance; full-document performance still requires layout and output.
