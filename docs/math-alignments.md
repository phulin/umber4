# Math lists and alignments

This proposal covers the two substantial list-building paths alongside ordinary
paragraphs. Their semantic groups remain explicit TeX state, while conversion,
packing, and scanning use ordinary Rust calls. They share the node stores and
packing primitives in [storage.md](storage.md) and [paragraphs.md](paragraphs.md).

## Executable math-code state

The interpreter supports `\mathcode`, `\delcode`, and `\mathchardef` as
initialization state independently of math construction. Character-code indices
are 0 through 255. Math codes are 0 through 32768, including the math-active
value `"8000`. Delimiter codes accept any negative integer supplied by the
integer scanner and nonnegative values through `"FFFFFF`.

INITEX math codes equal the character byte, except that digits add `"7000` and
ASCII letters add `"7100`. Delimiter codes start at -1, except that `.` starts
at zero. Both tables use the existing scalar slots and assignment save levels;
local/global assignments, `\globaldefs`, and checkpoints follow the same rules
as the other character-code tables. Checkpoints copy their fixed scalar array;
no additional snapshot root is needed. Reads return integers through `\the`,
`\number`, and internal integer operands.

`\mathchardef` creates a distinct math-character meaning with a value from 0
through 32767. Its target becomes `\relax` before the optional equals sign and
value are scanned, so scanning failure leaves that temporary meaning in place.
Aliases, `\ifx`, numeric reads, and hexadecimal `\meaning` output preserve the
math-character meaning independently of ordinary `\chardef` characters.
Out-of-range assignments return `Number` instead of upstream's interactive
recovery; a failed character-code assignment preserves its preceding value.

Inline math entry, math-character execution, and the construction subset below
are executable. Font-family bindings use ordinary scoped assignment state.
Math-only commands outside math fail explicitly; missing-dollar recovery and
display execution remain unsupported. The typed conversion core also remains
usable independently of input execution.
The compatibility reference is TeX's
`def_code`, `shorthand_def`, and `scan_something_internal` in the
[TeX source](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web).

Math-glue registers, parameters, and typed `mu` scanning/formatting are executable
initialization state, as defined in [layout-foundation.md](layout-foundation.md).
Their finite values retain math units until conversion uses the current symbol
font quad.

## 1. Math construction

The typed core uses one reusable `MathStore` arena backed by the existing typed
metadata buffer pool. Numeric `MathId` references address cells in that arena;
noncopyable `MathList` owners carry head and tail. Atom fields own a character,
an ordinary detached box, a subsidiary math list, or no value. An empty subsidiary
list is distinct from an absent field. Choices own four math lists. One enum
stores atoms, styles, choices, explicit ordinary material, and converted results;
there are no per-atom allocations, reference counts, packed tags, or cold stores.
Freed cells form an arena-local free list and are reused across formulas.

`Engine::math_to_hlist` consumes a math list, initial style, and penalty policy,
using caller-retained conversion scratch. Its only semantic mutations are in the
supplied math arena and the engine's ordinary node store. Fonts, bindings, scanner,
modes, and page state are not changed. The result is an affine ordinary `List`.
Raw IDs never own anything and must not survive release or cross arena/store
versions. Ordinary box fields must belong to the engine's matching node store.
Dropping numeric owners does not traverse children; callers convert or explicitly
release them through their stores.

The current core supports all eight spacing classes, operators with normal or
explicit limits policy, eight styles including cramped styles, choices, ordinary
math ligatures/kerns, scripts, boxed/submath fields, and explicit ordinary
kern/glue/rule/penalty material. It also converts fractions, radicals, overline,
underline, vcenter, accents, and paired standard left/right delimiters. Middle
delimiters, displays, and equation numbers remain
unsupported. Their absence does not select an approximate rendering.

Active math construction is excluded from quiescent checkpoints. The arena and
traversal scratch therefore require no COW or snapshot roots. Converted ordinary
nodes use the existing versioned node store and its checkpoint ownership.

