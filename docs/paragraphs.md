# Fonts, horizontal lists, and paragraphs

This proposal develops the main text-layout path. Execution builds TeX nodes in
their destination store; paragraph algorithms use reusable working memory and
return finished lines to the enclosing vertical list. The storage interface is
described in [storage.md](storage.md).

The executable initial font/text contract is in [fonts-text.md](fonts-text.md).

## Executable paragraph baseline

The executable baseline completes external horizontal paragraphs with TeX82's
`line_break`, `try_break`, and `post_line_break`. It uses the existing node store
and immutable font metrics. It preserves active-list order, six relative width
components, fitness classes, demerits, equal-cost selection, pretolerance and
tolerance passes, emergency stretch, looseness, and final-pass recovery. Existing
discretionary nodes participate in every pass. Hyphenation tables can be loaded
and queried as described below; inserting automatic discretionary breaks into
paragraphs uses the bounded, ligature-aware reconstruction contract below.
The pretolerance pass leaves words unchanged; subsequent passes inspect only
words following glue actually visited by the line search.

Ordinary before/after math-boundary nodes retain their signed widths and suppress
automatic glue/kern breaks and visited-glue hyphenation inside a formula. Explicit
penalties and discretionaries remain eligible there. An after boundary immediately
before glue is itself a breakpoint; its width is excluded from the line ending
there. Math boundaries are discarded with leading glue, penalties, and explicit
kerns after a break, with the same widths subtracted during search. Packing and
detached output preserve surviving boundary widths without emitting glyphs.
The scoped dimension `\mathsurround` starts at zero. The
[inline math frontend](math-alignments.md) constructs these boundaries around
its converted formulas using the live formula bindings before group restoration.

Paragraph completion consumes the horizontal list, transforms its final glue
into the terminal penalty, and appends paragraph-fill glue. Selected breaks
transfer existing nodes into line boxes with left/right skips, indentation,
baseline/line glue, and interline penalties. During the same packing traversal,
insertion nodes migrate after their line, and adjustment nodes transfer their
children there before interline penalties, as specified in [pages.md](pages.md). Hanging indentation and `\parshape`
select line widths. The lower-level completion accepts the final widow penalty,
returns to the enclosing vertical mode, and returns the optional final line's
nonowning node ID. Extracted adjustments or insertions can follow this line in
the vertical root. The ID is valid only while that root retains the line and must
be consumed before page work can release it. This path neither resets paragraph
settings nor runs page work; an empty paragraph returns no line. Ordinary
`end_graf` supplies the live `\widowpenalty`, resets paragraph settings through
ordinary local assignment semantics, and runs synchronous external page work.
The resulting vertical list is the [page builder](pages.md) contribution root.

Active and delta slots are recycled within reusable engine scratch; passive
predecessors survive until line construction finishes. Scratch is empty outside
paragraph completion and is absent from checkpoints. Paragraph shapes use the
established typed COW tables with pooled pages and directories, retained by
document/save/checkpoint roots. Finished lines remain in the versioned node store with their vertical-list owner.

Horizontal and vertical packing accept `PackSpec::Natural`, `To(dimension)`, or
`Spread(amount)`. Natural packing uses the measured list size; `To` fixes the
box width or height; `Spread` adds its signed amount to the measured size.
Paragraph lines and selected pages explicitly use `To` with their saved goals.
The existing measurement traversal determines the target without an additional
list pass or retained measurement state. Horizontal packing can also return its
operation-local natural width, height, depth, and four-order stretch/shrink totals
alongside the owned box. Adjusted packing reports the same measurements while
extracting migrating material in that traversal. These totals are not stored in
box payloads; convenience calls discard them when only the box is needed.

Horizontal packing measures width, height, and depth. Vertical packing measures
height excluding its final depth and the maximum of zero and each child width
plus its horizontal shift. A final depth above the supplied maximum is moved into
the natural height before resolving `Natural` or `Spread`; the resulting depth remains capped for
all specifications. Empty lists begin with zero dimensions; vertical depth
limiting still applies. Dimension arithmetic and glue excess preserve TeX's
operation order. Packing uses an IEEE binary64 glue ratio, matching the native Web2C oracle.
Normal shrink is capped at one; infinite glue orders and signed components retain
their upstream behavior. These rules follow `hpack` and `vpackage` in the
[TeX source](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web).
Native comparisons inspect node dimensions and displayed glue settings; the
page adapter additionally checks cumulative absolute output placements. Diagnostic text, overfull
rules, general discretionary syntax, display math,
last-line fitting, and pdfTeX spacing extensions remain outside this baseline.
Unsupported infinite paragraph shrink fails explicitly rather than applying
TeX's interactive recovery.

