# Vertical lists, page breaking, and output routines

This proposal develops page construction over the nodes produced by execution,
paragraphs, math, and alignments. Page building is incremental in TeX's ordinary
document order; its unfinished lists are persistent document state. Algorithm
scratch remains local to calls.

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

Before entry, `fire_up` checks the dead-cycle limit. A reached limit follows the
upstream recovery/default-output path without first executing the routine.
Entering a routine increments dead cycles; shipout resets them.

The routine can inspect, split, take, copy, or repack box 255 and other boxes;
make assignments; create more material; and explicitly `\shipout` a box. These
operations use the same ownership and primitive paths as ordinary execution.
The page builder cannot ship its chosen page in advance of that program.

Closing the routine restores its group and checks box 255 consumption. Splice
held-over insertions, then routine-produced material, then existing contributions
in that order. Page building continues according to those transitions. If no output
routine is supplied, the engine's default output path handles the prepared box.
An empty routine and a routine that fails to consume its box must retain their
specified recovery behavior.

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