The inline frontend has separate typed math frames, with one reusable arena and
conversion scratch. Each frame owns its list and either the suspended enclosing
text state or a stable atom-ID/field destination. Ordinary box frames suspend
math mode while preserving the outer math owners; nested math uses the same
arena without rewinding any live IDs. Completed nonvoid boxes become Ord nuclei,
while void box transfers append no atom. Restoring a checkpoint abandons active
math headers with their old ordinary-node root before installing restored roots.

Entry uses one unexpanded lookahead token. An immediately following shift in
unrestricted horizontal mode returns `Unsupported` for display math before
installing a math frame; restricted `$$` closes an empty inline formula. Vertical
entry replays the shift after paragraph entry and its hooks. Inline entry opens
its group, locally sets `\fam=-1`, and installs `\everymath`. Math and subsidiary
mode are inner modes for conditionals, while a contained hbox has ordinary
restricted horizontal mode.

Characters, `\char`, math-character meanings, and `\mathchar` capture their family
and character at scanning time. Class-seven codes become Ord, with a current
family override only when `\fam` is 0 through 15. Math-active characters return
through ordinary expansion. Field scanners skip spaces/relax and accept a single
math character or a braced subsidiary list. A singleton scriptless Ord unwraps
to its nucleus; empty subsidiary lists remain distinct from absent fields.
Raw braces build these fields, while `\begingroup` only scopes assignments.
Scripts attach to the immediate eligible tail or a new empty Ord; duplicate
scripts fail without overwriting an existing owner. Eight spacing classes,
explicit styles, and operator limit switches are executable.

`\delimiter` scans a nonnegative 27-bit code. Ordinary execution and immediate
field scanning use its upper 15 bits as a math-character code, with the existing
class/family rules. Delimiter operands for `\left` and `\right` instead skip
expanded spaces/relax and accept letter/other characters through `\delcode`, or
an explicit `\delimiter` code. Ordinary character definitions, `\char`, and math
character commands are not delimiter operands. Math-active mathcodes do not
affect a letter/other delimiter lookup. The low 24 bits retain small/large
family/character fields without a `\fam` override. Missing/negative delimiter
lookups back the offending token and fail; out-of-range explicit codes return
`Number` without interactive recovery.

A left/right frame owns its list from the initial Left boundary until a matching
Right boundary closes its typed group. Completion transfers that list into an
Inner subsidiary nucleus after restoring the left group's bindings. It does not
convert the subsidiary list: assignments after `\right.`, including plain's
`\n@space`, still affect conversion of the completed formula. Scripts attach to
the resulting Inner atom, or to a new empty Ord immediately after an opening
boundary. Braces, shifts, and `\endgroup` cannot close a left/right group. An
extra `\right` in the formula's MathShift group scans its delimiter before failing;
a mismatched intervening group fails before that operand is scanned. Boundary
temporaries are released on scanning/group-entry failure, independently of live
outer math and box owners.

Assignments, definitions, and diagnostics retain their ordinary execution paths.
Physical kern/glue/rule/penalty material retains ordinary units. `\/` contributes
a normal zero kern in math. `\mkern` retains math units in a scalar arena node
until the conversion pass uses the active symbol-family quad and native signed
truncation. Conversion and both `\mathsurround` widths use the live formula
bindings before its group is restored. The resulting ordinary list is enclosed
by before/after math nodes; the enclosing space factor becomes 1000. Automatic
math penalties are enabled only for an enclosing unrestricted paragraph.

Closers validate the current semantic group before restoring bindings or moving
owners. Active math frames exclude checkpoint handoff. Scanned choices,
fractions, radicals, decorations, accents, mu glue, displays,
and alignments remain unsupported; their pure conversion support is separate.
Future fraction frames will own incomplete fractions until their
semantic groups close.

Keep the distinction between a math character's stored family/code and the
font/size selected when conversion uses it. Math font bindings and parameters
remain document state. An already built formula is not retokenized after those
bindings change.

e-TeX's `\middle` closes and reopens the left/right semantic group while keeping
the accumulated list. It restores bindings at that boundary and resets conversion
style and binary context as upstream specifies. The math owner retains the latest
delimiter handle needed by incomplete fractions.

Future `\mathchoice` execution constructs all four alternatives, including their
assignments and other effects. Later conversion selects the alternative for the
current style. Build the four lists in their owned destination; they are not four
speculative engine branches or delayed programs.

