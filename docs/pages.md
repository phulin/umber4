# Vertical lists, page breaking, and output routines

This proposal develops page construction over the nodes produced by execution,
paragraphs, math, and alignments. Page building is incremental in TeX's ordinary
document order; its unfinished lists are persistent document state. Algorithm
scratch remains local to calls.

## Executable page baseline

The executable slice implements TeX82's ordinary incremental `build_page`,
`fire_up`, and default output path over `TextState.vertical` contributions.
A separate current-page list owns considered nodes; page dimensions, frozen
maximum depth, last-node caches, and the best break and saved goal are document
roots. Checkpoints retain these roots together with their node-store version.
An unfinished page or trailing contribution kern is eligible state, including
inside ordinary assignment groups and partially consumed macro calls.

The first insertion, box, or rule freezes `\vsize` and `\maxdepth`; the first
box or rule inserts adjusted `\topskip` without changing its stretch or shrink. Leading glue, penalties, and
kerns are discarded. Legal glue, kern-before-glue, and penalty breaks are tested
before the candidate enters totals. Equal costs select the latest break. An
awful cost or forced penalty fires the best break, returning its node and later
material ahead of the existing contributions. The selected penalty becomes
10000; `\outputpenalty` receives its former value globally. Selected material
is packed to the saved goal with the frozen maximum depth, then installed in
box 255 without changing its assignment level or creating a group save. A nonvoid
box 255 at installation fails with `Unsupported`. Infinite page shrink requires
unsupported interactive recovery and returns `Unsupported`, as in the paragraph
baseline.

Page work runs at TeX's paragraph-entry, paragraph-completion, vertical `\par`,
and vertical penalty points. Vertical glue, kerns, and rules alone do not drain
contributions. `\prevdepth` remains the enclosing vertical-list baseline state
across page output. `\end` inserts the current `\par` token when horizontal,
then retries after appending an empty horizontal box of `\hsize`, fill-order
glue, and penalty -1073741824
directly to contributions. This preserves TeX's final flushing and empty-job
behavior; it is not natural-height shipping.

The vertical command slice includes `\vskip`, `\vfil`, `\vfill`, `\vss`,
`\vfilneg`, `\hrule`, horizontal `\vrule`, `\prevdepth`,
and writable page dimensions. Rules preserve running dimensions until traversal.
Marks, leaders, math, and output backends remain unsupported.
Explicit horizontal/vertical/void box shipout uses the same detached lowerer.
Packing diagnostics and interactive recovery remain outside this fail-fast
interpreter contract.

`\output` is a scoped token parameter and participates in token-list reads,
copies, aliases, and checkpoints. A nonempty directly scanned body receives the
outer brace pair TeX stores; a token-list copy preserves its existing tokens.
Empty values select default output. A nonempty routine executes through the
ordinary execution loop in an output group and internal vertical mode. Its retained input
owner survives reassignment of `\output`; copied token lists remain verbatim,
so a missing opening brace fails with `Syntax`. The document output frame owns
the enclosing text state (which may already be horizontal) and group level.
Page building stops while that frame exists. Page-dimension reads inside the
routine expose the retained raw totals, including assignments made there, even
though the next page has not started. Outside output, an unstarted page still
reads as the default maximum goal and zero other dimensions.

Before entry, `\deadcycles` is compared with scoped `\maxdeadcycles` (INITEX 25).
A reached limit fails with `Unsupported`, before executing the routine or
publishing a fallback page. Entry increments the global page counter. Numeric
reads and assignments of `\deadcycles` access that direct state; assignments
are inherently global. Real horizontal/vertical shipout resets it to zero,
including huge-box rejection; void shipout changes nothing. Job completion
requires empty page/contributions and zero dead cycles. `\end` in internal
mode or an active output group fails with `Group`; horizontal mode first runs
the current `\par` token and retries in vertical mode.

