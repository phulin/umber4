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

The foundation admits canonical nullfont state, empty hyphenation data, and void
box registers. Nonempty dependent state returns Unsupported; it is never reset
or silently dropped. Reserved subsystem sections allow complete codecs for fonts
and TFM resources, hyphenation patterns/exceptions, and live box-register forests.

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

Dependent sections use tag 0 for their supported empty/canonical state. Tag 1 is
reserved for a complete codec and returns Unsupported until implemented. The
font codec retains loaded font identities, mutable parameters, and exact raw TFM
bytes once per used resource; existing checked parsing/scaling can rebuild derived
metrics. Hyphenation uses logical trie operations and exceptions. Box data must
form disjoint acyclic ordinary forests; transient Free/Unset nodes are inadmissible.

Images are limited to 64 MiB and individual counts to 1,048,576. Counts must also
fit remaining section bytes and interpreter ID domains. The decoder validates
length arithmetic, command-specific operands, token/name/font references, macro
signatures/parameter references, table bounds and glue-order tags before publishing
owners. Invalid data returns Format, budget exhaustion returns Capacity, and
unsupported dependent state returns Unsupported. Loading constructs a temporary
fresh Engine and publishes it only on success; a failed load mutates no live job.

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

The CLI build/load workflow and complete dependent codecs are outside the
foundation API. Their integration must compare freshly loaded plain against
source initialization and native output, including unchanged Gentle.
