# Resources, streams, and host adapters

This proposal supplies native and Wasm resource access for the interpreter in
[architecture.md](architecture.md). The core uses shared input bytes, append
buffers, and ordinary stream handles. The adapters find and acquire the bytes.

## 1. A synchronous core interface

The interpreter makes ordinary calls to resolve a logical name, open an input,
load font or image bytes, or operate on a generated stream. A call returns its
answer or an ordinary error/request. It does not suspend a scanner.

Acquire the chosen Umber format and its startup resources during setup, before
creating the initialization restart root. Pending setup acquisition returns to
the host directly; checkpoint replay applies after document execution
starts.

The native adapter can search configured paths and load files synchronously.
The browser adapter normally reads a prepared in-memory file bundle. If a resource
is absent from that bundle but can be acquired, it returns a request; the session
unwinds, restores its preceding checkpoint, and returns control to the host.
The host supplies the answer and replay enters the normal execution loop.
See [checkpoints.md](checkpoints.md).

Prepare the format's normal resource bundle before execution and prefetch resources
used by the previous successful run. Acquired bytes remain cached across replay;
prefetching creates no TeX input observation. Sequential misses before a checkpoint
can otherwise repeat an increasing preamble prefix. Replay remains the fallback
for new dependencies, without speculative execution of TeX.

Missing, pending acquisition, and found-empty are different outcomes. TeX's
file-existence checks and EOF behavior require that distinction. An adapter's
actual lookup failure follows TeX's selected interaction/error behavior;
pending acquisition uses restart before the operation can complete.

## 2. Shared input bytes

Acquire external bytes once into an immutable shared buffer. Ordinary input uses
a cursor and ranges into that buffer; metric and image parsers build directly in
their cache destination. Normalized lines need their own buffer only when their
contents change. Cache parsed resources by content version and parsing options.

Name resolution remains an adapter operation outside token/layout loops. Its key
includes resource kind, lookup policy, context, and input view. Follow the selected
TeX search conventions, including the distinct `\openin` probe and `\input`
acquisition policies. Resolved file identity lets aliases name the same file.
A small document file table supplies generated contents at each search candidate;
creation, truncation, or removal invalidates affected cached lookup outcomes.
A removed destination masks its older external entry rather than exposing stale
bytes from the input bundle.

Input handles retain position, open/EOF state, and the read-ahead window required
by the selected host profile. For an immutable source, that window can be a range
in the shared buffer. If output later aliases the source, already buffered bytes
remain available and subsequent fills consult the file's current visible contents.
This state is recorded from opening, so an alias requires no reconstruction of
earlier reads. Generated-file read windows own their buffered bytes.

Checkpoint observations retain prepared prefixes, observed EOF, lookup outcomes,
whole-resource versions, and metadata actually queried by TeX. Use full-version
validation initially for external inputs aliased by output. Reopening consults
the current input view. [checkpoints.md](checkpoints.md) defines prefix validation;
obsolete source versions release when their last cursor/checkpoint owner disappears.
Parsed font/image caches have their own retention policy.

## 3. Generated files: a shared prefix and writable tail

Keep one file record per resolved identity and ordinary handle records for numbered
streams, input readers, and the main output. A handle names the file, not a frozen
byte version. Each writer retains the starting offset of its pending buffer;
its next-write position is that offset plus the pending length. Readers retain
their consumed position, refill position, buffered window, and EOF state.

Represent visible file contents as retained immutable byte ranges followed by a
uniquely owned pooled byte-block tail. Ordinary appends write into that tail. A
checkpoint moves a changed, nonempty tail into shared ownership and obtains another
pooled tail; it retains descriptors and lengths instead of copying accumulated
bytes. Freeze initialized bytes without shrinking or repacking their allocation. Finalized
stream/page publication can also seal a tail after its patches finish, then start
a fresh unique tail. Range descriptors themselves use shared append blocks with an
endpoint per file version, using the same mechanism as page records. A branch
shares complete blocks and copies at most its partial descriptor tail. Copying a
growing flat range list at every checkpoint would make retained metadata quadratic
in checkpoint count despite sharing all byte payloads.