Closing an output group accepts only a logically exhausted current source with
role output text or backed-up input, following `fire_up` and output-group closing
in TeX82. Source storage shape does not determine its role. Logical exhaustion
includes all spans/windows. Only that accepted current source is popped: a
backed-up closing brace can leave old routine input to run in the enclosing mode.
An unbalanced source fails with `Syntax`; mismatched groups fail with `Group`;
ordinary premature source EOF remains `Eof`. The paragraph ends before group
assignments restore. Box 255 must then be void (`Unsupported` otherwise).
Routine-produced material moves before old contributions, the enclosing text
owner is restored, and page building resumes, possibly entering another routine.
Interactive recovery is outside this fail-fast contract.

Detached results expose an immutable page prefix, packed box records, absolute
glyph/rule placements in scaled points, count registers, and output penalties.
Coordinates include the current signed `\hoffset` and `\voffset`, before any
backend's implicit one-inch origin; positive y points down. The root box baseline
is `(hoffset, height + voffset)`, and lowering translates all descendants and
placements from that point. These typed dimension parameters start at zero and
follow ordinary scoped assignments and checkpoints. Huge-page rejection checks
width plus horizontal offset and height plus depth plus vertical offset, while
still checking height and depth individually. Negative offsets are valid.
Page records retain no live node IDs or
document font IDs. Cumulative glue rounding follows `hlist_out` and `vlist_out`.
Typed ordered run/rule and glyph-byte buffers are built in destination storage.
The forward placement iterator yields owned absolute glyph/rule values and reads
retained font widths within runs; it does not return a placement slice. Run
boundaries preserve all noncharacter nodes and box entry/exit, and glyph errors
remain eager during lowering. Reusable traversal/release scratch contains no
durable semantic state.
Immutable output chunks share prefixes across checkpoint, result, and branch
owners. Branching starts a fresh chunk linked to the retained endpoint without
copying page history. Held partial chunks may pin at most fifteen later invisible
pages. Lowering completes before releasing the corresponding node tree; only
successful records enter the published prefix.

TFM source headers and their exact immutable buffers share one sealed resource-set
owner. Registration is already closed before execution. Runtime scaled-font
headers occupy fixed chunks of once-initialized slots; opaque handles retain a
chunk and slot, with safe borrowed descriptor access. A page retains one handle
per used font and runs store local indices, with no per-glyph ownership or metric
copies. Both live layout and detached handles read dense immutable scaled metric
records through the same accessor; the header's separate presence bitmap retains
TFM character-existence semantics. Retained pages keep the entire registered source-resource set and the
scaled-header chunks they use alive, even after engine/checkpoint teardown.
Document font selectors and mutable parameters remain independently restored.

Acceptance compares upstream page breaks, box dimensions/glue settings, and DVI
absolute placements, including forced and final breaks, equal costs, discarded
top material, maximum depth, overfull material, and cumulative fractional glue.
Ownership checks retain old results while restoring incomplete pages and pending
macros/groups, grow font caches after publication, reuse document font IDs on a
new branch, and exercise both teardown orders. This stage makes no performance
claim and adds no backend.

## Executable insertions and adjustments

`\insert` captures an owned internal vertical list. Construction resets paragraph
parameters, starts with ignored previous depth, and runs no `\everyvbox` hook.
Closing ends its paragraph, captures `\splittopskip`, `\splitmaxdepth`, and
`\floatingpenalty` before restoring assignments, then measures with unconstrained maximum depth. The enclosing mode, space factor, and previous depth survive.
`\insert255` requires unsupported recovery. `\vadjust` captures the same child
list in horizontal mode; vertical use fails with `Syntax`.

Insertion nodes use affine handles into dense reusable COW payload slots;
adjustments own their compact list directly. Ordinary nodes remain 24 bytes.
Snapshots pair these owners with matching store roots; semantic copies recursively
copy children through the iterative copy machinery. Paragraph line packing and
new hboxes directly appended in vertical mode extract insertions and adjustments.
Adjustment children follow the line before its interline penalty, without changing
previous depth. Stored and explicitly shipped hboxes retain these zero-size nodes;
backend traversal ignores their children.

