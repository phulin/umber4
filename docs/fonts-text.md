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
immutable cache entries. They do not contain live TeX font descriptors. There is
no allocation or reference counting per emitted character.

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

## Horizontal construction and handoff

The document contains an external vertical list and a horizontal list, mode,
space factor, previous depth, and previous paragraph line count. Startup uses
INITEX values: nullfont, zero dimensions/glue, character space-factor codes 1000
except uppercase ASCII 999, space factor 1000, and ignored previous depth.
Paragraph entry appends paragraph skip, resets line count, enters horizontal mode,
and optionally adds an empty indentation box. Indentation in horizontal mode
resets space factor; noindent there does nothing. Ordinary brace groups do not
terminate a paragraph.

Character runs use the selected font, look ahead through expansion and `\\char`,
and preserve a noncharacter lookahead in pending input without executing it.
All TFM ligature operations retain original components and boundary-hit flags.
Boundary programs, false boundary characters, and `\\noboundary` follow upstream.
TFM kerns remain distinct from explicit kerns. An explicit hyphen-character
component adds an empty discretionary with zero replacement count in unrestricted
horizontal mode. Pattern hyphenation is absent.

Spaces freeze inline glue at execution, using current font parameters, spaceskip,
xspaceskip, and exact space-factor arithmetic. Control space uses the normal
space path without changing the current factor. Font mutations affect subsequent
spaces only. Missing glyphs create no character node, as in TeX; tracing lost
characters is outside this stage's diagnostic profile.

A nonempty paragraph remains owned by the horizontal root until the paragraph
consumer takes it. This stage returns an explicit unsupported error on paragraph
completion requiring line breaking; it never silently discards or packs text into
an invented natural line. Internal builder tests examine live horizontal lists.
Empty noindent paragraphs return to vertical mode. Later paragraph integration
replaces this single completion hook and directly transfers the list into line
boxes on the vertical list. No intermediate result queue is introduced.

Only successful external vertical driver boundaries are checkpointable. A work
budget defers handoff while horizontal construction continues. Font tables,
current selector, text state and lists are captured with their matching node-store
version. Errors require restore before execution continues.

Restricted boxes, general discretionary syntax, accents, math, alignments,
automatic hyphenation, pdfTeX font extensions, arbitrary I/O and output backends
remain unsupported. Tests explicitly set document dimensions and spacing; no
plain-format defaults are assumed.
