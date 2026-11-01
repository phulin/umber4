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

Math-mode entry, math-character execution, family selectors, and math list
construction remain unsupported. Executing a math-character meaning returns
`Unsupported` without creating a text glyph. The construction and conversion
sections below remain proposals. The compatibility reference is TeX's
`def_code`, `shorthand_def`, and `scan_something_internal` in the
[TeX source](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web).

Math-glue registers, parameters, and typed `mu` scanning/formatting are executable
initialization state, as defined in [layout-foundation.md](layout-foundation.md).
Their values retain math units until a future math-layout consumer converts them.

## 1. Math construction

Math execution builds a list of TeX atoms, commonly called noads, directly in
node storage. A noad has its class and typed nucleus, subscript, and superscript
fields: math character, sublist, box, or empty. Fractions, radicals, accents,
delimiters, choices, style changes, and explicit material have their own compact
records. Common noads should not carry the largest extension payload inline.
Allocate temporary noads and choice records in their own short-lived chunk class
within the document node-ID store. Surviving characters, boxes, and ordinary nodes
use their destination class. Once atoms are released, fully free temporary chunks
can leave the root before a checkpoint instead of leaving holes among results.

The math semantic nest owns the current list, incomplete fraction state, and
context needed until its delimiters close. Field-building groups own script or
subformula lists until transferring them to the selected field. `\mathcode`,
`\delcode`, `\fam`, and current meanings affect construction at the points TeX
specifies; math-active characters return through normal expansion.

Keep the distinction between a math character's stored family/code and the
font/size selected when conversion uses it. Math font bindings and parameters
remain document state. An already built formula is not retokenized after those
bindings change.

e-TeX's `\middle` closes and reopens the left/right semantic group while keeping
the accumulated list. It restores bindings at that boundary and resets conversion
style and binary context as upstream specifies. The math owner retains the latest
delimiter handle needed by incomplete fractions.

`\mathchoice` executes construction of all four alternatives, including their
assignments and other effects. Later conversion selects the alternative for the
current style. Build the four lists in their owned destination; they are not four
speculative engine branches or delayed programs.

## 2. Conversion to horizontal material

Use an ordinary `mlist_to_hlist` function with the required recursive calls for
subformulas. Current style, cramped state, font size, and intermediate dimensions
are locals. Scratch arrays or marked arenas hold temporary traversal work.
Resulting boxes, kerns, glue, and characters are initialized in destination node
storage and their lists are linked directly.

Preserve TeX's interleaved first pass: binary-atom cancellation happens at the
specified points before ordinary-atom processing, including retrospective
cancellation, while atoms are constructed and measured. The second pass inserts
class-dependent spacing and permitted penalties.
Style changes alter the appropriate later work. Operators and limits, fractions,
scripts, accents, radicals, and delimiter sizes use TFM parameters and integer
rules. Reuse an existing box or list where ownership permits it; conversion need
not first flatten the math list into an unrelated render tree.

Delimiter search uses the specified family/size order, larger-character chains,
and extensible recipes. Construct repeated pieces directly into the final box.
Keep italic corrections, skew behavior, script clearance, and rule thickness
separate from text space-factor rules. The exact calculations are compatibility
code, not interchangeable typographic approximations.

TeX's `mlist_to_hlist`, `make_fraction`, `make_scripts`, and `var_delimiter`
provide the reference behavior.
See the [TeX math implementation](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web).

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