The initial integer parameters match INITEX: tolerance 10000, hang-after 1, and
zero for the remaining paragraph penalties and controls. All dimensions and glue
start at zero. Sources set the geometry, font, and spacing they require.
`\penalty` appends to the current horizontal or vertical list. `\parshape`,
paragraph integer parameters, and existing typed skips/dimensions obey ordinary
local/global assignment and internal-quantity formatting.

Portable tests cover tie selection, final recovery, discretionary range/child
transfers with retained old node roots, local/global paragraph resets, search
failure cleanup, and paragraph checkpoints inside pending macros and open groups.
The ignored native adapter compares 51 packed vertical structures against
pdfTeX 1.40.25 (TeX Live 2023/Debian), using INITEX with e-TeX. These compare line
contents, dimensions, shifts, displayed glue ratios, retained discretionaries,
skips, and penalties. Native tracing confirms the emergency pass executes in its
fixture. Cases include attainable and unattainable looseness, equal-cost repeated
words, forced and overfull breaks, hanging/explicit shapes, kern boundaries, and
controlled metrics plus the three real fonts identified in [fonts-text.md](fonts-text.md).
The math-boundary adapter compares directly constructed ordinary lists with native
inline formulas, including signed surrounding widths, adjacent words/glue/kerns,
explicit breaks, suppressed hyphenation, and emergency passes. Its construction
hook is confined to tests and does not provide a production math frontend.

Run the native adapter with explicit executable/corpus paths and the executable's
library search path when needed, as for the font adapter:

```bash
cargo test --lib upstream_paragraph_lines_and_glue -- --ignored
```

A separate packing adapter compares 29 font-free natural, exact, and spread
boxes with native INITEX geometry and displayed glue settings. These include
empty lists, signed dimensions and glue, infinite orders, overfull shrink,
shifted child boxes, and final-depth limits. It needs only `UMBER_PDFTEX` and its
library search path:

```bash
cargo test --lib upstream_natural_exact_and_spread_box_geometry -- --ignored
```

No timing measurement is part of this validation. Page integration consumes
`TextState.vertical` directly. `hpack`, `vpackage`, and `append_to_vlist` are shared
internal helpers. The page builder owns top skip, maximum depth, default output,
and end-of-job page flushing; custom output routines remain unsupported.

## 1. Font metrics and live fonts

Parse TFM data once per immutable resource version. Retain compact character
metrics, ligature/kern programs, boundary-character information, extensible
recipes, and the baseline font parameters. Scale metrics using TeX's integer
algorithm for the requested size. Parsed baselines and scaled immutable data can
be shared; a document font descriptor selects them and document-owned mutable
parameter, character-tag, and code-table groups. Different font IDs share those
groups where pdfTeX's font-creation path aliases them, notably auto-expanded
variants; other paths copy or create the specified groups. Preserve the alias
relationships within a document without mutating process-cached baselines.

`\fontdimen`, `\hyphenchar`, `\skewchar`, and pdfTeX's mutable font settings
belong to document state. They are globally effective in TeX even within groups,
and checkpoints restore their earlier values. Font allocation and font-identifier
state are document-owned too. A cached space-glue value is invalidated when the
relevant font parameters change, following upstream's per-font invalidation
behavior even when parameter storage is aliased. Font loading preserves the rules about
reusing an already-loaded font and extending parameter storage.

The text path uses metric indices and scaled dimensions directly. It does not
call the host, inspect font-program bytes, or resolve resource names per character.
The actual font program is a backend resource, distinct from TFM layout metrics.
See TeX's `read_font_info`, `find_font_dimen`, and character processing in the
[TeX source](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web).

## 2. Character runs and spaces

Use a tight horizontal character kernel called from the execution loop. Its
locals hold the current font, character, boundary state, and small ligature stack.
Read through the shared token reader and accept direct character commands before
entering generic expansion dispatch. Expand an already-read fallback where needed,
then continue this kernel if the resulting command belongs here, including
`\char` scanning where TeX admits it. On the first different command,
return that already-read command to dispatch rather than re-reading its identity.

