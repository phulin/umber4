# Shipout, DVI, and PDF

This proposal develops output around TeX's shipout semantics. The interpreter
traverses the supplied box, performs its ordered effects, and builds output
directly in retained destination storage. Completed page records contain bytes
and immutable resource owners, not live TeX node IDs.

## Executable detached baseline

The [page contract](pages.md) defines the current default and explicit shipout
paths, without DVI/PDF serialization. `Outcome.pages` is a cheap retained prefix; `Pages::iter`
and `get` expose immutable `Page` records. `Pages::iter` builds a temporary
directory of borrowed chunk references in an iterator-owned `Vec` for forward
document traversal. Each page owns packed box records (including parent indices
and glue settings), ordered glyph-run/rule records and
glyph bytes, count registers, output penalty, and an immutable font-resource table.
`Page::items` exposes compact ordered records; run ranges index `Page::glyphs`.
`Page::placements` yields owned absolute `Placement` values through a forward,
exact-size iterator; its construction and iteration allocate no storage.
It does not expose a placement slice, element borrows, range slicing, or
constant-time random placement access. Each traversal reconstructs
intra-run x coordinates using the retained immutable scaled font widths and
wrapping integer addition; it borrows the font descriptor once per run. Consumers
that need an owned position array can collect the iterator explicitly. Repeated
scans repeat the font-width reads and coordinate additions; no scan cache is
retained.

Coordinates and dimensions use scaled points. Coordinates include the restored
horizontal and vertical page offsets before any backend's implicit one-inch
origin; dimensions remain intrinsic to each box or rule.
Glyph and box y coordinates denote baselines, while rules use top-left corners.
Font indices are page-local. `FontResource` exposes source name, exact retained TFM
bytes, design/scaled size, and immutable glyph metrics, independent of the engine.

Within each page traversal, a local binding reuses the last glyph font's borrowed
immutable metrics and page-local index. Every glyph still checks character
existence before placement; resource registration follows successful validation.
The binding spans nested boxes and is discarded before the next page.

Runs stop at noncharacter nodes, document-font changes, and both nested-box entry
and exit, including empty boxes and rules that move without painting. Each run
starts at the exact absolute position computed by lowering, so negative kerns,
box shifts, and cumulative glue rounding remain eager. Every glyph/font and the
aggregate glyph-plus-rule `u32` capacity limit is checked during lowering;
iteration never postpones a TeX error. Single-glyph runs require more storage
than one absolute placement; the representation does not promise a saving for
every page.

The traversal uses existing node lists and cumulative glue rounding, writes
records and glyph bytes directly into typed pooled buffers, and releases the live page tree on
success or failure. Only complete successful traversals append a page record.
Shipout samples the current `\outputpenalty`, count registers, and offsets,
including assignments made inside an output routine. It consumes any nonvoid
horizontal or vertical box and resets dead cycles even on huge-box rejection.
Void shipout has no effect. Huge boxes are consumed and return `Overflow`;
incompatible magnification returns `Number`. The first accepted page prepares magnification, paired with the existing
restored first-use state. No raw node or document-font identity crosses the result
boundary. Both output and font header chunks have sixteen slots; held partial
output chunks may pin a bounded invisible suffix, and held font chunks pin the
registered source-resource set. Full backend operations below remain proposals.

Native validation compares boxes, glue settings, output penalties, and absolute
DVI movements for 29 cases against pdfTeX 1.40.25 (TeX Live 2023/Debian). The adapter
uses controlled TFM metrics and the three real resources in [fonts-text.md](fonts-text.md).
Its DVI reader is test-only, never an output backend. Run with the same explicit
native/font environment used by the paragraph adapter:

```bash
cargo test --lib upstream_page_breaks_boxes_glue_and_absolute_placements -- --ignored
```

The font-free fixtures in `fixtures/output/` cover direct and routine shipout,
multiple publication, dead cycles, returned material, retained routine input,
verbatim token-list copies, and fail-fast errors. The native adapter compares
TeX-visible markers, success/failure, page counts, and DVI placements (including
prefixes published before errors):

```bash
cargo test --lib upstream_output_routine_state_and_dvi_geometry -- --ignored
```

## 1. Shipout is execution

Shipout walks horizontal and vertical boxes, glue, kerns, rules, leaders,
characters, and extension nodes with the positions and rounding TeX specifies.
Use ordinary functions and a local traversal stack or recursion. The traversal
does not yield. Glue movement uses compatible cumulative rounding so many small
adjustments produce the same positions as upstream.

Deferred writes expand with the then-current meanings at their traversal point.
Open/close operations and specials preserve their ordering and leader exceptions.
pdfTeX position-saving and other extension nodes update TeX-visible document state
at their specified points. They cannot be moved into an independent renderer
that runs after execution has already advanced.

The traversal writes commands or content bytes directly into the page's output
chunks. It records resource references as needed, without constructing a second
heap graph of every glyph placement. A small typed placement helper can share
dimension logic between backends, while each backend has its own compact emission
path. Output bytes and resource descriptors use pooled append storage. Preserve
nested-box offsets, running rules, leaders, and selected page dimensions.

