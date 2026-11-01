# Character input and scanners

This document supplies the file side of [expansion.md](expansion.md). Each file
token takes the character tables in force when it is consumed. The reader prepares
lines lazily and retains the current line and lexical cursor; it does not buffer
unconsumed tokens (section 3). Stored token lists already have frozen category
codes and use the shared sequence cursor instead.

## Executable immutable input

`Engine::with_inputs` accepts the main filename, borrowed main bytes, and an
immutable borrowed `InputResolver`. Its synchronous `resolve` receives the exact
scanned byte name and the active file's resolved name, returning an `InputFile`
with stable borrowed name/bytes or `Resource`. The host owns search policy and
default extensions; the core does not require UTF-8, open paths, or prompt.
`Engine::new` remains the unnamed single-source adapter. The resolver and all
source bytes remain unchanged and borrowed for the engine/checkpoint lifetime.

`\input` expands filename tokens, accepting ordinary, quoted, and balanced
braced names. Filename scanning discards initial spaces/relaxes, preserves byte
names and noncharacter terminators, and guards recursive unbraced filename
scanning. A child suspends the parent cursor and pending token input below a
file-boundary marker. Its first line is prepared when opening, including for an
empty child. All prepared lines use one session typed byte store.
Filename quoting, braced expansion, and leading-relax handling follow the
[web2c input changes](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.ch).

`\endinput` sets a global pending force-EOF flag. The next line request closes
the then-active file; opening a child first still prepares and reads its first
line. Closure clears the flag and restores parent input before scanner validity
is checked. Non-normal scanner EOF returns `Eof`; normal EOF resumes the parent.
Ordinary groups and conditionals may span files. Exhausting the unnamed/main
source without `\end` remains an explicit `Eof` failure without terminal input.

Snapshots copy active/suspended cursors, exact prepared lines and lexical state,
file markers, and pending force-EOF. Temporary write cleanup restores suspended
files when discarding markers, without treating cleanup as semantic EOF or
publishing a partial write. Literal and lookup readers use the active file
directly when its marker is on top. This slice excludes `\everyeof`, pseudo-files,
read/write streams, mutable/aliased files, source edits, and resource-request replay.

## 1. Immutable bytes, mutable cursor

An input source over stable external bytes retains an immutable resource view
and a small file cursor:
raw byte offset, line number, current logical line, character position, and TeX's
lexical state and EOF phase. A file cursor also records its source name and role
for diagnostics and EOF behavior. Nested `\input` suspends the parent cursor;
its bytes and current
line remain available until that source is exhausted or released.
Generated-file input, including an external file later aliased by `\openout`,
uses the live file/read-ahead contract in [resources.md](resources.md). It still
supplies prepared lines to the same character reader.

The initial compatibility profile is pdfTeX's byte-oriented input, not automatic
Unicode normalization. Character-code mapping belongs to the profile and host
adapter. Source names and bytes have separate identities: two versions of the
same logical filename are different immutable resources. Resource acquisition
and checkpoint rebinding are developed in [resources.md](resources.md) and
[checkpoints.md](checkpoints.md).

Read one physical line when TeX needs it. The line loader recognizes the adapter's
line endings, removes trailing blanks according to TeX's input convention, and
appends the then-current valid `\endlinechar`. It writes the normalized line into
its reusable destination buffer, or retains an existing byte range when no
transformation is needed. Keep that prepared line separate from the resource's
raw bytes: changing `\endlinechar` does not rebuild a line already loaded.
Opening an empty `\input` source prepares one blank first line, as TeX does.

## 2. The lexical state machine

TeX's three lexical states are semantic input state: beginning of line, middle of
line, and skipping blanks. They fit in the file cursor. They are not resumable
scanner frames.

Read the next character, consult its current category code, and apply the state
transition. Spaces and end-of-line characters can produce a space token, disappear,
or produce the control-sequence identity `\par`. A comment discards the rest of
the prepared line. An ignored character produces no token. An invalid character
takes the compatibility error path before reading continues.

