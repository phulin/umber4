# Storage and semantic lifetimes

This document defines the storage contract for the interpreter. The MVP implements
immutable token storage and reusable invocation packets; node stores, snapshots,
and output storage remain proposals. [mvp.md](mvp.md) separates the current slice
from the remaining allocator changes.

## 1. Allocation contract

The system allocator supplies backing chunks and occasional capacity growth.
Objects are constructed and reclaimed through reusable stores selected by their
semantic lifetimes. Once capacity covers a fixed working set, repeating that work
must not require system allocations per definition, invocation, glyph, candidate,
or metadata record. Live growth and unusually large objects can acquire more
backing blocks; this is not a fixed maximum-memory engine.

Headers, counters, delimiter tables, span descriptors, and ownership records follow
this rule too. A token arena surrounded by individually allocated metadata is not
finished. Reusable Rust vectors are suitable for stacks, dense directories, and
working arrays. Their capacity grows with peak simultaneous demand, not cumulative
execution. Empty-block caches are bounded.

| Data | Storage and lifetime |
| ---- | -------------------- |
| Control-sequence identities | Session-owned append-only spelling chunks and dense IDs |
| Format tokens and immutable resources | Immutable baseline/cache blocks, released with their last owner |
| Definitions and durable token lists | Durable token blocks and pooled value/metadata records |
| Arguments and generated expansion input | Transient token blocks and pooled packets, retained until consumed or escaped |
| Group restoration records | Reusable live stack; snapshot-owned copies of records and saved owners |
| Scanner and layout scratch | Rust locals, reusable arrays, or nested arena marks |
| Mutable nodes and large tables | Stable-ID stores with pooled COW chunks |
| Shipped output and diagnostics | Immutable append blocks, including their descriptors |

The MVP uses one token-block mechanism with separate active blocks for durable
definitions/register values and transient captures/generated input. Its bounded
empty-block cache shares complete block records, retaining their non-atomic
ownership counters together with the token backing. A reused block gets its new
allocation class only after its last previous owner has released it. Each class
retains at most one active block; the shared cache retains at most sixteen empty
blocks. Immutable format blocks remain a future class.

Use the same allocation mechanisms with separate active blocks for format,
durable-document, and transient-expansion data. Do not put every invocation in a
separate chunk, and do not mix all lifetimes into one active bump tail. Allocation
class describes expected retention; actual semantic owners decide reclamation.

Definitions may outlive bindings through aliases, saved values, input sources,
captured ranges, or checkpoints. Arguments can outlive their invocation when a
new definition captures them. No group or invocation rewind may invalidate those
owners. Tokens refer to control-sequence IDs, not owning definition pointers, so
recursive macro names do not create ownership cycles.

## 2. Allocation primitives

Keep a small set of concrete stores rather than a universal allocator interface.

A scratch arena owns aligned chunks and nested marks. Its fast path stores values
without destructors; owned resources stay in locals or explicit owner records.
Returning from an operation releases owners before rewinding. Live argument data
belongs to the token store, not to scratch that the scanner rewinds on return.

Pooled slabs hold stable value headers, packet records, chunk headers, and small
metadata. Size-class blocks hold variable-length arrays such as parameter tables,
delimiter fallback tables, and span lists. Common empty/singleton cases stay
inline. The pool reuses initialized capacity and releases external owners before
making a slot available again. Large arrays receive appropriately sized blocks.
Non-atomic counters in these records implement shared ownership where needed;
there is no separate system allocation for each counter or wrapper.

The MVP reuses capacity in existing value slots for token piece descriptors,
parameter descriptors, and one flat delimiter-fallback array. An unpublished
builder takes the cleared token descriptor storage and edits its slot's metadata
exclusively. Publication returns token descriptors without moving token words;
failed builders return them before last-owner release. Release drops all page
owners and clears metadata lengths before making the slot reusable. Capacity stays
with the slot until registry teardown or, for surviving owners, final release.
Headers share stable slabs, and variable metadata shares typed size-class pages.

### MVP stable header slabs

The MVP header representation uses fixed slabs of 64 value slots. The
registry retains each slab and assigns dense IDs to its initialized slots; growing
the dense non-owning pointer directory never moves a slot. ID lookup remains one
checked pointer load. Each slot embeds its non-atomic external-owner
count. An `Owner` remains one pointer to the slot, so reading a value or a token
window does not resolve another table. Cloning increments only the slot count.
The first owner of a checked-out slot retains its slab once, and final-owner
release clears token/definition resources before returning its ID and releasing
that slab reference. The registry owns a separate slab reference. A surviving
owner or unpublished builder therefore retains its entire slab after registry
teardown, without a slab/slot ownership cycle.

