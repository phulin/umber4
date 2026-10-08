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
`\protected`, and `\escapechar`. Generated tokens are built in reusable capture
storage. Expanded definitions and messages share an expanded-list scanner:
`\the` of a token register and `\unexpanded` append their tokens directly into
the destination, bypassing further expansion, brace accounting, and macro
parameter interpretation. Ordinary execution of these primitives installs a
source whose tokens subsequently undergo ordinary expansion. Protected macros
remain identities in expanded lists and expand in ordinary execution.

Add `\if`, `\ifcat`, `\ifodd`, `\ifcase`, `\or`, `\ifdefined`, `\ifcsname`,
and `\unless`. Character comparisons use expanded meanings, including character
aliases and suppressed active characters. Name tests do not define a previously
undefined name. Skipping recognizes every supported conditional by its current
meaning and leaves skipped macro bodies unexpanded. Case selection and boolean
selection share the existing conditional stack; their operand progress remains
in ordinary Rust calls. `\unless` reads an unexpanded boolean conditional.

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

Keep immutable token-register values in the stable token arena for this slice.
Register copies and save entries share a compact span. Arena retention makes
active sources independent of subsequent assignments. A source for a standalone
span carries its existing span and cursor, without manufacturing a macro header.
Checkpoint-aware reclamation remains with the storage deep dive.

Use the same list-scanning loop with destination-specific Rust functions:
definitions and register values append token words into their destination arena;
messages append formatted bytes into their output buffer. Raw insertions within
expanded scans use that destination directly. Avoid building an intermediate
token list just to print a message or preserve an unexpanded group.

Within an installed token window, scan consecutive ordinary characters using
Rust pointer locals and emit them directly. Braces, parameter tokens, and control
sequences return to the surrounding scanner for nesting, substitution, and
expansion. Destination emitters only append output; they preserve the installed
source and its immutable backing, including while destination storage grows.
Window exhaustion returns to the existing cursor refill and source retirement.

Expanded list scanners can emit conversion output directly into their
destination. Decimal output consists entirely of other-character tokens;
`\string`, `\meaning`, `\romannumeral`, and `\detokenize` produce other-character
and space tokens. This output needs no further expansion, group balancing, or
parameter interpretation. Read operands with the existing scanners and
expansion-depth accounting. Emit digits in reading order using bounded ordinary
Rust calls; text conversion shares its byte emitter with general expansion.
General expansion installs a token source when later input consumers need one.

Scalar tables share an array of integer values and group levels. A compact target
identifies its slot directly; assignment uses that slot for the read, save, and
write. Token registers and meanings retain their own representations. When
general expansion needs a token source, generated conversion and decimal output
append to a uniquely owned reusable buffer and publish one range at completion.
Decimal digits are generated with division by ten and reversed in place before
publication. Numbers that fit in the existing two-token inline source use it
directly. Other empty and singleton generated output stays inline in its packet.

Token-arena chunks have their fixed size in the Rust type. Per-word appending
uses a small inline gate; allocation remains in a separate growth helper. Common
control-sequence name assembly uses stack scratch space, spilling once for long
names. Fixed single-byte control names derive their spelling from their IDs and
use a static byte table. Spelling handles and hashes use arrays indexed directly
by control-sequence ID; fixed slots have empty handles. Interned identities
retain their existing session lifetime.

An integer scan can read its first literal decimal digit directly from the
loaded file line using the current category table. The cursor consumes that
character and enters the middle-of-line state. The existing suffix scanner then
reads further digits and the expanded terminator; signs, other radices, and
expandable operands retain the general scanner.

Integer entry uses a small Rust function to select the literal decimal suffix
scanner or the general recursive scanner for signs, other radices, and internal
quantities. Literal operands avoid setting up the general scanner's saved
registers and temporary reads. Both scanners continue on the Rust stack.

Expanded definitions and token-register replacements still accumulate arena
storage within a run. Benchmarks expose this cost through definition churn and
larger working sets, with bounded total materialized tokens. The first MVP's
constant-storage loops remain a separate regression set.

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
