# Portable Umber formats

An Umber format is initial semantic state for a fresh job. Loading performs no TeX
tokenization or expansion. Source-bound in-memory checkpoints are a separate
representation and cannot serve as format files.

## Semantic boundary

The format foundation retains ordered names, meanings, immutable macro/token
Values, integer and character-code tables, dimensions, glue, mu glue, paragraph
shape, and token parameters including output and everyjob. Distinct Values stay
distinct; aliases share one Value. Validated token ranges reconstruct macro
delimiter/KMP metadata directly in the existing pooled destination buffers.

Loaded font identities, mutable parameters and exact used TFM resources persist,
as do compiled hyphenation patterns and exceptions. Void box registers are
supported; live box-register forests return Unsupported and are never reset or
silently dropped.

`Engine::encode_format` requires a quiescent external-vertical boundary, closed
groups/conditionals, no pending afterassignment, no construction/output frames,
and no contribution/page/output material. Assignment levels are all global.
File cursors are excluded. Exhausted token-source wrappers do not prevent capture;
live token continuations do. Initialization `\dump` requires the same semantic
boundary and discards setup input continuations before ending initialization.

## Wire contract

All numbers are little endian and fixed width. The header is eight bytes
`UMBERFMT`, u32 schema version 1, u32 portable-engine profile 1, and u64 total
byte length. Eight sections follow in fixed order, each with a u32 kind and u64
payload length: names (1), fonts/resources (2), Values (3), meanings/token
parameters (4), scalars (5), layout bindings (6), hyphenation (7), boxes (8).
Unknown versions/profiles, reordered sections, trailing bytes, and inconsistent
lengths are invalid. Counts/IDs are u32; scalar words are i32.

Names preserve ordered IDs; single-character, active, and private frozen
identities have reserved IDs. Values use dense file IDs relocated once into
pooled headers and token chunks. No pointer, Rust layout/padding, usize, Rc count,
chunk handle, hash bucket, free list, or KMP failure array appears in the image.

Dependent sections use tag 0 for their supported empty/canonical state. Fonts and
hyphenation use tag 1 for complete records; the box section reserves tag 1 and
returns Unsupported. The font codec retains loaded font identities, mutable
parameters, and exact raw TFM bytes once per used resource; existing checked
parsing/scaling rebuilds derived metrics. Hyphenation uses logical trie nodes,
sparse operations and language-qualified exceptions, with checked tree structure,
bounded paths and operation ranges. Exception buckets are derived. Box data must
form disjoint acyclic ordinary forests; transient Free/Unset nodes are inadmissible.

Images are limited to 64 MiB and individual counts to 1,048,576. Counts must also
fit remaining section bytes and interpreter ID domains. The decoder validates
length arithmetic, command-specific operands, token/name/font references, macro
signatures/parameter references, table bounds and glue-order tags before publishing
owners. Invalid data returns Format, budget exhaustion returns Capacity, and
unsupported dependent state returns Unsupported. Loading constructs a temporary
fresh Engine and publishes it only on success; a failed load mutates no live job.

Derived font payloads and resource/scaled headers have a separate cumulative
64 MiB logical-size budget, independent of allocator rounding and store capacity.
Embedded-name and font-cache searches are bounded to 67,108,864 candidate
comparisons per image. Resource headers and name-search work are checked before
the first resource is parsed, and scaled payloads before scaling; the same budgets
apply to export. Raw TFM counts therefore cannot amplify a small record section
into unbounded metric storage or cache-search work.

## Startup and resources

`Engine::from_format` accepts image bytes and new source bytes. A resource-aware
constructor also accepts named input, a resolver and upfront extra TFMs. The image
buffer is needed only during decoding; resulting stores own all semantic bytes.

Embedded and extra TFMs register before scaled handles exist. Same normalized
names refer to exact immutable resource bytes. Registered but never loaded host
resources are not format semantics and are omitted; a new job supplies them as
extras. Every loaded font persists, even without a live selector meaning. Same
names with byte-identical contents deduplicate; differing bytes return Resource.
Embedded bytes remain authoritative at every size under the immutable-bundle
contract. Native changed-same-name-TFM lookup for a new size is not implemented.

Input, diagnostics, output, page/text auxiliaries, scratch, completion state and
session identity are fresh. Scalar mag persists, but run-local prepared_mag resets.
Hyphenation is frozen at format startup, as native dump/load finalizes its trie;
exceptions remain assignable. Calendar defaults reset after image fields to
00:00 on January 1, 1970; the host can set_calendar before execution. Saved everyjob
is queued exactly once before new input. Startup checkpoints retain that queued
source, so restoring them replays the same startup.
The first physical input line is prepared with the restored endline policy before
everyjob executes; category-code tokenization still happens on demand.

## Native CLI

`umber build-format SETUP.tex -o plain.ufmt` executes setup and writes a validated
image. Setup normally ends with `\dump`; `\input plain \dump` uses the ordinary
interpreter. `--input NAME=PATH` and `--tfm NAME=PATH` supply immutable setup assets.
The completed image is written to a new sibling temporary file and renamed only
after the full write succeeds. Failed initialization/encoding or publication
leaves an existing artifact intact. Live box registers still require the box
codec, so plain's strutbox is outside the current export subset.

`umber DOCUMENT.tex --format plain.ufmt` loads that image before reading the new
document. The original `umber DOCUMENT.tex` invocation remains available.
Extra resources are supplied before loaded font handles are constructed. Image
bytes are released before document execution. `--calendar MINUTES,DAY,MONTH,YEAR`
applies after image loading and before everyjob; without it, portable calendar
defaults are unchanged. The CLI writes diagnostics to stdout, never binary image
bytes. Invalid images or resource conflicts fail before document execution.

The document inspection/replay adapters accept the same format/calendar options.
A loaded-format READY checkpoint is the initial state before everyjob or source
execution, with no inserted diagnostic sentinel. Comparisons across source and
format startup compare page counts and positioned output; plain initialization
diagnostics deliberately do not belong to the fresh loaded job. Replay equality
within either startup path still includes complete diagnostics and retained output.