Slab allocation, in-place initialization, and final deallocation form a narrow
unsafe layer. Every slot is initialized before the slab is exposed. Only
unpublished, exclusively owned builders can mutate contents; cleared slots cannot
have outstanding views. Final slab release occurs after the releasing owner has
finished accessing its slot. Slabs retain the weak token-pool link once, rather
than once per value. Capacity growth remains proportional to the largest live
header set, with at most 63 unused slots in its final slab.

### MVP typed metadata blocks

Token piece descriptors, parameter records, and delimiter fallback entries use
three independent typed stores. Each growable buffer has the same 24-byte size as
a native `Vec`: pointer, `u32` length/capacity, and a shared store owner. Buffers
retain their cleared capacity in their value slots. A store divides stable typed
pages into equal power-of-two blocks, starting at four elements; buffers acquire a
free block and transfer initialized elements when growing. Common blocks share
pages of up to 4 KiB and at most 64 blocks. Arrays larger than that use one backing
allocation per large block. Length, rounding, byte size, and growth are checked.

Only a buffer owns its initialized prefix. Growing transfers those elements and
returns the previous block without dropping the moved values. Clearing sets the
published length to zero before dropping the previous prefix, and final buffer
release clears all initialized elements before recycling the block. Pages contain
`UnsafeCell<MaybeUninit<T>>`; typed ownership handles keep the store alive after
registry teardown. Free blocks contain no live values. Stores retain metadata
pages to each size class's high-water capacity, while buffers retain their largest
blocks just as the prior per-slot vectors did;
there is no cache of independently allocated empty metadata objects.

### MVP packet and scanner metadata

The packet directory and free IDs remain amortized dense vectors. Each packet's
argument table, retained-owner table, and each argument's overflow span list use
the same typed block primitive with independent stores for `Sequence`, `Owner`,
and `Span`. Argument reset retains descriptor capacity, clears spans, and then
releases retained value owners. Resizing an argument table transfers initialized
sequences, including their span-buffer ownership; removed sequences release their
buffers before blocks can be reused. Packet buffers retain their stores and value
owners through engine field teardown, so destruction order does not invalidate
unconsumed input.

Expanded-name scanning keeps its 64-byte inline prefix. Long-name scratch uses a
reusable pool with one buffer per simultaneously nested scanner; nested expansion
cannot borrow the outer scanner's storage. Both success and scan failure clear
and return scratch. Retained scratch capacity follows nesting and name high-water
marks, rather than the number of long control names scanned. Returned diagnostic
messages remain independently owned byte vectors at the public result boundary.

### MVP allocation boundaries

The remaining system requests supply token pages and their shared records, header
slabs, typed metadata pages, spelling chunks, and capacity growth of dense
registries, hash buckets, input lines, and driver stacks. These structures grow
with retained state or peak simultaneous demand. Fixed-size engine roots allocate
once. Repeating a warmed fixed workload reuses these stores, including nested
argument packets and long-name scratch. First long-name spill reserves the inline prefix plus its overflow byte together,
so it starts in the 128-byte size class and reuses that capacity on later calls.

The MVP's public message result remains `Vec<Vec<u8>>`: each `\message` constructs
independently owned output bytes and transfers that vector to the result. Those
adapter/result-boundary allocations are intentionally retained; this revision does
not redesign public output ownership. The planned append-block diagnostic/output
stores belong to the later runtime architecture. The optional whole-file lexer
prototype is separate from the engine and keeps its own benchmark buffers.

Typed pages retain each size class's capacity until its last buffer owner dies;
a surviving value can retain its entire 64-slot header slab and shared metadata
stores. Large metadata blocks can allocate individually, and token ranges can pin
partially occupied pages. These are explicit retention/granularity limits, not
per-definition or per-invocation allocation on a warmed working set.

An immutable token store allocates stable blocks, initially by bump reservation.
Builders initialize destination cells and publish ranges without moving tokens.
A failed builder releases its reservations and owners. Tail reservations rewind
when possible; other dead ranges become reusable through the store's block/free
range policy. Keep durable and transient active blocks separate. A large immutable
value may span blocks without requiring a contiguous copy at publication.

A mutable node store uses compact cells, bump allocation for new space, and free
ranges for discarded nodes. Snapshot roots share chunks; mutation first makes the
relevant chunk writable. Temporary math atoms have their own allocation class
within the same node-ID domain, so their disappearance does not leave holes among
long-lived results. Table blocks use the same ownership boundary.

Append stores hold output bytes, page records, and descriptor sequences in pooled
blocks. A retained prefix is a block root and endpoint. Branching after restore
shares complete blocks and copies at most the bounded partial tail; it never
copies all preceding descriptors simply to append. Byte and descriptor blocks
both release when their last retained prefix disappears.

