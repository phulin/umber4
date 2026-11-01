# Layout storage and numeric foundation

This contract defines the foundation used by executable fonts, paragraphs, and
[page construction](pages.md). The numeric and node facilities
below are independent of the layout algorithms; their correctness
references are `round_decimals`, `print_scaled`, `scan_dimen`, `scan_glue`,
`badness`, register arithmetic, and list ownership in
[TeX's WEB source](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web).

## Numbers and bindings

`Scaled` stores signed integer scaled points; one point is 65,536 units and
dimension scanning and multiplication enforce an absolute limit of
1,073,741,823. The type can also retain larger word values produced by register
addition; direct dimension-to-glue conversion preserves those values. Calculations
use wider integer intermediates with TeX's truncation and rounding order. Decimal scanning
retains at most seventeen fractional digits while consuming all digits. It
accepts the decimal comma, signs, integer radices, internal quantities, physical
units, and internal dimensions used as units. Scaled-point units discard the
fraction as upstream does. Invalid syntax and overflow fail at the operation;
interactive recovery is outside the existing interpreter contract.

Glue is an inline value: width, stretch, shrink, and independent normal/fil/fill/
filll orders. Only the zero components upstream normalizes change order; a
component that cancels to zero can retain an infinite order. A wholly zero glue
value is canonical. Infinite orders are compared before combining amounts. Addition, multiplication, and division preserve upstream's
component overflow limits and truncation, including signed glue components.
Math glue and dimension/glue expression primitives remain unsupported.

Dimension and glue registers and parameters use distinct typed COW tables with
assignment levels. Saved values live in typed group-save records. Local, global,
identical-assignment, alias, and after-assignment behavior uses the existing
assignment rules; checkpoint capture/restore includes both typed roots. Integer
table regions derive from explicit counts rather than hardcoded offsets.
`\mag` and its first-use frozen value belong to restored document state.
Font-relative units read the current font's quad and x-height through a narrow
getter. The initial null font supplies zero for both, as in INITEX.

`\the`, `\number` coercions, ordinary expansion, expanded-list scanning, and
`\meaning` share the same typed quantity and formatting rules. Formatting uses
TeX's shortest round-trippable scaled decimal algorithm and normal token
categories. The command inventory explicitly classifies expandable commands;
numeric command codes no longer reserve every code above 63 for expansion.

The supported numeric primitives are `\dimen`, `\dimendef`, `\skip`,
`\skipdef`, `\ifdim`, `\mag`, and their existing assignment/arithmetic and
conversion paths. Register indices are 0–255. New dimension parameters are
`\hsize`, `\vsize`, `\maxdepth`, `\parindent`, `\hangindent`,
`\emergencystretch`, `\lineskiplimit`, `\hfuzz`, `\vfuzz`, `\overfullrule`,
`\splitmaxdepth`, `\boxmaxdepth`, `\delimitershortfall`, `\nulldelimiterspace`,
and `\scriptspace`.
Glue parameters are `\baselineskip`, `\lineskip`, `\parskip`, `\topskip`,
`\leftskip`, `\rightskip`, `\parfillskip`, `\spaceskip`, `\xspaceskip`,
`\abovedisplayskip`, `\abovedisplayshortskip`, `\belowdisplayskip`,
`\belowdisplayshortskip`, and `\splittopskip`.
Their initial values are zero, as in INITEX; `\mag` starts at 1000. Loading
macro-package conventions uses ordinary assignments rather than a preset format.

Integer initialization state also includes `\hbadness`, `\vbadness`,
`\binoppenalty`, `\relpenalty`, `\displaywidowpenalty`, `\predisplaypenalty`,
`\tracinglostchars`, `\uchyph`, `\newlinechar`, `\delimiterfactor`,
`\showboxbreadth`, `\showboxdepth`, and `\errorcontextlines`. These start at
zero and accept signed integers without per-parameter clamping. Their slots use
the existing scalar array, save levels, and checkpoint copy; dimension and glue
parameters share their existing typed COW roots. Reads, aliases, arithmetic,
`\globaldefs`, and after-assignment tokens use the ordinary quantity/assignment
paths. Invalid syntax or numeric overflow leaves the preceding binding intact.

This state contract does not implement math/display layout, hyphenation,
insertion splitting, packing warnings, lost-character tracing, or box/context
diagnostics. Those consumers remain unsupported; accepting and retaining their
parameter assignments does not claim those rendering or diagnostic effects.
`\newlinechar` does affect emitted messages and immediate diagnostic writes as
defined in [resources.md](resources.md).

## Mutable nodes

`NodeId` is a store-version-relative numeric cell address with null zero. A
document node-store root owns a small directory of shared 256-cell chunks and
allocation state. A write to a shared chunk first obtains a unique reusable
chunk and copies its initialized records; links retain their numeric addresses.
Allocation prefers free cells in unique chunks and starts a fresh chunk rather
than copying a shared tail merely to append. Free-slot state belongs to the same
version as the nodes. Released empty chunks enter a bounded session reuse pool.

Nodes contain links and compact tagged values, with inline glue and font IDs,
and no per-node heap allocation or reference count. Box, ligature-component,
and discretionary child lists are owned edges. A non-`Copy` `List` carries head
and tail for building and ownership transfers. Appending/splicing consumes an
existing list; constructing a box consumes its contents. Allocation and removal
return a non-`Copy` detached-node owner with a null next link. Appending consumes
that singleton as a one-node list through the same splice operation as longer
chains. Link mutation stays inside the node store; other subsystems can mutate
only node payloads. Detached owners and lists require their matching store version;
their types do not encode store identity. Debug append checks retain null-link
diagnostics, while reads and tail mutations validate live nodes and preserve COW.
Capacity failure releases any transferred children. Node, box, and
ligature/discretionary payloads carry owned non-`Copy` child lists; only the
private COW copier duplicates them into another version. Raw node IDs are
internal references, never independent owners. Copying a root for a checkpoint
is distinct from copying a mutable TeX list. Semantic list copying allocates
fresh nodes and box payload slots, while immutable font metrics remain shared.

Box headers live in a dense reusable `Table` with 64-entry COW pages inside the
node store. A box node owns an opaque non-`Copy`, non-`Clone` slot handle; only
the private node snapshot copier duplicates it into another store version.
Box-table roots and their free-slot head clone and restore with the node roots.
The private slot copier snapshots child-list handles only when a shared table
page becomes writable. Box reads borrow the header directly, and header mutation
makes only the payload page writable. There is no per-box allocation or reference
count. The table retains its peak slot range and reuses released slots.

Box construction checks the existing node address budget before allocating a
payload slot. Rejection releases the transferred contents without growing the
payload table. Releasing a box first takes its header from the writable table
and links the slot into that version's free list, then traverses its owned
children. Retained versions preserve their own header and child-node roots.
Dropping a whole node-store version discards both directories together; dropping
numeric child handles does not recursively traverse nodes.

Nested release walks owned child lists using reusable session scratch, avoiding
recursion and allocations per node. Scratch must be empty on return and is not
checkpoint state. Chunk references retain their backing independently of engine
lifetime. All roots containing IDs must be captured with the matching node-store
version. A node ID from an abandoned branch cannot be used with another version.

`NodeStore::copy_list` accepts a source-head reference in its matching store
version and returns an independently owned `List`. Null copies to an empty list.
Every sibling and owned box, ligature-component, and discretionary pre/post list
is copied; discretionary replacement nodes remain part of the sibling chain.
All scalar payload fields, including dimensions, shifts, glue settings, subtype
and boundary flags, are preserved without repacking. Semantic copying never
uses `List::snapshot` or duplicates an owning box-slot handle. The order follows
TeX's `copy_node_list`: allocate the parent, copy its children (pre before post),
then continue to its next sibling.

Copying uses a caller-retained `CopyScratch` vector of ownership frames and the
existing iterative release scratch. Like `ReleaseScratch`, the vector retains
its capacity between operations; a separate one-buffer metadata pool would add
an owner without improving this lifetime. The work stack handles arbitrary valid
node nesting without Rust recursion or a separate nesting limit. It is empty on
both success and failure and is absent from checkpoints.

Each frame owns a partial destination list. New parents initially own empty
child lists and enter that destination before child copying begins. Completing
a child frame transfers its list once into the new parent; remaining frame
outputs form disjoint owned trees. Capacity failure releases all of those trees
through ordinary node release, leaving the source and retained versions intact.
Allocator backing, free-slot metadata, and scratch capacity may grow during a
failed copy. The existing node-address capacity and system allocation behavior
apply; copying adds no smaller limit or pre-count pass. Source scalar fields and
reference IDs are read before allocation, so no node or payload reference spans
mutable allocation or COW. These transfers follow `copy_node_list` in the
[TeX source](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web).

The initial node kinds cover characters, ligatures and their boundary flags,
explicit/TFM kerns, glue, penalties, rules, horizontal/vertical boxes, and
discretionaries. Text execution constructs these nodes, and paragraph and page
packing supply compatible glue calculations. Box headers carry dimensions,
shift, and glue sign/order/ratio. Font metrics resolve through immutable font IDs
rather than per-character owners. Paragraph/page algorithms traverse and splice these
lists directly, and publish detached immutable results without live node IDs.

## Driver and validation

The driver quiescent flag distinguishes completed outer commands from errors
that require restore. Entering `advance` requires quiescence. Returning a budget
handoff and creating a checkpoint also require external vertical mode; horizontal
construction defers the handoff until an eligible boundary. Ordinary assignment
groups and unfinished pages may remain open, as defined in
[checkpoints.md](checkpoints.md).

Validation covers independent arithmetic/format examples, exact operand cursor
boundaries, same-source INITEX register fixtures, grouped typed assignments,
restore/replay, and node mutation/free/reuse with older roots retained. Nested
release, chunk reuse, checkpoint lifetime, and branch independence receive focused
ownership tests and Miri coverage. This stage introduces no performance claims.
