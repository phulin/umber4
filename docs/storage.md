# Storage and semantic lifetimes

This proposal develops the allocation choices in [architecture.md](architecture.md).
The useful unit of ownership is usually a definition, invocation, list, resource,
or checkpoint. Token and node loops should operate on compact values without
allocating or updating reference counts for each element.

## 1. Stores with different jobs

| Data                                      | Owner and representation                              | Reclamation                               |
| ----------------------------------------- | ----------------------------------------------------- | ----------------------------------------- |
| Control-sequence identities               | Process interner; append-only bytes and dense IDs     | Process exit                              |
| Font files and parsed metrics             | Immutable resource cache keyed by resource version    | Cache or process lifetime                 |
| Live font parameters and settings         | Document-owned font tables and derived caches         | Last document/checkpoint root             |
| Macro definitions and durable token lists | Session-owned segmented append store                  | Session end initially                     |
| Invocation arguments                      | Immutable token chunks and small sequence descriptors | Last consuming input source or checkpoint |
| Group restoration records                 | Reusable stack vector                                 | Group exit                                |
| Scalar scanner work                       | Rust locals                                           | Function return                           |
| Line-breaking and packing work            | Reusable scratch arrays or marked bump region         | Operation return                          |
| Mutable TeX nodes and tables              | Document-owned stores with snapshot sharing           | Last document/checkpoint root             |
| Shipped output                            | Immutable page records and resource references        | Last retained result/checkpoint           |

Appending immutable definitions for the whole session is deliberately a cheap
starting point. Repeated editing can eventually make that store large; segment
retention or deduplication can be introduced independently of its read interface.
An abandoned execution tail can be rewound after younger roots and owners are
dropped, using the append boundary retained by a checkpoint. Definitions in the
retained prefix still have session lifetime. The interner continues growing
across rollback. A name's ID is permanent; its
meaning belongs to the current document's tables.

## 2. Three allocation primitives

Use ordinary Rust containers for tables, stacks, and reusable working arrays.
Introduce custom allocation only where bulk construction or stable layout helps.

A scratch arena owns aligned chunks and supports nested marks. A function drops
its live values before rewinding to its mark. Restrict the fast path to values
without destructors; owned resources remain in ordinary locals or an explicit
owner alongside the arena. Nested calls consume a suffix and return it together.

An immutable append store allocates stable chunks. Builders reserve space in the
destination, initialize it, and publish an immutable range. Growing the store adds
a chunk rather than relocating published contents. A partially built value has
one builder owner; completion transfers its storage and descriptor in place.
Keep raw arena ranges as plain token/cell data where possible. Headers and
auxiliary owners needing destruction use ordinary containers alongside the
chunks, so truncating an abandoned tail also drops its initialized owners.

