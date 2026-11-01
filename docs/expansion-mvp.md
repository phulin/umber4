# Broader parsing and expansion slice

This extends the [first MVP](mvp.md) with token-list values, conversions,
conditionals, and deferred tokens. Execution remains limited to macro and scalar
state. Text is consumed inside token-list scans and messages; typesetting stays
with the later node, paragraph, and output work.

The implementation registers 63 primitive names. The original eight workloads
remain a regression set alongside the broader [macro benchmarks](../benchmarks/expansion.py).

## Primitive contracts

Add `\toks`, `\toksdef`, `\chardef`, `\futurelet`, `\afterassignment`,
`\aftergroup`, and `\globaldefs`. Token-register assignment reads a balanced
unexpanded list or shares another register's immutable value. An installed source
continues reading its original value after the register is overwritten. Local
saves retain that value; global writes supersede earlier saves.

Add `\string`, `\meaning`, `\romannumeral`, `\detokenize`, `\unexpanded`,
`\protected`, and `\escapechar`. Generated tokens are built in reusable token
chunks. Expanded definitions and messages share an expanded-list scanner:
`\the` of a token register and `\unexpanded` append their tokens directly into
the destination, bypassing further expansion, brace accounting, and macro
parameter interpretation. Ordinary execution of these primitives installs a
source whose tokens subsequently undergo ordinary expansion. Protected macros
remain identities in expanded lists and expand in ordinary execution.