An ordinary token read scans an escaped control word using the current letter
category, or one nonletter for a control symbol, then interns its byte spelling. A control word
switches to skipping blanks. A control symbol takes TeX's category-dependent state
transition; control space uses the special space handling. Active characters
produce active-character IDs rather than ordinary character tokens.

The `^^` translation is part of character scanning, before the resulting
character is finally categorized. It can alter control-sequence spelling or
produce a character with a different category. Use a small explicit scanner with
the upstream ordering and reclassification behavior. This is a good place to
share tiny helpers between normal character input and control-word scanning.
The lazy reader translates characters as it scans a token. Scanning the end of a
control word can reduce a `^^` sequence in the first nonletter after the name.
That reduction is committed to the prepared line before the control sequence
executes. Keep such rewrites in
the line's owned buffer, making a shared line writable on this rare path; retain
the rewritten line and adjusted position at checkpoints. For example,
`\def\m{\catcode94=12\relax}\m^^2b` leaves a pending `+`, even though `\m`
changes the category of the original carets. Ordinary character translation can
use local decoded values where it consumes the translated characters immediately.

These choices follow `input_ln` and `get_next` in the
[TeX source](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web).
The pdfTeX-specific translations and extended primitives come from the
[pdfTeX source](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/pdftexdir/pdftex.web).

## 3. Category changes apply immediately

Keep character tables in directly indexed arrays. A token is classified with the
tables in force when it is consumed, so a category-code assignment can change a
later character on the same already-loaded line.

The file reader classifies each consumed character directly from the current
table. It has no token lookahead buffer, category epoch, or category-change
rewind. Category writes and group restoration therefore affect the next file
character without invalidation. Tokens already returned or put back into input
retain their identities as in TeX.

Narrow operations share this cursor with the general reader. `read_character`
consumes one byte only when its current category is 11 or 12, sets the lexical
state to middle of line, and returns its character token. It examines only the
already-prepared line. At any other category or at line exhaustion, it returns
without consuming input; the general reader remains responsible for translation,
control sequences, lexical transitions, line loading, and errors. Digit and
character-run scanners use similarly bounded operations while scanning operands
or constructing token lists.

Horizontal character construction uses `read_character` only when the token-source
stack is empty. Pending and backed tokens, macro bodies, and arguments keep their
normal source priority, substitution, and ownership through the general reader.
A reported literal byte enters the same space-factor, missing-character, and
ligature processing as a character obtained through expansion. A miss leaves
fallback input unread and uses the ordinary expansion and character-command path.

[fast-lexing.md](fast-lexing.md) describes a separate whole-file lexer prototype
under a fixed table and proposals for speculative token lookahead. The engine
does not use that prototype.

## 4. Control-sequence lookup

The interner stores compact names once and maps their byte spelling to stable
IDs. Control-word scanning can hash as it reads and look up the resulting name
without allocating a temporary string. The spelling is a contiguous range in the
prepared line, including any committed `^^` rewrites. Lookup and insertion happen
when the control word is read. A new spelling is written into session-owned name
storage only when lookup misses.

The MVP uses its own packed name table for probing and growth. Entries retain a
hash fragment and stable name ID; spelling bytes remain in the session arena.

Skipped conditional text and the unexpanded operands of `\ifdefined` and `\ifx`
need only current meanings. Their file reads share the same lexical cursor and
hash probe but do not insert missing control words or grow binding storage.
An absent spelling has undefined meaning. Already stored tokens retain their
original identities, including distinct undefined names in macro text, and their
ordinary one-read suppression behavior. Normal token reads still intern names;
`\csname` also creates an identity and defines it as `\relax` when required.

Single-character control sequences and active-character identities can use
preassigned dense IDs. The empty control-sequence name has its own identity.
Named identities and current meanings remain separate; name lookup does not make
an undefined name into `\relax` unless the operation's semantics require that
assignment, as `\csname` does in the relevant profile.

