# Resources, streams, and host adapters

This proposal supplies native and Wasm resource access for the interpreter in
[architecture.md](architecture.md). The core works with byte views and logical
streams. The adapters decide how to find and acquire those bytes.

## 1. A synchronous core interface

The interpreter makes ordinary calls to resolve a logical name, open an input,
load font or image bytes, or operate on a generated stream. A call returns its
answer or an ordinary error/request. It does not suspend a scanner.

Acquire the chosen format and its startup resources during session setup, before
creating the initialization restart root. Pending setup acquisition returns to
the host directly; paragraph-checkpoint replay applies after document execution
starts.

The native adapter can search configured paths and load files synchronously.
The browser adapter normally reads a prepared in-memory file bundle. If a resource
is absent from that bundle but can be acquired, it returns a request; the session
unwinds, restores its preceding checkpoint, and returns control to the host.
The host supplies the answer and replay enters the normal execution loop.
See [checkpoints.md](checkpoints.md).

Missing, pending acquisition, and found-empty are different outcomes. TeX's
file-existence checks and EOF behavior require that distinction. An adapter's
actual lookup failure follows TeX's selected interaction/error behavior;
pending acquisition uses restart before the operation can complete.

## 2. Byte ownership and lookup

A resource key identifies logical name, resource kind, lookup operation/search
policy, resolution context, and the active input view. A resolved lookup provides
the relevant existence and metadata answers without loading the whole resource.
Byte acquisition supplies an immutable content version and shared storage when
a reader or parser needs them. Font metrics, font programs, image data, format input, and
ordinary TeX files can have different lookup rules while using the same ownership
interface.

The native compatibility adapter follows the relevant TeX search conventions,
including extension defaults and resource-specific paths. Implement lookup
outside token and layout loops. Its result can cache a concrete path and immutable
bytes. The browser adapter resolves the same logical requests against its bundle
and configured provider. Name scanning remains in the interpreter, since its
expansion and termination are TeX semantics.

Input files retain shared bytes plus their mutable cursors. Metric and image
parsers consume borrowed ranges and build their parsed representation directly in
its cache destination. Only mutable normalization buffers need a separate owner.
Resource-cache keys include content version and parsing options, so a changed file
does not reuse an old parsed object merely because its name stayed the same.

Checkpoint validation retains per-resource input observations: prepared text
prefixes, observed EOF boundaries, full-resource versions, lookup outcomes, and
attributes actually queried by primitives such as `\pdffilemoddate`. Metadata can
change while byte content stays identical. Distinguish `\openin` probe policy
from `\input` acquisition policy in lookup and negative-cache keys; Web2C's
search behavior can differ between them. Resources used later than
a checkpoint can remain cached without becoming prerequisites of that earlier
state. Reopening a source consults the current input view rather than reviving a
stale resolution by filename alone.
Resolution checks the current overlay at each profile-ordered search candidate
before using cached external outcomes. Overlay creation, truncation, or removal
invalidates affected lookup results, including earlier cached misses.
Release obsolete ordinary-file versions when no active input view or checkpoint
needs them; reusable parsed font/image caches have their own retention policy.

## 3. Generated output and immediate semantics

Generated files live in a document-owned overlay. A logical file has versioned
visible bytes; each numbered stream and `\input` reader has its own file binding,
position, open/EOF state, and any buffered bytes. `\openout` opens for writing
and truncates that file, including when another handle already names it. It does
not reset the other handle's position. Writing at a handle's position usually
appends, but an aliased handle can overwrite an existing range.

Logical open also asks the adapter to accept or reserve the destination.
An unwritable or otherwise rejected destination follows TeX's open-error and
filename-retry behavior at that operation. A later failure to publish an accepted
result is an adapter I/O failure, distinct from TeX's filename scanning/recovery.

TeX distinguishes immediate execution from deferred nodes. `\immediate\write`
expands and writes at execution time. A normal `\write` retains its token list
in a node and expands it during shipout, with the then-current meanings.
Open and close commands can also be deferred. The core performs those effects
while traversing a shipped page, before publishing its completed page record.
This ordering is defined by `out_what`, `write_out`, and the write commands in the
[TeX source](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web).

Shipout suppresses ordinary deferred open/write/close effects inside leaders,
following TeX's traversal context. Terminal and log write destinations also keep
their command-specific routing rather than becoming ordinary generated files.