Add `\if`, `\ifcat`, `\ifodd`, `\ifcase`, `\or`, `\ifdefined`, `\ifcsname`,
and `\unless`. Character comparisons use expanded meanings, including character
aliases and suppressed active characters. `\ifcsname` expands its character input
and performs a non-inserting lookup: a missing name acquires neither a symbol ID
nor retained spelling bytes. Expansion while building that name preserves its
ordinary side effects, including creation by nested `\csname`. Name tests do not
define a previously undefined name. This follows the [e-TeX conditional lookup contract](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/etexdir/etex.ch#L4616).
Skipping recognizes every supported conditional by its current
meaning and leaves skipped macro bodies unexpanded. Case selection and boolean
selection share the existing conditional stack; their operand progress remains
in ordinary Rust calls. `\unless` reads an unexpanded boolean conditional.

Conditional skipping advances over ordinary character runs without constructing
resolved reads. Control-sequence identities and internal parameter markers
return to the ordinary reader for aliases and substitution. File runs use the
current category table and stop before escapes, active characters, translations,
spaces, and line transitions; skipped text still leaves the correct lexer state.

Add `\lccode`, `\uccode`, `\lowercase`, and `\uppercase`. Case changes scan
unexpanded balanced lists, preserve categories, transform active-character
identities, and retain other control-sequence identities. Initialize the ASCII
case tables as INITEX does. Add the assignable integer parameters `\time`,
`\day`, `\month`, and `\year`. The portable library accepts host clock values
through `Engine::set_calendar`; the MVP library and CLI otherwise use 00:00 on
January 1, 1970. Fixtures and benchmarks assign them explicitly.

Add integer `\numexpr` with parentheses, signs, precedence, checked arithmetic,
rounded division, and multiplication/division scaling. Expression evaluation
uses Rust locals and calls, with a depth limit. A final effective `\relax` is
consumed; other terminating tokens remain available for the surrounding scan.
Arithmetic errors remain a local flag until the complete expression has been
scanned, preserving e-TeX's ordering of scan errors and arithmetic errors.
Dimensions and glue expressions remain part of later scalar execution work.

`\futurelet` snapshots the second token's meaning and reinserts both original
token identities. The pending `\afterassignment` token runs after the complete
assignment and above that continuation. `\aftergroup` tokens run in encounter
order after restoration. These are pending token sources and save entries, not
execution continuations.

## Storage and control flow

Primitive registration, meaning serialization, and assignment/conditional
classification share one command inventory. Expansion, conversion, and expression
scanning use ordinary Rust calls. Generated output installs an owned token source
only when a later reader needs it; expanded-list scanners can emit conversion
results directly into their final destination.

Definitions and token-register values retain immutable token storage. Aliases,
save records, installed input, and captured ranges preserve that storage through
replacement. All list sources use the same cursor/window traversal. Argument
cursors borrow their body packet while it remains beneath them; forwarded ranges
acquire their own backing owner before the enclosing source can retire.

Parameters and delimiter fallback tables are definition-owned metadata. The
validated zero-to-nine arity stays with the hot invocation fields, so packet
selection does not reread a descriptor length to recover arity or revalidate the
parameter limit. Packets reuse argument and span capacity by arity. Empty/singleton
output stays inline. The bounded
allocation revisions in [mvp.md](mvp.md) separate durable and transient active token
blocks and cache complete block ownership records. Value slots reuse cleared
parameter/fallback and multi-piece descriptor capacities in place. Headers share
stable slabs and variable metadata shares typed backing blocks. Both classes use
the same cursor API.

The interner stores stable spelling bytes in session-owned chunks, with a compact
probe table and dense spelling directory. Source and dynamically built names use
the same identity lookup. The engine session keeps this interner across checkpoint restores and releases
it at session teardown.

Scalar tables retain integer values and assignment levels. Token registers own
values with their group levels; temporary internal quantities retain only the
value. Group restoration and local/global ownership behavior follow
[execution.md](execution.md). Scanner fast paths must preserve the same token
consumption and errors as the complete reader.

Measured expansion comparisons are retained in the
[benchmark report](../benchmarks/results/expansion.md). Common validation and timing
methodology lives in [performance.md](performance.md).

## Corpus and measurements

Use Michael Doob's [gentle.tex](https://ctan.net/info/gentle/gentle.tex)
as a source of genuine macro kernels. The full document needs fonts, dimensions,
boxes, mathematics, and page construction; this benchmark measures identified
excerpts and adaptations:

- Spelling switches and aliases, plus the two-argument `\talks` text template.
- The original calendar formatter with all twelve month branches.
- Contents-entry serialization, moving its token-building expression from a
  write into an expanded definition; its layout control names are stringified.
- Paragraph-delimited records using the document's argument signature, with an
  explicitly adapted consumer retaining and expanding both payloads. Measure
  source input and a stored variant that alternates between macro bodies and
  forwarded captured arguments; the consumers and expected text are identical.

Also exercise token-register pipelines, a larger dynamically populated name
table, and an optional-argument parser using `\futurelet`, `\unless`, and integer
expressions. Use varied branch choices, names, argument sizes, and nested field macros.
Consume payloads, compare expanded definitions with independently generated
expected text, and emit a final checksum. Equality validation contributes to
timing. Record original corpus hashes, selected line ranges, and adaptations.

Numbered references form control names from decimal register values inside
`\ifcsname` and `\csname`, then append decimal text to the expanded definition.
Use a fixed set of 256 defined references with positive and negative operands
from three to ten digits. This exercises generated numeric input consumed by a
name scanner alongside direct numeric emission into a destination. Generate the
expected reference text independently and compare every result with `\ifx`.

A supplemental register-meaning workload rotates count and token-register alias
indices through 0–255, with normal, changed, and disabled escape characters.
Serialize each pair through `\meaning` into an expanded definition. Compare its
tokens against a separately generated literal built with `\string` and decimal
source digits. Keep this focused conversion case separate from the default nine
workloads and compiler training so their input identities remain comparable.

Use identical complete sources with native pdfTeX INITEX. Retain raw samples at
several sizes, warmup and process overhead, result validation, allocation counts,
and native/Wasm build identities. Wasm runtime timing remains separate. Preserve
the original eight workloads to detect regressions as the command set grows.

The core suite also reads one- to ten-digit decimal fields directly from source
bytes, cycling through six values up to the maximum TeX integer. Each field is
compared with its independently stored macro operand before incrementing the
checksum. This complements the original integer workload, whose repeated
operands come from macro tokens.

Compatibility references are upstream
[tex.web](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web)
and [etex.ch](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/etexdir/etex.ch),
especially expanded list scanning, conversions, conditionals, shorthand
definitions, save-stack insertion, and integer expressions.
