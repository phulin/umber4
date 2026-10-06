# Bootstrap and reusable formats

This proposal covers starting a document without repeatedly rebuilding its format.
A reusable format is initial semantic data, not an on-disk execution checkpoint.
Document checkpoints remain in memory at the boundary in
[checkpoints.md](checkpoints.md).

## 1. Bootstrap through the same interpreter

Create the profile's primitive identities, initial meanings, character tables,
default parameters, null font, and other initial values through compact bootstrap
tables. Primitive IDs are canonical process identities; their initial meanings
are values in the format/document state.

An initialization mode executes format-building TeX with the same expansion,
assignment, grouping, and layout code as ordinary documents. It enables the
specified initialization-only operations, notably loading hyphenation patterns.
The extended-mode flag and pdfTeX profile behavior remain explicit initial state.
At `\dump`, retain the semantic values upstream's format operation would write:
apply its dump-time normalizations and exclude later termination-only changes.
For example, retain the written `\tracingstats` value before cleanup resets it,
while preserving e-TeX's pre-dump enhancement resets. Freeze those roots in place
without serializing a resumable Rust program.

Resource acquisition for format setup happens before the document's initialization
restart root exists. A pending setup request returns to the host and the format
build can restart from bootstrap with the newly supplied resources. Its abandoned
temporary stores can be released together; process interning remains available.
An already-built cached format makes this path unnecessary for ordinary sessions.

TeX's INITEX initialization, `store_fmt_file`, and `load_fmt_file` are the primary
reference for what belongs in a format and what startup initializes separately.
See the [TeX format source](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web) and
[pdfTeX format extensions](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/pdftexdir/pdftex.web).

## 2. A frozen semantic baseline

A format retains its canonical meanings, parameters and registers, immutable
definitions, any retained node values, loaded font state, hyphenation data, and
the profile flags required by the selected format. Payloads point to their real
owners: immutable token chunks, resource baselines, or the frozen node root.
Mutable font alias groups retain the relationships described in
[paragraphs.md](paragraphs.md).
The baseline also retains the PDF subset native formats store: object/payload
tables, image metadata, pending forms, allocation counters and last identities,
and ToUnicode data. Each document gets independent mutable roots over these
values, resets the specified emission/output fields, and pairs pending forms
with its matching node-store root. Clearing the entire backend would lose saved
box references; carrying old emission status would suppress required output.

Freeze the stores already built in place. Definition segments become immutable
format storage; table/node chunks and font state provide shared baselines. There
is no need to flatten every definition or copy every saved box into a second
representation just to start a document. A cache owner keeps these baseline
chunks available across sessions.
Release setup scratch and wholly unreferenced definition segments where practical.
Measure useful live definitions separately from overwritten setup history;
retaining reachable spans in place does not require retaining every abandoned
definition for the cache's lifetime.

A document creates its own small mutable roots over that format. Small tables
can copy eagerly; larger table/node chunks become writable on first mutation.
New definitions append to document session storage and can retain compatible
format spans directly. Reading either source uses the same token cursor.
Document font settings, runtime exceptions, and other mutable values never write
into another document's format baseline.

## 3. Startup state is distinct

Apply upstream's post-format startup initialization in its actual order. The
format's stored values and the new run's environment are distinct: current input,
job/output names, date/time initialization, backend state, and runtime-only fields
follow their profile's startup rules. A cached format must behave like loading
the corresponding format into a fresh engine, not like restoring a previous
document's partially finished page or generated streams.

The ready document then has an initialization restart root before reading its
first source tokens. It uses that root if an edit or missing resource occurs
before its first eligible paragraph. Normal execution and later rollback use
the same roots, stores, and outer loop as [execution.md](execution.md).

## 4. Loading existing native formats

Executing format sources is the most useful early compatibility path because
it exercises the interpreter instead of depending on a binary import layer.
For practical cold pdfLaTeX startup, the intended path is an importer for selected
known Web2C/pdfTeX `.fmt` versions, developed after the source-building path works.
Native and Wasm can load the same supported resource bundle. The importer decodes
semantic values into
our stores; native memory words are not Rust objects or portable Wasm layouts.

Validate the supported header, engine/profile flags, numeric bounds, and encoded
references while decoding. Intern ordinary names into the process interner;
map engine-private frozen commands and permanent font selectors by their identity
domain/role, independently of their printable spelling. A token captured from
`\the\font` keeps selecting its original font after its named selector changes.
Reserved compact IDs use the same token reader and meaning table. Rewrite token identities
directly into the destination token chunks, preserving macro sharing, flags,
register aliases, font references, and mutable font alias groups. Map node links
to the imported node store. These transformations justify a one-time import copy;
subsequent document execution shares the completed format.

The importer can remain independent of the ordinary token and node hot paths.
Unsupported format encodings return a setup error with the supported profile,
rather than executing with guessed layouts. Native and Wasm use the same decoded
semantic baseline. No general checkpoint serializer or pointer image is needed.

## 5. Cache identity and ownership

Cache a frozen format by its actual source/imported resource versions, selected
profile, and setup inputs that affected construction. Record relevant resource
observations while building it. A changed package, font baseline, pattern source,
or setup answer produces a different baseline; ordinary document edits can reuse
the same one.
Before choosing a document restart, the session revalidates this selected baseline.
Changed format/setup dependencies require rebuild or import, removal of its old
execution checkpoints, and a new initialization root. The old root cannot serve
as the fallback for a new baseline.

Format storage has cache/process lifetime when its size is modest and reuse is
likely. Larger or multiple format baselines can use shared owners and ordinary
cache eviction. The process interner remains append-only even when a format is
evicted. A document's meanings handle identities introduced by other formats or
later execution through the default-undefined rule from [storage.md](storage.md).

## 6. Useful comparisons

Compare source-built startup with upstream INITEX plus format loading, then compare
an imported baseline with the same source-built one. Loading plain TeX and an
actual pdfLaTeX format exercises definitions, aliases, character tables, fonts,
hyphenation, and extended-mode behavior together.

Start two documents from one cached baseline and mutate definitions, font settings,
exceptions, and saved boxes in one. Their independent behavior exposes shared
mutable-state mistakes. Measure format build/import separately from per-document
startup and full execution; a warm cached baseline is useful performance evidence
only when the comparison names the corresponding native pdfTeX startup path.
Also measure cold source build/import and successive missing-resource setup
retries. A prepared startup bundle avoids repeated construction without adding
resumable setup frames or an on-disk checkpoint format.