Token chunks can hold `UnsafeCell<MaybeUninit<Token>>` cells. The uninitialized
array is valid storage; appending constructs each token before publishing its
index. Reads and cursor windows cover only the initialized prefix. These cells
have token size and alignment through
[MaybeUninit's layout guarantee](https://doc.rust-lang.org/std/mem/union.MaybeUninit.html#layout),
and boxed chunks keep their addresses as the directory grows. Dropping a chunk
releases its allocation; token words own no resources. This permits direct
allocation without first filling the chunk.

Interned spelling bytes use the same append-store lifetime. Pack new spellings
into stable byte chunks, with one contiguous range per name. A spelling longer
than the normal chunk size receives a sufficiently large chunk. Initialize its
complete range before publishing its dense-ID descriptor; later insertions write
only disjoint suffixes. Byte cells can use `UnsafeCell<MaybeUninit<u8>>`, so shared
views of published prefixes coexist with construction in unused cells. The
interner owns every chunk and releases them together. The MVP's engine owns that
interner for its run; the full interpreter retains it across checkpoint restores.
Symbol spellings continue to use static bytes. Mutable meanings remain separate.

A mutable node store allocates compact cells in chunks, with bump allocation for
new space and reusable free ranges for material TeX discards. Its snapshot roots
retain the chunks needed by older states. It shares the arena's bulk allocation
benefit while permitting replacement and mutation of long-lived boxes and lists.

The narrow unsafe layer, if needed, covers aligned chunk allocation, initialization,
and direct cursor access. Its documentation explains which ranges are initialized,
what keeps their addresses stable, why mutable borrows are exclusive, and when a
chunk can be recycled. Callers get builders, IDs, and scoped views; they do not
manually coordinate raw-pointer reclamation.

## 3. Immutable token sequences

Keep the span/sequence representation from [expansion.md](expansion.md). A sequence
contains one contiguous range in the common case and several ranges when existing
input and newly written tokens are mixed. Retaining a compatible range changes
ownership metadata, not its token words. A transform writes only the tokens it
actually changes into destination storage.

Session-owned ranges need no individual reference counts. Shorter-lived capture
chunks use shared ownership at chunk boundaries. A completed argument packet owns
its sequence descriptors and retains the backing chunks directly. A child packet
retaining an argument range acquires that chunk, so the enclosing packet can die
without creating a chain of packet owners. Repeated occurrences of a parameter
reread the same immutable sequence.

New file tokens are materialized once, directly in the packet's chunks. Definitions
are built directly in session storage. A builder can mix owned new ranges with
retained old ranges and coalesce adjacent compatible spans. If a caller genuinely
needs contiguous output, it writes that output into the caller's destination;
normal expansion traverses the existing pieces.

An input source owns the packet needed by its cursor. A checkpoint copies cursor
positions and retains those same immutable owners. Exhausting a source releases
its ownership once; advancing a token cursor only moves its position.

## 4. Dense state tables

Separate identity from mutable state. Control-sequence IDs index a dense meaning
table whose entries contain a command, payload, and assignment level. Category
codes and other small character tables use fixed arrays. Registers use the same
assignment mechanism with a representation suitable for their domain: dense
storage for commonly used indices, and lazily allocated blocks for the wider
e-TeX register range.

Copy small arrays at checkpoints. For a large meaning or register table, share
blocks between checkpoint roots and the live root. On the first write to a shared
block, copy that block; subsequent writes use its unique storage. Reading requires
an index calculation and ordinary load, with no reference-count operation.
Block size is a measured layout choice, not an architectural constant.

Parsed font metrics provide an immutable baseline. Live `\fontdimen` values,
`\hyphenchar`, `\skewchar`, font allocation state, and profile-specific settings
belong to checkpointed document tables. Mutating spacing parameters invalidates
the document's derived space-glue cache. These assignments are globally effective
in TeX, while rollback still restores their earlier document values.

An interner may contain IDs beyond the restored table's extent. Those entries read
as the profile's default undefined meaning, and blocks are allocated when an
assignment first needs them. Active-character identities and single-character
control sequences occupy distinct ID domains. Format-owned initial entries can
provide a shared immutable baseline for each document.
Engine-private frozen commands and permanent font selectors also have distinct
reserved identities; printable spelling does not merge them with user names.
Their compact tokens still use the same direct meaning-table path.

Table payloads use compact handles to session definitions, immutable values, or
document nodes. Shared owners belong to a block or store root where possible.
Cloning a table block therefore copies entries in bulk; it should not turn a
meaning lookup into a heap access to an individually allocated entry.

## 5. Mutable nodes and snapshot roots

TeX's live lists need mutable links and fields: box dimensions can change, an
unboxed list can be spliced, and page building can separate material at a chosen
break. A document node store makes those operations direct. Start with chunked
storage and stable numeric node IDs, with a null ID reserved. IDs identify a cell
in a store version; they are not process-global identities like control sequences.

The live root holds the chunk directory, allocation state, and list roots.
A checkpoint retains its own root and shares initialized chunks. Before modifying
a shared chunk, the live store makes its copy writable and updates the directory.
Copies preserve cell offsets, so links and node IDs remain valid in each root.
New construction starts in unique chunks rather than copying a shared tail just
to append nodes. Reuse free ranges in unique chunks first. Free-list state is part
of the versioned store; writing a reused cell in a shared chunk requires a writable
copy, leaving an older version's cell unchanged. Fully free chunks can be omitted
from new retained roots and recycled when their last older owner disappears.

A node read resolves its ID through the current root. A bulk list operation can
obtain a scoped chunk view, pay the uniqueness check once for that chunk, and then
walk or edit its cells directly. That view cannot survive another operation that
replaces the chunk. Store-version ownership stays at the operation boundary;
there is no reference count or generation check on each list link.

Prefer compact tagged records with fields selected for TeX's common node kinds.
Keep large uncommon payloads out of the common cell. A Rust enum is a useful first
executable representation; a packed layout is justified by size and generated-code
measurements. This choice can change without changing the list ownership model.

For a first implementation, uncommon immutable shared payloads can be
reference-counted heap objects retained by chunk owners. Mutable external
payloads have their own copy-on-write owner or are copied before mutation;
semantic list copying likewise gives them independent mutable state. Immutable
glue specifications are small values, while tokens and resources already have
their dedicated stores. Chunk cloning retains the external owners its initialized
cells need. Reclamation releases them in bulk or through ordinary destruction.

This proposal makes node mutation and snapshots concrete without requiring every
TeX object to use the same technique. An isolated large object may use its own
copy-on-write owner if measurements show that copying node chunks is expensive.

## 6. Ownership transfers in TeX operations

List builders maintain a head and tail in destination node storage. Appending a
new node initializes it there; appending an existing list transfers its links and
ownership. Line breaking can reuse paragraph nodes while adding line boxes around
the selected ranges. Packing computes dimensions over an existing list and
constructs its box header directly beside it.

A box register owns the list it names. `\box` takes the box, and `\unhbox` or
`\unvbox` takes its contents. Their copying variants create new mutable node
records because the two resulting lists can subsequently change independently.
They still share immutable token, glue, and resource data. The save stack can own
an older box while a local assignment occupies the register; this is separate
from the node store's checkpoint sharing.

Marks retain immutable token sequences. Insertions transfer node lists into page
construction. A shipped page is lowered into immutable output storage, releasing
its live node ownership after shipout effects have completed. Output results keep
their own records rather than retaining the entire mutable node-store version.

These behaviors follow TeX's `copy_node_list`, `eq_destroy`, and box/list commands;
the arena arrangement is our proposed implementation.
See the [TeX source](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web).

## 7. Restore and reuse

Restoring a checkpoint selects its table and node roots, input cursors, and
immutable owners. The abandoned live roots are dropped together. Scratch arrays
retain capacity but reset their lengths; no scanner locals need reconstruction.
The session definition store and process caches remain available for replay.
Reclaim the abandoned definition tail only after dropping all younger owners,
as described in [checkpoints.md](checkpoints.md).

Numeric node IDs stay inside the state version that gives them meaning. Host
results expose detached immutable pages and diagnostics, so a discarded live
state cannot leave a host-held node ID referring into a later run.
Every root carrying node IDs is restored with its matching node-store version,
including backend objects such as pending PDF forms that still own TeX boxes.

The main experiments are table-block and node-chunk size, the cost of directory
copies, and the live material retained by checkpoints. Start by copying small
roots and sharing large chunks; change the store that dominates measured cost.
This keeps allocation and snapshot design adjustable without changing TeX's
ownership transitions or the expansion loop.
