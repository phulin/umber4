# Fonts, horizontal lists, and paragraphs

This proposal develops the main text-layout path. Execution builds TeX nodes in
their destination store; paragraph algorithms use reusable working memory and
return finished lines to the enclosing vertical list. The storage interface is
described in [storage.md](storage.md).

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

Format-owned hyphenation patterns can be immutable packed tries indexed by
language. Runtime exceptions are globally effective document state, with
snapshot sharing where needed. Language switches and left/right minima in the
paragraph list control which words are eligible. Lowercase mapping uses a saved
per-language map when e-TeX's `\savinghyphcodes` recorded one with the patterns;
absent characters then map to zero. Otherwise it uses the live `\lccode` table.
Hyphenation and exception scanning preserve that selection rule. Saved maps
belong with immutable patterns; eligibility follows the corresponding operations.

Reuse small word buffers for character codes, lowercase codes, hyphen positions,
and ligature reconstruction work. Hyphenation reconstructs character/ligature
ranges with TeX's `reconstitute` algorithm, producing discretionary nodes directly
in the paragraph's node store. It cannot simply insert a penalty between final
glyphs: a ligature may have different forms on either side of a break.

A discretionary owns detached pre-break and post-break lists. Its no-break
replacement is the next `replace_count` ordinary nodes in the enclosing list,
so construction advances that list's tail through the replacement range.
Line-breaking trials measure the correct alternative; a selected break splices
its material as TeX does. Unchosen discretionaries keep their pre/post lists
inside finished lines where diagnostics and later unboxing can observe them.
Discard only the material upstream discards. Later passes reuse paragraph structures
as upstream does, without multiplying complete word or paragraph copies.

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
ordinary calls. At the outer no-open-groups paragraph handoff, no line-breaker
continuation survives; the resulting nodes can remain on the unfinished page.

## 9. What to measure and compare

Compare line breaks, box dimensions, glue settings, discretionary material, and
diagnostics against upstream, including equal-demerit choices and overfull cases.
Use text with boundary ligatures, changing space factor, multiple languages,
explicit discretionaries, hanging shapes, displays, and enabled pdfTeX spacing.

Measure character-kernel dispatch, node bytes per character, line-break candidate
counts, hyphenation work, and allocations/copies per paragraph. Then compare full
documents; a faster break-candidate microbenchmark is useful only if the actual
text and packing paths remain faithful and improve with it.
