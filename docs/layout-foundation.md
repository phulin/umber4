# Layout storage and numeric foundation

This contract defines the foundation used by executable fonts and paragraphs;
page construction remains a proposal. The numeric and node facilities
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
Font-relative units call a narrow current-font unit getter; before fonts are
implemented the null font supplies zero quad and x-height, as in INITEX.

`\the`, `\number` coercions, ordinary expansion, expanded-list scanning, and
`\meaning` share the same typed quantity and formatting rules. Formatting uses
TeX's shortest round-trippable scaled decimal algorithm and normal token
categories. The command inventory explicitly classifies expandable commands;
numeric command codes no longer reserve every code above 63 for expansion.

The supported numeric primitives are `\dimen`, `\dimendef`, `\skip`,
`\skipdef`, `\ifdim`, `\mag`, and their existing assignment/arithmetic and
conversion paths. Register indices are 0–255. New dimension parameters are
`\hsize`, `\vsize`, `\maxdepth`, `\parindent`, `\hangindent`,
`\emergencystretch`, `\lineskiplimit`, `\hfuzz`, and `\vfuzz`.
Glue parameters are `\baselineskip`, `\lineskip`, `\parskip`, `\topskip`,
`\leftskip`, `\rightskip`, `\parfillskip`, `\spaceskip`, and `\xspaceskip`.
Their initial values are zero, as in INITEX; `\mag` starts at 1000. Loading
plain-format conventions is outside this foundation.

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
return a non-`Copy` detached-node owner that append consumes. Capacity failure releases any transferred children. Node, box, and
ligature/discretionary payloads carry owned non-`Copy` child lists; only the
private COW copier duplicates them into another version. Raw node IDs are
internal references, never independent owners. Copying a root for a checkpoint
is distinct from copying a mutable TeX list. Future semantic list-copy commands
must allocate fresh nodes, while immutable font metrics can remain shared.

Nested release walks owned child lists using reusable session scratch, avoiding
recursion and allocations per node. Scratch must be empty on return and is not
checkpoint state. Chunk references retain their backing independently of engine
lifetime. All roots containing IDs must be captured with the matching node-store
version. A node ID from an abandoned branch cannot be used with another version.

The initial node kinds cover characters, ligatures and their boundary flags,
explicit/TFM kerns, glue, penalties, rules, horizontal/vertical boxes, and
discretionaries. No kind implements text execution yet. Box headers carry
dimensions, shift, and glue sign/order/ratio; packing will supply the compatible
glue calculation. Font metrics resolve through immutable font IDs rather than
per-character owners. Later paragraph/page algorithms traverse and splice these
lists directly, and publish detached immutable results without live node IDs.

## Driver and validation

The existing driver quiescent flag distinguishes completed outer commands from
errors that require restore. Font/text execution will add mode eligibility
separately: entering `advance` must require quiescence, while returning a budget
handoff and creating a checkpoint also require eligible external vertical mode.
The foundation does not introduce placeholder modes or layout behavior.

Validation covers independent arithmetic/format examples, exact operand cursor
boundaries, same-source INITEX register fixtures, grouped typed assignments,
restore/replay, and node mutation/free/reuse with older roots retained. Nested
release, chunk reuse, checkpoint lifetime, and branch independence receive focused
ownership tests and Miri coverage. This stage introduces no performance claims.
