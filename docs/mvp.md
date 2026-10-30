# Expansion and execution MVP

This slice makes the execution path in [expansion.md](expansion.md) and
[execution.md](execution.md) executable. It runs small TeX programs that expand
macros and change integer state, then report their result with `\message`.
Shipout, fonts, layout, format files, and document checkpoints come later. This
slice has no checkpoint or scheduler yield implementation yet.

The slice is implemented in [crates/umber](../crates/umber/). Its
[native measurements](mvp-results.md) record the tested behavior, optimization
results, and reproduction commands. The [broader expansion slice](expansion-mvp.md)
adds token registers, conversions, further conditionals, deferred tokens, case
changes, and integer expressions. The table below records the original baseline.

## Scope

Use byte-oriented INITEX input and its initial category table. Programs explicitly
set the category codes of braces and `#` before defining macros, just as the same
source must do in native INITEX. Assignments must change later characters on the
current line. The current implementation reads lazily; line-local lookahead is
proposed in [input.md](input.md). Prepare
physical lines, handle comments/spaces,
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
built directly in stable chunks retained by their immutable value. Common token
sources read one contiguous span; source changes do the ownership work.

Definitions retain their flags, literal parameter prefix, delimiters, and body
coordinates. Their stored signature preserves parameter marker spelling and the
final `#{` convention for equality and serialization. Delimiter metadata is built
after the signature, and stays live with the definition. Expanded-list destinations
write token values or formatted message bytes directly.

Invocation packets own argument sequences and their backing owners. Existing
ranges are retained; a singleton can stay inline. File tokens materialize directly
in capture storage. Parameter-free calls install the selected body without a
packet. Bodies that ignore parameters still scan and validate every argument but
need not retain the captures. Retire exhausted sources before installing a
successor, after any forwarded ranges have acquired their new owner.

Stored-token windows let argument, numeric, and list scanners advance direct
pointers through ordinary runs. Structural tokens, expansion, substitutions, and
source changes return to the complete reader. Fast paths preserve token consumption,
current meanings, one-read suppression, and failure order. Backing storage remains
owned while a raw cursor window is installed.

Grouping uses a reused save stack, preserving local/global/local restoration and
the extended profile's identical-local-assignment behavior. Saved values transfer
ownership where possible. Control-sequence tokens keep identities; meanings are
resolved when read. Macro equality compares flags and complete signatures, with
immutable-window comparisons and no temporary flattened list.

## Allocation target for this slice

[storage.md](storage.md) defines the final requirement: fixed-working-set execution
reuses backing capacity, including metadata. The first bounded allocator revision
keeps one 4,096-token active bump block for durable definitions/register values
and another for transient captures/generated input. Both use the same builder,
window traversal, and shared empty-block cache. Recycling retains each complete
block record and its non-atomic shared ownership counter, avoiding a fresh wrapper
allocation when token backing is reused. The cache holds at most sixteen empty
blocks; actual owners retain checked-out blocks independently of that bound.

Allocation class never overrides an escaped value's actual owners. Single-token
arguments stay inline; larger ranges remain immutable and can be retained across
producer replacement. Capturing existing input keeps its backing owner's class;
it does not move or relabel the block. Newly built definitions and register values
write into durable storage. Small-copy promotion remains optional when a concrete
retention case justifies it.

Value IDs/headers and packet capacity are reused, but value-header growth still
allocates individual shared records. Definition parameter/fallback vectors and
multi-piece value descriptors can still allocate on repeated construction; moving
those variable records into reusable stores remains the next allocator work.
Historical counts in [mvp-results.md](mvp-results.md) are not a zero-allocation claim.

Publication transfers initialized storage without flattening. Failed builders
release their reservations. Reused IDs cannot become visible until old owners and
views have disappeared. Preserve the existing ability for a retained value or
builder to outlive its registry by retaining the necessary allocator backing at
store/slab granularity. No per-token ownership operation is introduced.

Validate repeated replacement of fixed live state, mixed short-lived work with
durable survivors, active definitions during redefinition, forwarded arguments,
and registry teardown. Count system allocations separately from reusable-store
operations; check CPU throughput as well as retained capacity. This slice does not
add layout, checkpoint roots, or a browser scheduler merely to exercise allocation.
The next architectural slice connects those owners to real paragraph/page state.

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
