# Executable font and horizontal-text contract

This stage supplies TeX82 metric loading and horizontal list construction. The
paragraph and page algorithms remain separate consumers of these owned lists.
The compatibility reference is `read_font_info`, `new_font`, `find_font_dimen`,
`new_graf`, and the character loop in the retained upstream
[TeX source](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web).

## Resources and font identities

The host registers named immutable TFM byte resources before execution. The first
nonzero driver advance seals registration permanently, including after restore.
Registration rejects duplicate names and malformed bytes. Names are exact byte
strings; an optional terminal `.tfm` is normalized at registration and lookup.
The portable engine never opens a path. Missing resources and unsupported syntax
are explicit errors. Host bytes are copied once at this resource boundary.

Session-owned typed backing pools retain resource descriptors, character records,
program instructions, and scaled metric arrays. Parsed and scaled metrics are
immutable cache entries. Safe handles retain scaled-header chunks and the sealed
resource-set owner for [detached page output](output.md); source/header/payload
storage can therefore outlive the session. They do not contain live TeX font descriptors. There is
no allocation or reference counting per emitted character.

Each scaled font stores one dense width/height/depth/italic record per character
in its TFM range, at most 4 KiB of initialized metric records. These replace the
compressed scaled dimension tables: character lookup reads the immutable record
directly, without decoding TFM indices or resolving the source resource. A
separate presence bitmap preserves the nonzero-width-index existence rule,
including present zero-width characters and absent characters whose other metric
fields remain observable through `\fontcharht`, `\fontchardp`, and `\fontcharic`.
Scaled kerns and baseline parameters occupy separate pooled arrays. Engine and
detached-result access share the same immutable metric lookup.

Paragraph measurement and horizontal packing borrow font tables and the scaled
catalog immutably for each traversal. A local reader retains the last font ID
and resolved descriptor; resolution remains lazy in node visit order, and every
glyph lookup checks character existence. The reader adds no checkpoint state.

Document state owns loaded-font order and IDs, source resource names, sizes,
mutable font parameters, hyphen/skew characters, and font identifier names.
Selectors and current-font bindings obey ordinary assignment grouping. Font
parameter and font-integer mutations are global even inside groups; checkpoints
restore them. Loaded-font reuse searches document descriptors by resource name
and resolved size. Cached baselines never resurrect abandoned mutations or font
IDs after rollback. Parameter extension is allowed only for the last loaded
font, including reads, following upstream `find_font_dimen`.

TFM loading validates size fields, character indices, next-larger cycles,
ligature/restart/kern references, and extensible recipes. Extra trailing bytes
are permitted, as in TeX. Character existence means a nonzero width index;
a present character can still have zero width. Scaling uses TeX's staged integer
algorithm, including large-size reduction and negative metrics. Slant is a signed
unscaled fixed-point number. Fonts have at least seven parameter slots.

Persistent character metadata keeps the six decoded TFM byte fields in ordinary
`u8` fields: width index, height index, depth index, italic index, tag, and
remainder. These are relative indices, with maxima 255, 15, 15, 63, 3, and 255;
table offsets and combined metric indices use host-sized arithmetic. Registration
validates the widened fields against the original table lengths before indexing.

Each immutable TFM resource holds one 256-entry table of 16-bit ligature/kern
program starts, shared by all its scaled fonts. Registration prepares the table
after validation, following at most one initial restart for each tagged character,
including absent characters. Boundary program starts bypass this preparation;
the runtime program search and retained raw TFM bytes remain unchanged.

## Horizontal construction and handoff

The document contains an external vertical list and a horizontal list, mode,
space factor, previous depth, and previous paragraph line count. Startup uses
INITEX values: nullfont, zero dimensions/glue, character space-factor codes 1000
except uppercase ASCII 999, space factor 1000, and ignored previous depth. The
nullfont has hyphen/skew characters 45 and -1; the separate default-hyphen/default-
skew integer parameters start at zero, as in INITEX.
Paragraph entry appends paragraph skip, resets line count, enters horizontal mode,
and optionally adds an empty indentation box before inserting `\everypar`.
Indentation in horizontal mode resets space factor; noindent there does nothing. Ordinary brace groups do not
terminate a paragraph.