The page distinguishes empty, insertions-only, and ordinary box contents. The
first insertion freezes page specifications; top discardable material is still
discarded until a box or rule arrives. Pooled sorted class records retain current
and best accepted insertion pointers, accumulated size, and class skip corrections.
Page costs and arithmetic follow TeX, including divide-before-multiply scaling.
At the selected break, accepted children transfer into naturally repacked class
boxes without changing those registers' assignment levels. `\holdinginserts`
retains insertion nodes in box 255 when positive; `\insertpenalties` is direct
writable page state, reset by page output and output-group completion.

This executable slice supports unsplit insertions. The exact page-fit or class-limit
condition requiring TeX's internal `vert_break` returns `Unsupported`; it neither
approximates a split nor drops the insertion. Thus it does not yet produce genuine
held-over insertion lists. Public `\vsplit`, split pruning, marks, and discard
extensions remain proposed below.

## 1. Vertical contribution and current page

Keep separate contribution and current-page lists with direct head/tail handles.
Vertical commands and completed paragraphs append material to contributions.
`build_page` consumes it at the points TeX specifies, updating the unfinished
page's dimensions and best break. It can stop with contributions still pending
or with an output routine installed.

The page state contains its content phase, totals, depth, goal, frozen maximum
depth, stretch/shrink components, best cost/break and its saved goal (`best_size`),
insertion-class records, marks, and output/dead-cycle state. Last-glue, penalty,
kern, and node-type caches retain their values/owners independently of list tails;
discarded or replaced nodes can leave those TeX-visible caches meaningful.
These values live with the page, not in an active paragraph function. Starting a
page freezes the appropriate parameters at TeX's specified first-material point;
later assignments do not silently reconstruct the page from the new parameters.
Explicit writable page quantities still perform their defined mutations.

Discardable top material, `\topskip`, maximum depth, forced penalties, and break
cost calculations follow upstream. Use integer width/dimension arithmetic and
preserve the tie behavior that selects a break. TeX is not doing paragraph-style
global optimization over every future page.

The reference is TeX's `build_page` and `fire_up` in the
[TeX source](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web).

## 2. Lists transfer rather than copy

Moving contributions onto a page changes list links and ownership. At the chosen
break, return excess material to contributions; held-over insertions remain on
the current-page list while an output routine runs. The output box header is
constructed directly over the selected
nodes; ordinary page breaking does not clone every line.

Mutable links go through the live node-store root. Unique chunks mutate directly;
snapshot-visible chunks become writable as needed. Page scalars and list roots
are small copied state. A checkpoint can retain an unfinished page and later
restore it without requiring a separate serialized page-builder machine.

When a page is shipped, output traversal produces detached immutable records and
releases the shipped node ownership. Nodes in saved boxes, held-over insertions,
or contributions keep their independent owner. [output.md](output.md) describes
the lowering and serialization boundary.

## 3. Insertions and vertical splitting

Insertion nodes transfer owned vertical material, its natural dimensions,
penalties, and split parameters to page construction. A compact per-class table
tracks insertion capacity, accumulated boxes, and candidate split points. Allocate
class records when used; ordinary text should not traverse every possible class
on each node.

Apply TeX's insertion scaling, skips, class limits, split-top glue, maximum depth,
and penalty accounting. A chosen page break commits the corresponding insertion
state; later candidate calculations must not destroy material needed by the best
earlier candidate. Holdovers return as owned lists, with no copying of their
contents. `\holdinginserts` changes the specified page/output behavior, not the
insertion node's allocation lifetime.