The main DVI/PDF file uses the same versioned file/handle contract as generated
streams. Open, truncate, and obtain destination acceptance at the upstream
operation's actual first-open point, before its ordered effects. That includes
PDF operations such as image scanning or immediate object emission that can
initialize output before any page. Retain the emitted file prefix, position,
and buffering state, so same-job file queries and aliased streams see the correct
bytes. The shared prefix and writable tail in [resources.md](resources.md)
support appends and stream-length patches directly. Before sharing completed
ranges with a page record, finish their patches and seal their byte owner;
subsequent output uses a fresh writable tail.

If acquisition returns a pending request, the ordinary result unwinds shipout and
the session restores its preceding checkpoint. Only a successfully completed
shipout appends its immutable page record. Other document mutations performed
during the failed traversal disappear with that restored state.
Genuine lookup failures and parser errors follow their TeX/pdfTeX error path.
A rejected huge page consumes its box and resets dead cycles without a page
record; initialization and other specified pre-rejection effects still occurred.

Reference behavior is TeX's `ship_out`, `hlist_out`, and `vlist_out`, plus pdfTeX's
extensions, in the
[TeX source](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web) and
[pdfTeX source](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/pdftexdir/pdftex.web).

## 2. Detached pages and document state

A completed page owns emitted content chunks, its count-register values and
dimensions, and immutable resource references. The document backend owns page
ordering, font/object numbering, resource-use state, and the other data required
for final serialization. Mutable backend tables use pooled COW records from
[storage.md](storage.md). Page/resource descriptors use shared append blocks and
an endpoint, so each checkpoint does not copy the growing page history.
Running links/threads, color stacks, and output-mode/init flags also belong to
backend roots across pages. Continuation payloads own their retained data after
the originating page nodes disappear; any retained node IDs use the matching
node-store version. Freeze each PDF setting at its actual initialization point:
version checks and first shipout initialize different settings, while compression
level is selected per stream. Retain the frozen values and initialization flags,
including initialization performed before a rejected page.

Retaining an output prefix shares completed pages. Restoring a checkpoint restores
the backend roots paired with that prefix, including allocation counters and
objects that have not yet been emitted. Abandoned output pages can remain in a
host's old displayed result, but they cannot supply live handles to the new
interpreter branch.

Pending PDF forms are an important exception to detached output: a `\pdfxform`
can own a TeX box until the form is emitted. That pending owner retains node IDs
in the matching document node-store version. Once traversed, its form stream is
immutable bytes. Restore selects the matching form status, backend root, and node
root together. Form traversal performs the upstream-prescribed effects and
context changes, rather than flattening the box at an arbitrary earlier point.
Placing a form emits its reference. At the end of page/form content traversal,
pdfTeX emits pending raw objects, images, and referenced forms in that order,
before its final resource dictionary. A form's deferred effects therefore run
once after the parent traversal, including a form placed in leaders; referencing
an already-emitted form does not run them again. An immediate form is traversed
at its primitive. An unreferenced non-immediate form remains untraversed.
Recursive form output restores the specified enclosing resource context, while
TeX-visible effects such as saved positions keep upstream's propagation behavior.

## 3. DVI emission

The DVI backend emits the required movement, push/pop, character, rule, special,
and font commands during traversal. Movement compression uses local/backend
state with the same scope behavior as TeX; it is not a host callback per command.
Font identities and first-use definitions are document state.

Build page bytes in their final chunk owner. The frozen preamble, current byte
count, and previous beginning-of-page offset are known during sequential shipout.
Emit the backward page link directly and retain those counters in backend state.
Restoring a prefix restores its offsets; the postamble is emitted after execution.

The preamble and pages enter the logical file during ordered emission. Finalization
writes the remaining font definitions, postamble, and padding through its existing
handle. Publication streams that file's authoritative ranges to a native sink or
the embedding's result buffer. Exact DVI comparison is useful where the
chosen command encoding follows upstream; equivalent encodings still require
independent structural and placement comparisons.

## 4. PDF object allocation and primitives

PDF primitive execution participates in document semantics before final file
serialization. Object allocation, `\pdflastobj` and related results, named
destinations, outlines, links, annotations, forms, images, page attributes, and
literal content have their required execution and shipout timing. Preserve
primitive-visible numbers and allocation order. Final serialization cannot assign
fresh numbers merely from the sorted set of objects that happened to be used.

Use a compact object directory with kind, state, and payload handle. Allocate
object numbers as the primitive/output operation requires. Payloads are immutable
byte chunks or owned structured records; pending forms can own node lists as
described above. Changes to status, destinations, page references, and deferred
object data are document-owned and copied or made writable at object/table
boundaries.

Raw object and literal data follow their primitive's exact token scanning and
expansion rules. Build newly produced bytes directly in their destination stream;
retain existing compatible byte ranges. There is no blanket rule that every
extension token list expands when its node is created or when the final PDF file
is written.