## 2. Conversion to horizontal material

Conversion uses an iterative work stack instead of Rust recursion. Frames own
subsidiary lists and partial ordinary outputs until transferring them exactly once
to their parent. Clean-box work, scripts, operators, and nested math lists use the
same stack. Success and failure leave scratch empty; failure releases the disjoint
forest of pending math lists, ordinary boxes, and partial outputs. There is no
additional nesting bound beyond storage address capacity. Reusable release work
also traverses children iteratively.

The converter checks that all three family-2 fonts have 22 parameters and all
three family-3 fonts have 13 parameters before consuming layout work. Undefined
used families fail with `Font`; absent glyphs are discarded as in text layout.
Interactive recovery and lost-character diagnostics remain outside this contract.
Integer operations preserve TeX's order, truncation, half rounding, and finite
mu conversion; infinite glue components are not scaled by the mu unit.

Preserve TeX's interleaved first pass: binary-atom cancellation happens at the
specified points before ordinary-atom processing, including retrospective
cancellation, while atoms are constructed and measured. The second pass inserts
class-dependent spacing and permitted penalties.
Style changes alter the appropriate later work. Operators, limits, scripts,
fractions, radicals, and delimiters use the same immutable font resources and
integer rules. Existing boxes and lists transfer directly wherever ownership
permits; conversion does not flatten them into a separate render tree.

TeX's `mlist_to_hlist`, `make_ord`, `clean_box`, `make_op`, `make_scripts`,
`make_fraction`, `make_radical`, `make_over`, `make_under`, `make_vcenter`,
`var_delimiter`, `make_math_accent`, `rebox`, and `math_glue` provide the reference behavior.
See the [TeX math implementation](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web).

### Fractions, decorations, and delimiters

One affine `Fields` record carries nucleus and script fields for atoms and
scriptable constructions. Fractions instead own numerator/denominator math lists,
two delimiter specifications, and a typed default or explicit rule thickness.
A delimiter specification retains small/large family and character bytes until
conversion; it does not freeze a selected glyph. All forms remain variants in
the same reusable arena. No auxiliary per-formula store or reference-counted
construction object is introduced.

Fraction and decoration tasks use the existing iterative clean-box work. A
finished decorated nucleus transfers to the ordinary script task without a
synthetic math node or another traversal framework. Every suspended field, clean
box, partial list, and completed fraction has one owner, including on failure.
Vcenter requires a vertical box nucleus and rejects an invalid typed input with
`Syntax` while reclaiming its fields. Fractions have no direct script fields;
a surrounding atom owns scripts on a grouped fraction.

A standard delimited list begins with a left boundary and ends with a right
boundary. Nested delimiter pairs belong to subsidiary lists; the surrounding
Inner atom is explicit. Unpaired or misplaced boundaries fail with `Syntax`.
Boundaries own no scripts and remain unconstructed during the first pass. The
first pass measures completed noads and explicit rules, preserving binary
cancellation and style order. The second pass constructs both delimiters using
the original list style and measured maximum height/depth. Right-boundary style
restoration follows e-TeX even though middle delimiters are unsupported.

Delimiter search probes the small specification across the requested size down
to text size, then the large specification in the same order. Undefined families
are skipped; absent characters stop a larger-character chain. The first usable
extensible candidate ends the search. Otherwise the first sufficiently large
candidate or largest available candidate wins, with a null-delimiter box when
none qualifies. The target uses division before multiplication, as in TeX.

Extensible recipes are borrowed from already validated immutable TFM bytes.
Optional zero top/middle/bottom pieces are absent; the repeat piece is required.
Nonpositive repeat extent produces no repetitions. Components are constructed
directly in the ordinary result list. The outer width comes from the repeat
piece, height from its topmost emitted component, and depth from total extent.
Character boxes retain signed metric height/depth, including values that natural
horizontal packing would clamp. All delimiter boxes are centered on the current
math axis. Running rules retain the existing ordinary rule representation.

Fraction styles, equal-width reboxing, null delimiters, default/zero/explicit
thickness, and clearances follow `make_fraction`. Overline and radicals clean
cramped nuclei; underline uses the current style and resets packed height/depth.
Radicals use their selected delimiter height as overbar thickness. Vcenter
redistributes the existing vertical box height/depth around the math axis.