The kernel follows the TFM ligature/kern program, including boundary characters,
retained ligature components, and `\noboundary`. Ligature components matter for
hyphenation and diagnostics even though output uses the resulting glyph. New
character, ligature, and kern nodes are initialized directly in the live list.
Small working stacks retain capacity across calls or use marked scratch; ordinary
text does not allocate one heap object per glyph.

Space-factor updates follow the current `\sfcode` table and TeX's special rules.
A space command constructs glue from `\spaceskip`, `\xspaceskip`, font parameters,
and space factor at that operation. Glue is a compact immutable specification,
shared or copied as a value; later font changes do not retroactively alter it.
Language/minimum-hyphenation changes and other semantic nodes are inserted at the
points TeX specifies, rather than inferred from source text after construction.
When enabled, pdfTeX's interword-glue adjustment uses the preceding character's
font and `\knbscode`, `\stbscode`, and `\shbscode`, including its special
space-factor path. A font switch before the space preserves that dependency.

Assignments, font switches, and noncharacter commands return to execution in
order. Expansion can run while looking ahead, but the kernel does not execute an
unrelated assignment ahead of the current text. pdfTeX's ligature suppression and
other character-path settings are part of the same profile-specific kernel.
Prepend/append auto-kerns run in their specified order before TFM processing and
can end that processing. `\pdfnoligatures` changes access to both ligature and
kern programs through the document's mutable character tags.

## 3. Packing over existing lists

Horizontal and vertical packing walk existing node lists and compute their
natural dimensions, stretch and shrink totals, and required glue setting. Packing
adds the box header in destination node storage; it does not copy the list into
a temporary layout representation.

Preserve TeX's treatment of infinite glue orders, excessive shrink, badness,
rounding, shifts, rules with running dimensions, and over/underfull diagnostics.
TeX's stored glue-set representation and output rounding need an explicit
compatible arithmetic path; replacing them with arbitrary floating-point layout
would alter placements. The chosen Rust representation can differ from WEB's
memory words while producing the same dimensions and output movements.

Adjustments extracted during packing transfer their lists to the vertical
destination at the proper point. Unset alignment boxes have a separate final
packing step, developed in [math-alignments.md](math-alignments.md).

## 4. Paragraph preparation

The paragraph owns its horizontal list and the state accumulated while reading:
initial language and hyphenation minima, line-number context, and relevant
semantic nodes. At paragraph completion, use the then-current paragraph
parameters according to TeX's rules, including `\parshape`, hanging indentation,
line widths, skips, penalties, and `\looseness`.

Prepare the list in place. Remove or transform the final glue as TeX does and add
the terminal penalty and `\parfillskip` directly. Transfer the list out of its
horizontal semantic nest. The enclosing vertical nest supplies the continuing
line-number state, including continuation around displayed formulas.

The line breaker walks this list in document order. Use the compact node fields
directly, with local cached pointers or scoped chunk views where useful. A separate
materialized array of all paragraph contents would add a full copy and another
source of node identity; create only the working arrays the algorithm needs.

## 5. Exact line breaking with paragraph scratch

Preserve Knuth–Plass behavior as implemented by TeX: active break candidates,
fitness classes, demerits, line numbers, discretionary alternatives, and the
ordering used to select equal-cost candidates. Use stable indices into reusable
scratch arrays for active and passive records. Recycle active slots when removed;
passive predecessor records remain until the final path is selected.

Track width deltas and stretch/shrink components in the same integer domain as
TeX. Its delta-width organization avoids both repeated prefix measurement and
overflow from a whole-paragraph accumulated distance. Common break tests should
read a small hot record rather than chase individually allocated candidates.
The choice between packed records and separate hot/cold arrays can follow actual
cache and generated-code measurements.

e-TeX's enabled `\lastlinefit` path carries the preceding line's shortfall and
finite glue amount in each active candidate. It changes final-line badness and
later rewrites the terminal `\parfillskip` specification, using upstream's
enabling conditions and integer fraction arithmetic. Keep these fields in the
candidate representation where that path needs them.

