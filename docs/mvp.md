# Expansion and execution MVP

This slice makes the execution path in [expansion.md](expansion.md) and
[execution.md](execution.md) executable. It runs small TeX programs that expand
macros and change integer state, then report their result with `\message`.
Shipout, fonts, layout, format files, and document checkpoints come later. There
is no paragraph handoff yet, so this slice has no scheduler yield point.

The slice is implemented in [crates/umber](../crates/umber/). Its
[native measurements](mvp-results.md) record the tested behavior, optimization
results, and reproduction commands.

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

The Rust driver reads an already-resolved unexpandable command, executes it, and
continues. `next_expanded` iterates ordinary macro chains. Operand scans,
conditional evaluation, argument matching, and expansion-order operations are
Rust calls with local progress. Pending sources contain tokens and cursors.

Use 32-bit token words and a dense meaning table with assignment levels. An
append-only byte interner supplies stable names for a run. Definition tokens are
built directly in session-owned stable chunks. Common token sources read one
contiguous span; source changes do the ownership work.

Invocation packets own argument sequences and their backing capture chunks.
Existing token ranges are retained; newly scanned file tokens are written once
into the packet's destination. A child capture retains backing storage directly,
so retiring a parent does not invalidate it. Reuse packet and capture-buffer
capacity. Parameter-free invocation installs the existing body without allocation
or reference counting. Retire exhausted sources before installing a successor.

Definitions record whether their replacement tokens substitute any parameter.
Calls always scan and validate all arguments. A body using no parameters can
consume the arguments without retaining a packet or capture storage. Select this
scanning mode once per invocation; the token loops use the same matching rules.

Grouped arguments scan an immutable source window directly, retaining one range
at its boundary. Parameter substitution and source advancement resume this scan
at the next window. One-token substitutions return the existing token and range
directly while the body retains their owner, avoiding a temporary input source.

Keep grouping in a reused save stack, preserving local/global/local restoration
and the extended profile's identical-local-assignment behavior. Character tokens
keep their captured category codes. Backing up a token preserves identity, while
its next read resolves its current meaning and any one-read suppression is spent.

The implemented arena uses 4,096-token boxed chunks. Published tokens are
immutable; fresh suffix writes use `UnsafeCell` and chunk addresses remain stable
when the directory grows. A source window covers initialized cells in one chunk.
Capture buffers grow while uniquely owned; publication gives their packets shared
ownership and makes the buffers immutable. A child retains that backing before
the parent source can retire. Source pointers remain valid through those semantic
owners. Checked name and storage indices preserve the compact token layout.

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