### Math accents

An accent owns a family/character specification and the same affine `Fields` as
other scriptable forms. Accent lookup happens before nucleus lookup. An absent
accent glyph leaves ordinary nucleus/script conversion intact; an undefined used
family fails with `Font`. Lookup of a character nucleus may discard a missing
nucleus glyph independently of the accent. No font or scanner state is changed.

Skew comes only from a kern instruction matching the nucleus font's skew character.
Ligature instructions, boundary programs, font slant, and the accent font's skew
character do not supply skew. Larger accents follow that font's larger-character
chain while glyphs exist and their metric width fits the original clean nucleus;
selection does not use delimiter-family search or extensible recipes.

The first clean nucleus uses cramped style. A character nucleus with present
scripts transfers its nucleus and both scripts into subsidiary Ord work in the
original style, then cleans that result. The outer script fields become empty.
The original nucleus width still determines accent placement; the rebuilt nucleus
width determines the final result width. Existing iterative atom/clean-box work
performs this transfer without a synthetic arena node or another allocation
framework. Noncharacter nuclei retain scripts on the outside.

The accent box is horizontally shifted by skew plus half the width difference,
then contributes zero width. Clearance uses the accent font's x-height and is
adjusted after a scripted character nucleus is rebuilt. Vertical packing preserves
the cleaned nucleus width and repairs insufficient final height with a leading
kern. All intermediate boxes and partial lists have affine owners through success,
lookup failure, allocation rejection, and suspended-work cleanup.

### Typed-core validation

Ordinary tests use controlled in-memory TFM resources and typed ASTs. They cover
arena and ordinary box-slot reuse, absent versus empty fields, font failures,
capacity rejection, spacing overflow, deeply nested conversion/release, and every
suspended ownership handoff. A retained unrelated ordinary box, checkpoint, and store version survive
conversion failures and both engine/store drop orders. These tests require no host font files.

The ignored native adapters construct ASTs and independent TeX source expressions.
They compare natural box dimensions, shifts, glue settings, and DVI glyph positions
through the existing ordinary packing/publication adapter. Cases include the full
spacing-class matrix, all math ligature operations, choice/style propagation,
eight styles, signed parameters, scripts, operators/limits, missing glyphs, boxed
fields, fractions, radicals, decorations, growing delimiters, accents, and real CM
fonts. Accent cases cover skew versus ligatures, font separation, width-chain
termination, empty scripts, missing glyphs, original-style script transfer, and
height repair.
Native-only overline wrappers expose cramped root styles; comparisons select their
inner box and relative formula glyph/rule positions, excluding the wrapper rule.
Controlled recipe cases cover optional and zero repeat pieces, nonpositive repeat
extent, negative metric geometry, search fallback, and division thresholds.

Set `UMBER_PDFTEX` to an explicit pdfTeX executable and, for CM cases,
`UMBER_TFM_DIR` to the TeX distribution's `fonts/tfm/public` directory, then run
`cargo test --lib math::oracle_tests -- --ignored`. The oracle is pdfTeX 1.40.25
(TeX Live 2023/Debian), invoked as INITEX with e-TeX. Adapter files are isolated in
`umber-math-accents-*` temporary directories and removed afterward. Portable tests,
Miri, and Wasm builds do not invoke this host adapter.

## 3. Inline and display math

Inline math transfers the converted horizontal list and surrounding math boundary
nodes into its paragraph. Its math atoms can then be reclaimed, while resulting
nodes have the paragraph/list lifetime.

Display math interacts with the preceding paragraph, display width/indent,
equation numbers, short versus ordinary display skips, penalties, and the
following paragraph's line-number context. Use the upstream finishing order and
pack directly over the formula list. Alignment-in-display cases share the
alignment finalizer rather than a second display renderer.

The extended group and lookahead rules follow the
[e-TeX change file](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/etexdir/etex.ch),
alongside the [pdfTeX implementation](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/pdftexdir/pdftex.web).