Writes wholly within the unique tail update it directly. This includes PDF
stream-length patches during normal output; see
[pdfTeX's seek-and-write helper](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/pdftexdir/utils.c).
A write touching retained prefix bytes materializes the current file once into
a unique contiguous buffer, then applies the change there. Truncation installs
empty contents without copying the old prefix. This gives unusual alias overwrites
a simple cold path while normal append and tail patching remain direct.

The initial logical I/O profile is deterministic byte-oriented batch I/O. Readers
refill in 4 KiB windows; writers commit full 4 KiB buffers and flush the remainder
on close or an explicit flush/seek. Large writes follow the same block rule.
Text line preparation recognizes LF, CRLF, and CR as described in [input.md](input.md).
Path resolution and queried metadata are adapter-supplied answers. This fixes
Umber's visibility contract on native and Wasm; it does not claim identical
buffering to every host libc. Alias fixtures must use a matching reference profile
before claiming exact compatibility for those cases. Buffer size is a profile
constant, not discovered from filesystem block sizes during replay.

The profile determines when pending writer bytes become visible.
`\immediate\write` executes immediately but does not inherently flush.
Close flushes; seek/flush operations preserve the profile's visibility points.
At an append flush, transfer a completed buffer into a compatible unique tail,
preserving writable stream headers until their patches finish. Otherwise copy
only those pending bytes into the tail. Pending writer bytes and visible file
bytes are separate state. Checkpoint sealing changes ownership only: it leaves
pending writes unflushed. Copy small handle buffers into pooled snapshot storage
and retain their immutable range owners at checkpoints. Handle records and byte-owner metadata are pooled too.

`\openout` truncates the shared file at its execution point. Other handles keep
their offsets, pending bytes, and read-ahead. A writer flushing at an offset beyond
the new EOF produces a zero-filled gap. Later reader fills see the current visible
bytes, while buffered bytes and EOF state follow the profile's normal transitions.
Keep TeX's open/closed stream state separate from the reader's sticky EOF.
File-size, byte-dump, and checksum queries use visible contents; file metadata
updates at logical truncation/commit using retained environment answers.
The Web2C lowering and open behavior are in
[fixwrites.c](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/web2c/fixwrites.c) and
[openclose.c](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/lib/openclose.c).

Logical output open asks the adapter to accept the destination. Rejection follows
TeX's open-error and filename-retry behavior there. The native CLI publishes
accepted file ranges to disk; an embedding receives logical results. Publication
after accepted execution lets replay replace a suffix without reversing disk
writes. A later publication failure is an adapter I/O failure.

Deferred open/write/close nodes perform their effects in shipout order.
A deferred write expands with the then-current meanings. Leaders suppress the
specified effects; terminal/log destinations keep their command-specific routing.
These rules follow `out_what` and `write_out` in the
[TeX source](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web).

Restore selects the retained file contents and handle records, dropping the
abandoned tail. Measure tail capacity, range descriptors, pending buffers, and
prefix materialization on alias writes. Large ordinary output should stay on the
append path; more elaborate byte indexing can follow an actual measured need.

## 4. Diagnostics and run environment

### Implemented immediate diagnostic writes

`\immediate` reads exactly one expanded token. An ensuing `\write` is executed
immediately; any other supported non-extension token is backed by identity for
the ordinary driver. It does not skip spaces or `\relax`, and the lookahead's
one-read suppression has already been consumed. Deferred `\write`, stream
open/close operations, and pdfTeX shell stream 18 return `Error::Unsupported`.
There are no file-stream or shell side effects in this slice.

Immediate writes first scan an unexpanded balanced list, under the ordinary
absorbing scanner rules. They then expand that retained list at the write's
execution point. The second scan has a private closing brace and outer
`endwrite` token, and a source boundary prevents every read, including normal
scanner exceptions, from consuming the caller's input. A malformed closing
sequence fails with `Error::Syntax`; forbidden outer/runaway reads keep their
ordinary scanner error. Expansion uses TeX's mode-zero environment: surrounding
horizontal/vertical lists are untouched, `\prevdepth` and `\spacefactor` are
invalid, and last-item quantities are zero. Mode and scanner context restore on
success and failure. Expansion's legitimate conditional/name effects remain
document state. The temporary input sources and token owners end before handoff.

Negative streams select log-only output. Streams 0 through 15 are unopened in
this slice, and select terminal-and-log output, as do nonnegative streams above
15 other than unsupported stream 18. Each successful write appends one owned
diagnostic record after expansion and validation have finished. Rendering uses
the same supported token spelling and printable-byte rules as message output.
Rendering follows the default `\newlinechar=-1` behavior; the parameter itself
is outside the current primitive slice.

`Outcome::diagnostics` is one chronological `Vec<Diagnostic>` containing
`Message` payloads and `Write` payloads with an explicit diagnostic destination.
It replaces the untagged `messages` field; there is no parallel log copy.
Engine/checkpoint roots share the same immutable append blocks, now containing
typed diagnostic records. Restoring replaces the accepted prefix, and published
outcomes own their bytes independently. The CLI preserves its message rendering,
prints terminal-and-log writes as lines, and leaves log-only records available
through `Outcome` without implicitly creating a log file. Exact terminal/log
selectors and retained file-stream publication beyond this boundary remain
adapter and later I/O work.

These scanning and immediate-prefix rules follow `new_write_whatsit`,
`write_out`, and `do_extension` in the [TeX source](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web),
with stream 18 reserved by the [pdfTeX extension](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/pdftexdir/pdftex.web).

### Broader diagnostic environment

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
The executable scaled cache owns dense character-metric records, scaled kerns,
and baseline parameters in typed pooled buffers. These immutable baseline arrays
are independent of restored document parameters; detached page font handles retain
their scaled-header chunk and sealed source-resource owner.

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
view plus its document file table; accepted outputs become the next pass's
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
The browser runs the engine in a worker and normally yields at eligible checkpoints;
interruption unwinds to the last checkpoint as described in
[checkpoints.md](checkpoints.md). Rust's `wasm32-unknown-unknown` target provides containers and allocation but not ordinary
filesystem services.
See the [Rust target documentation](https://doc.rust-lang.org/stable/rustc/platform-support/wasm32-unknown-unknown.html).

Cross the Wasm host boundary for resource acquisition and bulk results, not token
reads or node visits. Host-owned bytes can be copied once into module-owned
immutable storage when that ownership boundary requires it. Once inside Rust,
views, parsing, replay, and page construction reuse that storage. The result API
transfers or exposes complete byte buffers with an explicit lifetime; the host
does not retain pointers to mutable interpreter nodes.