`\vsplit` uses an ordinary vertical-break function with local search state. It
mutates the selected box/list according to TeX's ownership rules and constructs
the split result directly. Split marks become document state. Pruned top material
is normally released; positive `\savingvdiscards` transfers the specified nodes
to owned page-discard or split-discard lists with separate roots. `\pagediscards`
and `\splitdiscards` consume those lists by transfer. Preserve upstream's reset
points, including split-discard reset at `\vsplit`; insertion-remainder pruning
does not capture split discards. Scratch for candidate search can be reused once
that operation returns.

See TeX's `vert_break`, `vsplit`, and insertion handling in the
[TeX source](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web).

## 4. Marks across pages

A mark node retains an immutable token sequence. Top, first, bottom, split-first,
and split-bottom marks own the corresponding sequence directly, independently of
the page nodes that introduced it. A page's first mark and inherited top mark
are distinct even when they happen to name equal tokens.

e-TeX mark classes use a sparse document table for active classes, with small
class records and snapshot sharing as useful. Scanning a mark writes or retains
its tokens in durable storage. Reading a mark installs that sequence in normal
token input; it does not copy its text on each expansion.

Marks and insertion state are retained across paragraph checkpoints because they
are page semantics. Their ownership is not bounded by the paragraph that created
them. The extended behavior follows the
[e-TeX change file](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/etexdir/etex.ch).

## 5. Output routines use the execution loop

When TeX fires up a page, it prepares box 255, mark state, insertion registers,
and `\outputpenalty`, then installs `\output` token input in an output group and
internal vertical mode. The main loop executes that routine as TeX code.
No recursive copy of the whole interpreter or host callback is needed.

Before entry, `fire_up` checks the dead-cycle limit. A reached limit fails before routine execution; the native interactive
recovery/default-output continuation is not implemented.
Entering a routine increments dead cycles; nonvoid shipout resets them.

The routine can inspect, split, take, copy, or repack box 255 and other boxes;
make assignments; create more material; and explicitly `\shipout` a box. These
operations use the same ownership and primitive paths as ordinary execution.
The page builder cannot ship its chosen page in advance of that program.

Closing the routine restores its group and checks box 255 consumption. Splice
held-over insertions, then routine-produced material, then existing contributions
in that order. Page building continues according to those transitions. If no output
routine is supplied, the engine's default output path handles the prepared box.
An empty token parameter selects default output; a nonempty routine that fails
to consume its box fails with `Unsupported`.

Shipout itself is an ordinary function that consumes or rejects the supplied box
and performs deferred effects in traversal order. Backend lowering occurs there,
not during line breaking. [execution.md](execution.md) covers box construction
and assignment staging; [resources.md](resources.md) covers immediate/deferred I/O.

## 6. Checkpoint handoff

After paragraph line construction and synchronous page work return, the driver
can checkpoint if it is in external vertical mode with no unfinished construction
or output routine. Ordinary assignment groups can remain open. Retain page scalars,
contribution/current-page roots, marks, insertion state, last-node caches, discard
owners, and the matching node-store version. The selected break's saved goal
remains distinct from the current goal after later insertions.

An active output routine prevents eligibility. Closing it can provide an eligible
handoff immediately, after its finishing/page-building work returns; another
paragraph is not required. Checkpoints may retain unfinished pages, and shipout
alone does not establish eligibility. [checkpoints.md](checkpoints.md) owns the
complete predicate and cadence.

## 7. Page-specific checks

Compare page break positions, dimensions, marks, insertion boxes, output penalties,
dead cycles, and the TeX-visible state after output. Use forced breaks, leading
discardable material, insertion splits, held-over insertions, mark classes, and
routines that repack or delay a page. Include a checkpoint before the break that
later exercises those retained nodes and compare resumed versus fresh execution.

Measure contribution traversal, inserted node count, writable-chunk copies, and
memory held by unfinished pages. Saved boxes and output routines should be part
of real document workloads, since a straight text stream understates both lifetime
and mutation costs.
