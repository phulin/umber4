# Character input and scanners

This proposal supplies the file side of [expansion.md](expansion.md). Each file
token takes the character tables in force when it is consumed. The reader lexes
ahead within the current line and discards that lookahead when those tables
change (section 3). Stored token lists already have frozen category codes and use
the shared sequence cursor instead.

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

An escape scans a control word using the current letter category, or one
nonletter for a control symbol, then interns its byte spelling. A control word
switches to skipping blanks. A control symbol takes TeX's category-dependent state
transition; control space uses the special space handling. Active characters
produce active-character IDs rather than ordinary character tokens.

The `^^` translation is part of character scanning, before the resulting
character is finally categorized. It can alter control-sequence spelling or
produce a character with a different category. Use a small explicit scanner with
the upstream ordering and reclassification behavior. This is a good place to
share tiny helpers between normal character input and control-word scanning.
Translation never runs ahead of execution: line lookahead stops before a `^^`
pair it would translate. Control-word lookahead can reduce a `^^`
sequence in the first nonletter after the name. That reduction is committed to
the prepared line before the control sequence executes. Keep such rewrites in
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

Producing one token per reader call gives that directly, but every token then pays
a full call, dispatch, and return around a few category loads. On a line of
integer assignments and conditionals, replaying the same tokens from a stored
body took a third of the time of reading them from the file that way. The reader
therefore lexes ahead within the current prepared line:

- When its lookahead is exhausted, the reader loads the next line if the current
  one is consumed, then lexes the rest of the current line into a reusable token
  buffer. Before each buffered token it records the line position and lexical
  state from which that token was lexed, and it records the same pair after the
  last token.
- Consumers read the buffer as an ordinary token window. Character runs, argument
  scans, digit runs, and conditional skipping share the window paths, so file
  input needs no separate byte-level fast paths.
- Lookahead stops before a token whose effect must follow the execution of
  earlier tokens. That token, and only that token, is read on the committed path
  described in section 2 before lookahead resumes. The stops are:
  - a `^^` pair that would be translated, including in control-word lookahead,
    since translation rewrites the prepared line;
  - an invalid character, whose error follows the execution of earlier tokens;
  - a control word whose name is not yet interned, so names enter the table in
    consumption order and a capacity failure occurs at the same token;
  - the end of the prepared line. The next line is loaded, applying the
    then-current `\endlinechar`, only when a token is needed after the last one.

Lookahead is exact. A token depends only on the prepared line, the position,
the lexical state, and the category table. Lookahead never rewrites the line,
inserts a name, or loads a line, so while the table is unchanged each buffered
token equals the one a lazy reader would produce when it is consumed. Any write
to a category code, including a group-end restoration, discards the unconsumed
lookahead and restores the position and state recorded before its first token.
Reading then continues lazily under the new table. Blanks, ignored characters, and
comments lexed after the last buffered token are reconsumed the same way. For
example, `\catcode37=12 %` reads the `%` as an other character after the
assignment executes, although lookahead first passed it as a comment.

The rewind is the only invalidation. Ordinary reads never compare an epoch or
recheck the table; a category write is rare and pays for it by relexing at most
the rest of the current line. Tokens already
returned are unaffected, as are tokens put back into input, which keep their
identities as in TeX.

## 4. Control-sequence lookup

The interner stores compact names once and maps their byte spelling to stable
IDs. Control-word scanning can hash as it reads and look up the resulting name
without allocating a temporary string in the common case. Line lookahead only
looks names up; insertion happens on the committed path. A plain contiguous
name can be a borrowed range in the line; a translated spelling uses a reusable
small scratch buffer. A new spelling is written into process-owned name storage
only when lookup misses.

The MVP uses its own packed name table for probing and growth. Entries retain a
hash fragment and stable name ID; spelling bytes remain in the session arena.

Single-character control sequences and active-character identities can use
preassigned dense IDs. The empty control-sequence name has its own identity.
Named identities and current meanings remain separate; name lookup does not make
an undefined name into `\relax` unless the operation's semantics require that
assignment, as `\csname` does in the relevant profile.

The spelling stored by the interner is the interpreter's byte spelling. Printable
escaping and `\escapechar` belong to conversion and diagnostic formatting, not
to the identity. Rollback retains names discovered later in the document.

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
state and can be copied at a paragraph checkpoint.

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

A file snapshot copies cursor, lexical state, and EOF flags and retains the exact current
prepared line and resource version. Its position and state are those recorded
before the first unconsumed lookahead token; the lookahead buffer is derived and
is rebuilt after restore. Usually only a few lines are active; an
immutable owned line is a simple initial representation. Live readers can reuse
unique line-buffer capacity, while a retained line is replaced when reading the
next one. Resource bytes remain shared.

Examples that distinguish the design include a category-code change on the same
line, `^^` inside a control word, a trailing comment, altered `\endlinechar`,
active characters, `\read` with unmatched braces, and `\everyeof` followed by
resumption. Compare token identities, consumed positions, and diagnostics with
upstream before tuning the common scanning path.