A paragraph break performed while entering a display does not yield from the
middle of `init_math`. The primitive returns after establishing its math group;
that unfinished math construction excludes a checkpoint. Once execution returns to
eligible external vertical mode, the driver can take a checkpoint. Display-layout
locals have already disappeared.

## 4. Alignment state and templates

An alignment owns a preamble, column measurements, span records, rows, and current
entry information until the alignment group closes. This is TeX's semantic state
for `\halign`/`\valign`, not a scanner continuation. Nested alignments push an
alignment owner and restore the enclosing one on completion.

Scan preamble tokens into alignment-owned immutable chunks using the same span
builders as definitions and arguments. Completed templates and their consuming
cursors own those chunks. A definition or argument capturing a template range
retains its actual backing chunk directly, so it can outlive the alignment.
Each column has its before/after-parameter
templates and `\tabskip` state; repeated preamble regions are represented by
their template identities and position. The scan preserves TeX's selective
expansion, `\span`, parameter-marker, and `\cr` rules. It is an ordinary call
with local preamble progress.

An entry installs template cursors through the normal input reader, constructs
its list in place, and returns it at its semantic boundary. `\omit` bypasses the
templates at the specified point. The reader's brace/alignment accounting decides
when `&`, `\span`, and `\cr` end the current column's template use rather than
behaving as ordinary tokens. `\span` advances to the next column's templates
while keeping the same entry list and assignment group; a nonspan terminator
restores that group and packs the spanning entry. e-TeX's entry/column lookahead
uses the shared expanded-or-protected reader helper, so a protected macro cannot
bypass templates by revealing `\omit` during lookahead. Template-origin end
markers and outer-validity checks remain source-role
semantics in that same reader.

An entry group's restoration data retains the finishing context, while its
temporary operand scans stay on the Rust stack. There is no special expansion
machine for alignment templates.

## 5. Measuring and finishing rows

Pack each entry into an unset box, retaining its natural extent and stretch/shrink
totals. Update compact column measurements and span constraints. Ordinary columns
need direct indexed measurements; spanning entries need records for their actual
spans, not a dense matrix of every possible column pair.
Coalesce equal start-column/span-length constraints by their maximum width.

At `fin_row`, immediately natural-pack the row as an unset box, append it with
the specified baseline/previous-depth updates and extracted adjustments, and
only then execute `\everycr` or `\noalign`. Those commands can observe the row's
already-produced list state.

The alignment finalizer resolves column extents and tabskip glue using TeX's
algorithm. It walks existing rows and entries, retags their existing unset box
headers, and sets their dimensions and glue without rebuilding them. Span material
and running rules receive the dimensions
specified by that final pass. `\halign` and `\valign` use their corresponding
orientation while sharing the underlying measurements and packing helpers.

`\noalign` executes a real intervening group/list. `\everycr`, `\crcr`, repeated
preambles, missing terminators, and nested alignments enter the same execution
and recovery paths as upstream. Their effects are not postponed until a completed
table has been built.

TeX's `init_align`, `fin_col`, `fin_row`, and `fin_align` define these transitions.
See the [TeX alignment source](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web).

## 6. Lifetimes and checkpoints

Math and alignment working arrays retain capacity for reuse after their owner
finishes. Result lists transfer to the enclosing list, box register, or page.
Temporary atoms, unused choice lists, and alignment measurement records can be
released once their last operation completes. Group save records release their
binding owners separately from the resulting list.

Unfinished math/alignment construction excludes checkpoint eligibility. Ordinary
assignment groups alone do not. Once conversion/finalization returns to eligible
external vertical mode, completed material follows the node-store snapshot rules.
A missing resource unwinds and replays from the preceding checkpoint.

## 7. Focused compatibility and cost checks

Compare actual math lists, packed dimensions, and output for binary cancellation,
cramped styles, limits/scripts, fractions, accents, extensible delimiters, all four
math-choice branches, and display equation numbers. For alignments, use spans,
`\omit`, repeated preambles, changing `\tabskip`, `\noalign`, nested templates,
and errors that require token insertion or backing up.

Measure common-noad size, conversion allocations, template token copies, and row
finalization traversals. Large matrices and formula-heavy documents can reveal
costs hidden by text benchmarks. Optimize the actual list path while preserving
the algorithm and construction effects.