The selected Web2C host profile defines buffering and flush behavior. Executing
an immediate write does not imply flushing its bytes to another read handle.
Output buffers commit at their handle's offset when flushed; closing flushes them.
Generated-file readers retain their read-ahead state and observe visible file
bytes on subsequent fills. Preserve those small handle records at checkpoints.
Ordinary stable external resources still use simple immutable byte cursors.
This exception includes a reader opened on an external file before `\openout`
aliases and truncates it. Retained read-ahead bytes can outlive that truncation.
Their external backing is a checkpoint input observation; use full-version
validation initially for such aliased readers. File bindings use the adapter's
resolved identity so different names for the same file share these effects.
Web2C lowers writes to C stdio and opens output files in write mode; see
[fixwrites.c](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/web2c/fixwrites.c) and
[openclose.c](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/lib/openclose.c).

Visible files and buffers use chunked byte builders. The append path writes into
its destination chunk. Aliased overwrites make the affected chunks writable;
snapshots share untouched chunks and copy handle positions and buffers as needed.
Opening, closing, truncation, and failed-read EOF transitions retain their native
profile semantics. This small stream layer handles live generated content without
turning every immutable resource into a virtual filesystem.
At flush, pending chunks can transfer or become visible ranges without copying
their bytes. Generated-file metadata belongs to the versioned file state and
uses retained environment answers when timestamps are assigned.

The native CLI writes the accepted generated files to disk; an embedding receives
their logical results. Deferring physical publication lets restored execution
replace a suffix without undoing filesystem writes. Immediate versus deferred
TeX effects are still distinct inside the compilation. Physical visibility to
another process during a running compile is a separate adapter behavior, outside
the initial incremental-session contract.

## 4. Diagnostics and run environment

Diagnostics append to a document-owned result stream with an input-source
location and printable TeX context. Formatting happens on error or tracing paths,
not on every token. Retained input sequences and line owners provide context
without copying source text into each cursor.

A checkpoint keeps the accepted diagnostic prefix. Resource-request replay drops
the abandoned suffix, so the host does not receive repeated messages as new events.
A native batch frontend can print accepted prefixes at checkpoint handoffs and
the remaining output on completion. Trace-heavy execution can keep a byte stream
instead of allocating a separate object for every line.

The run environment supplies TeX's date/time parameters and profile-specific
clock or random inputs. Store deterministic random-generator state in document
state. Supplied terminal answers form retained ordered input, with a cursor in
each checkpoint; successive reads of the same primitive consume distinct answers,
and replay consumes them again in order. External clock samples use equivalent
occurrence identity where their semantics require it. A pending request carries
its prompt context because rollback discards the diagnostic suffix. A new compile
can choose a new environment; restoring a checkpoint uses the retained one.

## 5. Font and image acquisition

Font loading separates immutable metrics from the document's mutable font state.
Parsed TFM data and original font bytes can be cached across documents; font IDs,
font parameters, character-use sets, expansion settings, and output numbering
remain document state. [paragraphs.md](paragraphs.md) and [output.md](output.md)
develop their layout and serialization uses.

Image loading separates immutable decoded/header data from the requested TeX
dimensions and backend placement. Depending on the format, PDF output can retain
the original compressed stream or an immutable imported object. Build a converted
representation once where conversion is required; page placement references it.

Acquisition may happen inside a scanner, layout operation, or shipout. The same
request-and-restart path handles it in each case. A parser error is a normal
engine error with the primitive's specified recovery, rather than a pending
acquisition request.

## 6. Multiple passes and external operations

The session driver owns repeated TeX passes. A pass reads a stable external input
view plus its live generated overlay; accepted outputs become the next pass's
inputs. Keep those layers distinct so an `.aux` file written during one pass is
not accidentally treated as an unchanged editor input on restore.
Passes and publication retain or transfer generated chunks directly; flattening
them into a second complete buffer is unnecessary for ordinary streamed output.

Bibliography and index tools run above the interpreter and supply generated bytes
to a later input view. The initial core contract does not execute arbitrary shell
commands. If a native adapter later supplies pdfTeX's shell-escape operations,
their effect and replay behavior need an explicit adapter contract. This does not
require adding continuations to scanners or treating external tools as TeX
primitives implemented by the layout engine.

## 7. Native and Wasm allocation boundaries

Keep native and Wasm on the same Rust engine and data layout wherever practical.
Byte loading, publication, and job scheduling are adapter responsibilities.
The browser runs the engine in a worker and returns at checkpoints; Rust's
`wasm32-unknown-unknown` target provides containers and allocation but not ordinary
filesystem services.
See the [Rust target documentation](https://doc.rust-lang.org/stable/rustc/platform-support/wasm32-unknown-unknown.html).

Cross the Wasm host boundary for resource acquisition and bulk results, not token
reads or node visits. Host-owned bytes can be copied once into module-owned
immutable storage when that ownership boundary requires it. Once inside Rust,
views, parsing, replay, and page construction reuse that storage. The result API
transfers or exposes complete byte buffers with an explicit lifetime; the host
does not retain pointers to mutable interpreter nodes.