The pretolerance pass, when enabled, tries without automatic hyphenation. The
tolerance pass introduces hyphenation, and an emergency-stretch pass is used when
the preceding search requires it. Final-pass recovery and `\looseness` selection
follow upstream, including artificial feasible candidates where TeX uses them.
Reinitialize scratch for a new pass, retaining its allocated capacity.

This algorithm is TeX's `line_break`/`try_break`/`post_line_break`, not an approximate
optimizer with a different cost function.
See the [TeX line-breaking source](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web).

## 6. Hyphenation and discretionary material

### Executable loading and lookup

`\language`, `\lefthyphenmin`, and `\righthyphenmin` are signed integer
parameters, initially zero, with ordinary assignment, arithmetic, grouping, and
checkpoint behavior. Table loading normalizes language values outside 1 through
255 to zero. `\patterns` and `\hyphenation` publish globally within the document,
independently of grouping and `\globaldefs`; successful completion releases
`\afterassignment`. Both scanners expand tokens in the normal scanner context.
They accept a mandatory opening brace after spaces or relax commands, then
publish each completed entry at a space or closing brace. An invalid later entry
does not undo earlier entries. Patterns accept letter and other-character
meanings; exceptions also accept character definitions and `\char`.

Patterns lowercase through the live `\lccode` table, use a dot for the word
boundary, and alternate digit weights with letters as in TeX. Words and patterns
retain at most 63 letters while continuing to validate subsequent input. Sparse
nonzero weights occupy contiguous ranges in a pooled operation table. An indexed
sorted-child trie includes the language edge; it needs neither packed-trie
preprocessing nor operation interning. A terminal with no nonzero weights remains
unset for duplicate detection: zero-weight patterns can repeat or acquire weights,
but redefining an already weighted terminal fails, including with zero weights.

Exception entries retain bounded lowercase keys, language, and a hyphen-position
bitmask. Replacing an existing exception updates its mask. A zero mask still
overrides patterns. A growing bucket directory keeps the indexed dictionary's
load bounded. Lookup accepts an already lowercase word, checks exceptions first,
then takes maximum overlapping pattern weights and applies odd-weight and
normalized left/right minimum tests. Fixed stack arrays hold scanner and lookup
working data; no word needs a separate allocation.

Two lazily created shared roots own pattern and exception COW tables. A checkpoint
retains those roots and a document freeze flag in constant time with respect to
the table inventory. The first mutation after a retained snapshot copies table
directories and only modified pages; later exception updates do not copy pattern
directories. Freezing rejects subsequent pattern loading but permits exceptions;
restoring an earlier checkpoint restores both data and readiness. The first
tolerance/hyphenating pass freezes the tables, even when no patterns were loaded;
a successful pretolerance pass does not. e-TeX's saved hyphenation-code maps
remain unsupported; the renderer uses live lowercase codes.

These rules follow `new_patterns`, `new_hyph_exceptions`, and `hyphenate` in
[TeX](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web),
including Web2C's replacement of repeated exceptions in
[tex.ch](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.ch).

### Paragraph integration

Paragraph entry captures normalized language and left/right minima before
`\everypar`. A positive-horizontal character-run entry compares the current
language parameter with the list's current language and, when different, appends
a zero-width language node carrying the normalized language and current minima.
This occurs even if the character's glyph is absent. Assigning minima alone does
not change the captured state. Explicit `\setlanguage` always appends such a node
in either horizontal mode, including for an unchanged language, without assigning
the language parameter. Nodes survive copying, packing, line-leading pruning and
output traversal; both the main search and its word lookahead update language
state when crossing them.

Each hyphenating pass begins with the paragraph's initial language/minima. It
inspects a candidate only after visiting glue, using live lowercase codes,
uppercase-word permission and the font's hyphen character. Initial punctuation,
normal kerns, font changes, complete ligature components, terminating node types
and the 63-letter candidate prefix follow TeX's pre-hyphenation traversal.
Kerns have normal, explicit and accent kinds. Only normal kerns are skipped
during word collection; either other kind terminates a candidate. Only explicit
kerns are discarded at a line start or become before-glue kern breakpoints.
Accent kerns survive pruning and, like normal kerns, permit a following glue
breakpoint.
Existing discretionaries and math/box boundaries constrain eligibility. An
emergency pass reuses inserted nodes, but can inspect later words that an earlier
pass did not reach. A future e-TeX saved-code map belongs with the pattern root;
when present, it replaces live `\lccode` lookup for that language, with absent
characters mapping to zero.