Character runs use the selected font, look ahead through expansion and `\char`,
and preserve a noncharacter lookahead in pending input without executing it.
Each non-nullfont run retains one immutable font-resource handle and borrows its
scaled descriptor and source TFM for existence, ligature-program, boundary, and
kern access across input expansion. Mutable font parameters remain in document
state; an unexpandable assignment ends the run before execution.
All TFM ligature operations retain original components and boundary-hit flags.
Boundary programs, false boundary characters, and `\noboundary` follow upstream.
TFM kerns remain distinct from explicit kerns. An explicit hyphen-character
component adds an empty discretionary with zero replacement count in unrestricted
horizontal mode. Pattern hyphenation is absent.

Spaces freeze inline glue at execution, using current font parameters, spaceskip,
xspaceskip, and exact space-factor arithmetic. Control space uses the normal
space path without changing the current factor. Font mutations affect subsequent
spaces only. Missing glyphs create no character node, as in TeX; tracing lost
characters is outside this stage's diagnostic profile.

A nonempty paragraph remains owned by the horizontal root until the
[paragraph consumer](paragraphs.md) takes it. Paragraph completion transfers
selected ranges into packed line boxes on the vertical root. Internal builder
tests can still inspect unfinished horizontal lists. Empty noindent paragraphs
return to vertical mode. No intermediate result queue is introduced.

Only successful external vertical driver boundaries are checkpointable. A work
budget defers handoff while horizontal construction continues. Font tables,
current selector, text state and lists are captured with their matching node-store
version. Errors require restore before execution continues.

Internal font-token conversion (`\the\font`), restricted boxes, general
discretionary syntax, accents, math, alignments,
automatic hyphenation, pdfTeX font extensions, arbitrary I/O and output backends
remain unsupported. Tests explicitly set document dimensions and spacing; no
plain-format defaults are assumed.

## Validation and native adapter

Ordinary tests construct small controlled valid TFM resources and malformed
variants entirely in memory. They cover metric rounding, signed slant, missing
characters, extensible recipes, ligature component ownership and all operation
forms, real/false boundary characters, explicit hyphens, spaces, group restoration,
new/reused font descriptors after rollback, and paragraph-entry ordering.

The ignored `upstream_font_metrics_and_horizontal_nodes` test is an explicit
native adapter. Supply `UMBER_PDFTEX` as an absolute executable path and
`UMBER_TFM_DIR` as the corpus `fonts/tfm/public` directory, with that executable's
library search path when required, then run:

```bash
cargo test --lib upstream_font_metrics_and_horizontal_nodes -- --ignored
```

The adapter writes only into a temporary directory and removes its files. It
compares 380 marked metric/parameter messages over five sizes and 20 horizontal
node lists against pdfTeX 1.40.25 (TeX Live 2023/Debian), using INITEX with e-TeX.
The e-TeX character-metric primitives expose indexed height/depth/italic fields
even when the width index marks a character absent; emitted glyphs still require
existence. Native list comparisons use restricted hboxes, where explicit-hyphen
discretionaries are suppressed; separate tests check their unrestricted creation.
No timing measurements are part of this stage.

The real TFM inputs are retained in the local lexing corpus, rather than vendored
or required by normal tests:

| Corpus TFM | SHA-256 |
| ---------- | ------- |
| `ae/aer10.tfm` | `49b937ceb0598bab7dba22fcf3544dd909abb92513bb00399d407dd15c17da64` |
| `latex-fonts/lcmss8.tfm` | `84922bb747d57f8c2c60f34b31c968f6f99febe55ce67d6e2fa2c4417a842d0d` |
| `latex-fonts/icmtt8.tfm` | `f2519b7344c1718113523711494d52dbdf94599ec1c81fd5b588fa2392896a24` |