The PDF backend can lower ordinary placements straight into a page content stream
using the already allocated resource identities. Links, annotations, and save/
restore or matrix nodes carry the geometry needed by the traversal. Their mutable
records are independently copied/COW when required; sharing a containing node
chunk does not authorize mutation of a checkpoint's annotation payload.

Object-stream containers also allocate numbers at logical emission. The first
compressed object can reserve a container ID hidden from the user-facing object
count while advancing the system allocation counter. Retain that allocation,
open container grouping/occupancy, and its buffered object data in backend state;
final serialization reuses those IDs. Creating containers only at finalization
would change later primitive-visible object numbers. Pending container bytes can
stay in a small chunk builder until the container stream is closed.

When any PDF stream begins, select its current compression policy. Compress
directly into its retained destination and release disposable uncompressed work
once the stream closes. Completed page/form/resource streams keep their compressed
bytes, so previews or resumed results do not recompress the unchanged prefix.

## 5. Fonts and images

Layout uses TFM data and document font settings. PDF emission separately resolves
font maps, encodings, font programs, and embedding options. Immutable font bytes
and parsed representations can be shared across runs; object numbers, used
characters, subset membership, and profile-visible font settings are document
state. Expanded font variants preserve pdfTeX's sharing and numbering rules.

Record character use at emission and at primitives such as `\pdfincludechars`,
preserving query-triggered font initialization and object allocation too. Build
subset programs and encoding
objects directly in their final resource storage. Ordinary glyph emission sets
bits or updates compact use tables; it does not retain a heap owner per glyph.
Preserve font-type-specific behavior, including Type 1 and bitmap/Type 3 paths
needed by the selected pdfTeX profile. ToUnicode and map-derived resources use
their actual configured inputs, not metrics inferred from the TeX source text.

For PDF, virtual fonts execute immutable packet programs through ordinary calls
at character output: local fonts, movements, rules, and specials write directly
to the current destination. Retain document-local font bindings and discovery
state, while parsed packets can be cached. Outer advancement uses the virtual
character's TFM width. DVI retains the original font/character references for its
consumer to interpret.

Images retain immutable streams or parsed imports and their placement/resource
records. Repeated placement shares the same underlying image where pdfTeX does.
JPEG or other directly supported compressed data can remain in its original byte
owner. Decoding or PDF-page import writes the necessary converted representation
once; page content references it. Version cache keys by content and interpretation
options.

Resource acquisition belongs to [resources.md](resources.md). Request replay uses
the same synchronous core path even if acquisition occurred inside font emission
or PDF-page import.

## 6. Final serialization

After TeX's final page/output behavior completes, finalize the document backend
through ordinary calls. PDF serialization uses the retained object directory,
page tree, metadata, resource use, deferred objects, and cross-reference/trailer
data. Preserve the settings frozen at their upstream initialization points,
each stream's selected policy, object-stream state, and primitive-visible resource
relationships. Only objects whose required emission/finalization conditions hold
are emitted; finalization does not traverse every allocated unused form.

Emit genuinely deferred objects and the final file structures into the logical
output stream, recording offsets. Reuse already-emitted compressed streams and
file-prefix chunks. Deferred stream compression consumes its existing input chunks
and writes into the destination. An embedding that needs one contiguous file
allocates that file buffer as the destination.
Native streaming does not require assembling a second whole document in memory.

Restored execution continues from the logical file prefix and matching backend
state. Finalization writes through that file's handle, preserving any aliased
truncation or overwrite performed by TeX. The file's logical contents are the
accepted job output. A preview can separately serialize retained page/object data
into its own destination, without mutating the live backend or consuming pending
TeX effects.

Final result status also follows the selected backend/profile: a zero-page run
does not create DVI, unused immediate PDF objects do not create a page-only
result, and pdfTeX's specified fatal cleanup can remove the logical PDF output.
Retained preview pages are separate immutable host results; they do not turn a
failed current run into a completed file.

The DVI and PDF serializers differ where their formats require it. They share
input positions, exact dimensions, resource acquisition, chunk ownership, and
ordinary traversal helpers where those genuinely have the same semantics.

## 7. Compatibility and cost

Compare TeX-visible object/font results and ordered deferred effects as well as
rendered output. DVI gives precise placement evidence. PDF checks should inspect
content movements, resources, objects, links, forms, page boxes, and extracted
text; byte differences from metadata or compression need their cause understood.
Use an independent renderer to check representative final files.

Include copied boxes with annotations, repeated forms/images, leaders containing
writes, deferred writes affected by later definitions, saved positions, raw PDF
objects, and rollback after partial shipout. Compare a resumed result with a fresh
run before attributing differences to harmless serialization variation.

Measure shipout traversal, emitted bytes, resource parsing/embedding, compression,
and final serialization separately and in the full compile. Retaining output
chunks should remove repeated layout and copies without making common glyph
emission pay for snapshot or host machinery.