The spelling stored by the interner is the interpreter's byte spelling. Printable
escaping and `\escapechar` belong to conversion and diagnostic formatting, not
to the identity. Rollback retains names interned later in the document.

## 5. Scanner functions

Numeric, filename, keyword, and token-list scanners are ordinary functions
consuming the shared reader. A scanner selects expanded or unexpanded reads at
the points its syntax requires. It retains its partial number, brace depth,
keyword progress, or filename pieces in locals and operation scratch.

Numeric scanners preserve distinctions between character identity, current
meaning, and the one-read effective command. Signs, radix prefixes, alphabetic
constants, internal quantities, `true` units, decimal fractions, and glue orders
follow TeX's exact routines. Arithmetic uses the integer helpers described in
[execution.md](execution.md), including overflow recovery and rounding.

Filename scanning builds the name directly into the resource-request destination
or reusable name buffer. Its termination, quoting, and expansion rules follow
the selected pdfTeX profile; the host resolves that completed logical request.
It does not borrow a transient current-line pointer after reading another line.

Token-list scanners construct immutable sequences in their destination store.
Existing token ranges can be retained; newly read file tokens are written there.
Expanded definition and conversion scans reuse the expansion loop and preserve
the special treatment of protected macros and unexpanded token results.

The reader's scanner context records only the information needed for TeX's outer
validity, alignment, and recovery checks. Nested scanner calls save and restore
it through ordinary Rust scope. A failed scanner returns or takes TeX's recovery
branch; it never yields a partially scanned construct to the host.

## 6. EOF and special input

Exhausting a token sequence pops that input source. Exhausting a file follows
TeX's file-ending behavior, including outer validity, `\endinput`, and e-TeX's
`\everyeof` insertion. EOF handling can install an immutable token sequence and
later resume closing the file. Those pending source cursors are semantic input
state and can be copied at a quiescent checkpoint.

Retain the global pending `\endinput` force-EOF flag and each file's EOF-hook phase.
`\def\stop{\endinput A\par}\stop` can reach a checkpoint before the forced
file close, and `\everyeof{A\par}` can reach one while its hook is still active.
Resume must preserve both, so neither later lines nor a second EOF hook appear.

`\read` and `\readline` consume their selected stream using their primitive's
line and tokenization rules. They are not aliases for nested `\input`: `\read`
can collect a balanced token list across lines, and `\readline` produces the
e-TeX category treatment. Their results are built directly in durable token
storage. Stream position and open/just-open/normal/closed state belong to the
document. Reaching the byte offset at EOF alone does not make `\ifeof` true;
TeX's failed-read transition changes that state, including its synthetic final
empty read line.

`\scantokens` detokenizes a captured sequence directly into an owned pseudo-file
byte buffer. It splits using the operation's `\newlinechar` and then uses normal
lazy line preparation and current category codes. A pseudo-file cursor has its
own role and EOF behavior but reuses the file reader. Its byte owner remains
available to pending input and checkpoints; no intermediate flat token stream
or second copy into linked pseudo-lines is needed.

The native batch adapter supplies configured answers or ordinary TeX errors for
terminal interaction. A browser adapter can supply input ahead of time or request
it by unwinding and restarting from a checkpoint, using the same resource-request
path. The core has no interactive scanner continuation.

The e-TeX additions are defined in the
[e-TeX change file](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/etexdir/etex.ch).

## 7. Checkpoint representation and useful comparisons

A file snapshot copies cursor, lexical state, and EOF flags and retains the exact
current prepared line and resource version. Its position and lexical state identify the
next unread character directly; there is no token lookahead buffer to retain or
rebuild. Usually only a few lines are active; retain them as pooled immutable
lines. Live readers reuse unique line-buffer capacity, while a retained line is
replaced when reading the next one. Resource bytes remain shared.

Examples that distinguish the design include a category-code change on the same
line, `^^` inside a control word, a trailing comment, altered `\endlinechar`,
active characters, `\read` with unmatched braces, and `\everyeof` followed by
resumption. Compare token identities, consumed positions, and diagnostics with
upstream before tuning the common scanning path.