Fixed word arrays retain original and lowercase codes and a position mask;
reusable stack capacity holds pending ligature operations. Hyphenation reconstructs
cut prefixes with TeX's `reconstitute` algorithm directly in the paragraph's node
store, preserving original components and boundary flags. It synchronizes the
no-break and post-break translations before fixing each discretionary's replacement
count, and reconstructs the pre-break translation with the font boundary and an
existing hyphen glyph, if available. A replacement exceeding 127 nodes retains
the no-break translation and discards that discretionary's alternatives.

Temporary output lists have disjoint owners. The original range remains in the
paragraph until complete reconstruction succeeds; failure releases new branches.
An infallible list splice then installs the result and releases the original
range. Font resources and the existing table roots are borrowed, without per-word
resource ownership or character buffers. Search engine borrows are confined to
individual break trials, allowing mutation at the native visited-glue point.

A discretionary owns detached pre-break and post-break lists. Its no-break
replacement is the next `replace_count` ordinary nodes in the enclosing list,
so construction advances that list's tail through the replacement range.
Line-breaking trials measure the correct alternative; a selected break splices
its material as TeX does. Unchosen discretionaries keep their pre/post lists
inside finished lines where diagnostics and later unboxing can observe them.
Discard only the material upstream discards. Later passes reuse paragraph structures
as upstream does, without multiplying complete word or paragraph copies.

Portable reconstruction tests check every allocation-failure boundary against an
unchanged retained source version, scratch reuse, checkpoint replay, and the
127-node replacement boundary. Native tests compare complete pre/post branches,
component/boundary flags and replacement counts using controlled ligature programs
and real cmr10 with the English pattern source. They cover language/minima timing,
live lowercase codes, uppercase permission, missing hyphen glyphs, font changes,
63-letter prefixes, exceptional ligature growth and selected line breaks. The
page adapter also compares detached glyph placements with native DVI records.
Use the same explicit executable/corpus environment as the other native adapters:

```bash
cargo test --lib automatic_hyphenation -- --ignored
```

## 7. pdfTeX spacing extensions

Character protrusion and font expansion have distinct control levels. With
`\pdfprotrudechars=1` or `\pdfadjustspacing=1`, line construction/packing applies
the extension while retaining baseline break choices. Values above 1 also change
breakpoint feasibility. Implement those predicates in the corresponding paragraph
and packing calculations, preserving pdfTeX's thresholds, integer arithmetic,
and font-selection behavior.

Document font state holds the relevant character codes, expansion settings, and
derived variants. Immutable metrics can still be reused. Add the enabled-feature
branches around the same baseline loop; separate specialized kernels are a later
measurement choice if dormant extension checks become costly.
The compatibility reference is pdfTeX's `try_break`, protrusion, expansion, and
packing code in the
[pdfTeX source](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/pdftexdir/pdftex.web).

## 8. Construct lines in place

After choosing the predecessor chain, split and splice the existing paragraph
nodes into line ranges, construct line box headers, and add left/right skips and
interline material. Transfer adjustment and insertion material as TeX requires.
Reuse nodes where their semantics permit it; no flattened sequence of new glyphs
is necessary. These nodes all remain in the document node store; paragraph-to-
vertical handoff changes list ownership without moving storage or changing IDs.
The resulting lines outlive the paragraph scratch arena.

Once the lines and diagnostics are produced, release all active/passive records,
word work buffers, and pass scratch. Return to execution and page building through
ordinary calls. At an eligible outer-driver handoff, the resulting nodes can
remain on the unfinished page, and ordinary group save records can remain live.
Long line-breaking/hyphenation loops support coarse interruption checks that unwind
to the last checkpoint and release their scratch.

## 9. What to measure and compare

Compare line breaks, box dimensions, glue settings, discretionary material, and
diagnostics against upstream, including equal-demerit choices and overfull cases.
Use text with boundary ligatures, changing space factor, multiple languages,
explicit discretionaries, hanging shapes, displays, and enabled pdfTeX spacing.

Measure character-kernel dispatch, node bytes per character, line-break candidate
counts, hyphenation work, and allocations/copies per paragraph. Then compare full
documents; a faster break-candidate microbenchmark is useful only if the actual
text and packing paths remain faithful and improve with it.
