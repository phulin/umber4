# Bootstrap and Umber format files

Umber builds formats through initialization TeX, exports its own `.fmt` files,
and reads those files into a reusable initial semantic state. The same format
encoding serves native and Wasm. Document checkpoints remain in memory at the
boundary in [checkpoints.md](checkpoints.md).

## 1. Bootstrap through the same interpreter

Create the profile's primitive identities, initial meanings, character tables,
default parameters, null font, and other initial values through compact bootstrap
tables. Primitive IDs are canonical process identities; their initial meanings
are values in the format/document state.

Initialization mode executes format-building TeX with the same expansion,
assignment, grouping, and layout code as ordinary documents. It enables the
specified initialization-only operations, notably loading hyphenation patterns.
The extended-mode flag and pdfTeX profile behavior remain explicit initial state.

At `\dump`, retain the semantic values upstream's format operation would write,
apply its dump-time normalizations, and export them in Umber's encoding. Capture
those values before later termination-only changes. For example, the written
`\tracingstats` value precedes cleanup's reset, while e-TeX's pre-dump enhancement
resets belong to the exported state. The completed stores can also become an
in-memory format baseline directly.

Resource acquisition for setup precedes the document's initialization restart
root. A pending request returns to the host; the format build restarts from
bootstrap with the newly supplied resources. Reuse the setup environment and
ordered answer history, rewinding its consumption cursor. Drop abandoned mutable
state, diagnostics, and generated output with the temporary stores; process
interning and acquired resources remain available. Reading an exported format
makes this rebuilding unnecessary for ordinary startup.

TeX's INITEX initialization, `store_fmt_file`, and `load_fmt_file` define what
belongs in a format and what startup initializes separately. Use the
[TeX format source](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web) and
[pdfTeX format extensions](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/pdftexdir/pdftex.web)
as semantic references. Umber's file layout belongs to Umber.

## 2. Retained semantic data

A format retains meanings, parameters and registers, immutable definitions,
retained node values, loaded font state, hyphenation data, and the selected
profile's flags. Stored font metrics come from the artifact, even if a later TFM
file differs. Hyphenation includes its saved character-code data and exceptions.
Mutable font alias groups retain the relationships in
[paragraphs.md](paragraphs.md), independently of the immutable metrics baseline.

Preserve the PDF subset upstream stores: object/payload tables, image metadata,
pending forms, allocation counters and last identities, and ToUnicode data.
Pending forms keep their owned boxes and matching node references. Per-document
startup then applies the specified emission/output resets while retaining these
format-owned values.

Freeze completed definition segments and resource baselines in place for an
in-memory cache. Retain useful token ranges directly, including shared bodies.
Release setup scratch and wholly unreferenced segments where practical. Export
writes the retained semantic data; overwritten setup history need not become a
permanent part of the file.

Each document gets independent mutable roots over the baseline, using the
per-type storage in [storage.md](storage.md). Small tables can copy eagerly;
larger table/node chunks become writable on mutation. Immutable definitions and
resources keep their cached owners. New document definitions own their immutable
storage and can retain compatible format spans directly. Assignments leave the
baseline and other documents independent.

## 3. An owned, portable encoding

Use a versioned header and an explicit engine/profile identifier. Encode counts,
tags, scalar values, and references with fixed-width fields and a specified byte
order. The writer and reader share these semantic record definitions. Supported
versions can track the engine's schema; a format from an unsupported version is
regenerated from its TeX sources.

File-local identities describe ordinary names, active characters, single-character
control sequences, frozen commands, and permanent font selectors. Ordinary names
resolve through the process interner when loading. Private commands map by their
reserved role; permanent selectors map by font identity, independently of their
printable spelling. A token captured from `\the\font` must keep selecting its
original font after the named selector changes.

Internal references between definitions, token sequences, fonts, nodes, and PDF
objects use file-local IDs. TeX-visible object/image/form numbers and allocation
counters retain their values: those numbers may already appear in registers or
macro text. Remapping internal handles leaves those values intact.

Preserve alias relationships: shared definition headers stay shared, and distinct
headers stay distinct even when their bodies compare equal. Distinct font
descriptors can share parameter, tag, or code-table groups; encode those groups'
identities separately. Node links and backend references use the same object
mapping, including pending form boxes. Encode shared token payloads once, with
sequence spans naming the payload, offset, and length.

The writer can enumerate retained objects and stream their records and immutable
ranges directly to the output builder. The reader resolves IDs and initializes
records in their final stores. Rewrite token identities directly into destination
chunks while decoding; remapping is a one-time startup cost. Encoded counts size
file-ID maps and object stores; meanings install at the mapped process IDs, with
undefined gaps remaining lazy. Use compact fixups where forward references need
them. An intermediate tree of decoded objects adds no value here.

Check versions, counts, tags, numeric limits, and reference validity before
publishing the completed baseline. This encoding is initial semantic data;
Rust calls, document input cursors, and scheduler state have no file representation.

## 4. Document startup

Apply upstream's post-format startup initialization in its actual order. Stored
values and the new run's environment are distinct: input, job/output names,
date/time, output state, and runtime-only fields follow their profile's startup
rules. A source-built baseline and its exported/read-back equivalent start
documents identically.

The document gets an initialization restart root before reading its first source
tokens. That root is the fallback for an edit or missing resource before the first
eligible paragraph. Later checkpoints and rollback use the same live stores and
outer loop as [execution.md](execution.md).

## 5. Cache identity and ownership

Cache a loaded baseline by the Umber format resource's version and selected
profile. Cache a source-built baseline by its setup inputs and resource
observations, including versions, lookup/existence outcomes, EOF, and queried
metadata. Export can retain build provenance for regeneration, while the loaded
artifact defines its stored semantics. Fonts and other resources still needed by
a document retain their own resource identities and validation.

Before selecting a document restart, revalidate the chosen baseline's dependencies.
A changed format artifact or source-build input requires loading or rebuilding
the baseline, dropping its old document checkpoints, and creating a new
initialization root. Ordinary document edits can reuse the same baseline.

Modest format storage can have cache/process lifetime. Larger or multiple formats
can use shared owners and ordinary cache eviction. The process interner remains
append-only even after eviction; each document supplies the meanings for its IDs,
with default-undefined behavior for identities beyond its table.

## 6. Useful comparisons

Build plain TeX and real pdfLaTeX formats from source. Export and reload each in a
fresh process with a different interner history, then compare document behavior
against the source-built baseline and upstream INITEX/format loading. Include
private commands, captured font selectors, shared definitions, mutable font
aliases, hyphenation, saved boxes, and pending PDF forms.

Start two documents from one cache and mutate definitions, font settings,
exceptions, and saved boxes in one. Their independent behavior checks ownership.
Measure source construction, export, cold format read, and per-document startup
separately from full execution. Compare cold Umber format loading with native
pdfTeX format loading, and report warm cache reuse separately. Format preparation
and setup retries are distinct from in-memory document checkpointing.