The unsafe layer covers aligned allocation, initialization, stable views, and
reclamation. Published token cells may use `UnsafeCell<MaybeUninit<Token>>`;
only initialized ranges are readable. Headers are mutable during construction and
reclamation, and read views borrow a live owner. Stable slab/block addresses must
survive directory growth. Slot reuse requires all external owners and views to be
gone. APIs should expose builders, compact IDs, and scoped views rather than ask
callers to coordinate raw pointers. See
[MaybeUninit's layout guarantee](https://doc.rust-lang.org/std/mem/union.MaybeUninit.html#layout).

Session spelling storage uses append-only byte blocks, one contiguous range per
name. Session teardown releases its extension; restoring a checkpoint does not.
A shared in-memory format owns an immutable name-ID prefix adopted unchanged by
each new session. This keeps shared baseline token IDs valid. Format-file decoding
remaps file-local names into that prefix once; new source/dynamic names append to
the session extension. Changing baselines starts fresh document/session roots.

## 3. Immutable token sequences

Keep the span/sequence representation from [expansion.md](expansion.md). A sequence
contains one contiguous range in the common case and several ranges when existing
input and newly written tokens are mixed. Retaining a compatible range changes
ownership metadata, not its token words. A transform writes changed tokens into
destination storage. Small escaping slices may also be copied to avoid retaining
disproportionate backing storage.

Stored ranges retain their immutable backing values at source and packet
boundaries; those values own their chunks. A completed argument packet owns its
sequence descriptors and retains each backing value once. A child packet acquires
the value behind a forwarded range, so the enclosing packet can die without
creating a chain of packet owners. Repeated occurrences of a parameter reread
the same immutable sequence. Descriptors should name backing token blocks directly
where practical; avoid chains of composite values retaining entire producers.

At a lifetime extension, retain a substantial existing range or copy a small
range into its new owner's allocation class. Coalesce adjacent spans and allow
bounded coalescing copies when descriptor fragmentation costs more than the tokens.
These policies affect storage only: token identity, category codes, order, and
scan failure behavior stay unchanged. Begin with inline singleton arguments and
separate allocation classes; add thresholds only for a demonstrated retention case.

New file tokens are materialized once, directly in the packet's chunks. Definitions
are built directly in their owned destination storage. An argument can mix owned new
ranges with retained old ranges and coalesce adjacent compatible spans. If a caller
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

Table entries contain compact payload handles. Each physical table block retains
the immutable owners used by its entries. Sharing that block at a checkpoint
increments the block owner only. Making a writable copy copies entries and retains
their payload owners once for the new physical block. Replacing an entry in a
unique block releases its previous owner and retains or transfers the new one.
Dropping the block releases its remaining owners. Repeated overwrites must not
accumulate a historical owner list. Meaning reads never touch these counters.

Snapshot save records independently own saved values. Restoring a group transfers
those values back to live bindings, applying the local/global rules in
[execution.md](execution.md). Sharing a snapshot must not make its save records
subject to a live stack rewind.

## 5. Mutable nodes and snapshot roots

TeX's live lists need mutable links and fields: box dimensions can change, an
unboxed list can be spliced, and page building can separate material at a chosen
break. A document node store makes those operations direct. Start with chunked
storage and stable numeric node IDs, with a null ID reserved. IDs identify a cell
in a store version; control-sequence identities instead belong to the session.

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

Uncommon payloads use typed pooled records or dedicated large blocks. Immutable
glue specifications are small values; tokens and resources have dedicated stores.
Each physical node chunk retains the external payload owners its live cells need,
using the same share/copy/replace/release rules as table blocks. Reusing a cell
releases its previous external owners. Mutable external payloads have matching
COW roots or are copied before mutation, including during semantic list copying.
Sharing a containing chunk never permits mutation of an older payload version.

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
Definitions shared with retained state remain available for replay. Dropping
abandoned table roots and sources releases their immutable owners. Process caches
and interned identities remain available across restore.

Numeric node IDs stay inside the state version that gives them meaning. Host
results expose detached immutable pages and diagnostics, so a discarded live
state cannot leave a host-held node ID referring into a later run.
Every root carrying node IDs is restored with its matching node-store version,
including backend objects such as pending PDF forms that still own TeX boxes.

Snapshot records and COW copies draw from reusable blocks too. Initially copy
small mutable root directories and compact stacks; share large data chunks and
append-only descriptor blocks. A full output history must not be recopied into
each checkpoint. Root release is proportional to released records/blocks, and
belongs in restore latency as well as memory accounting.

Check fixed-working-set reuse, mixed transient/durable survivors, and repeated
snapshot/mutation/restore. Count backing allocations, metadata, copied bytes, and
retained capacity separately. [performance.md](performance.md) defines the common
measurement method.
