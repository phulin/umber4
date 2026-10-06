# Input and expansion loop

This design develops the execution path in [architecture.md](architecture.md).
Expansion is driven by ordinary Rust calls. A compact input stack holds the
token sources that TeX is consuming, while scanner progress and primitive
execution remain in local variables and control flow. Checkpoints occur at the
end of a paragraph with no open groups.

Build new data in its destination storage and reuse existing token ranges where
possible. Argument capture, definition construction, and primitive results should
each avoid an intermediate flattened list followed by a copy into final storage.

## 1. The execution path

Use two central operations:

- `next_token()` reads one token and resolves its current meaning, without
  expanding it. Argument matching and other raw-token scanners use this path.
- `next_expanded()` repeatedly reads and expands until it obtains an unexpandable
  command for the caller to consume. Ordinary execution and numerical scanners
  use this path.

Their proposed shape is:

```rust
fn next_expanded(&mut self) -> Result<ReadToken> {
    loop {
        let read = self.next_token()?;
        match read.meaning {
            Meaning::Macro(definition) => self.begin_macro(definition)?,
            Meaning::Expandable(primitive) => self.expand_primitive(primitive)?,
            Meaning::Undefined => self.undefined_control_sequence(read.token)?,
            _ => return Ok(read),
        }
    }
}
```

This is an outline, not an API commitment. `ReadToken` is a small local value
containing token identity and its effective meaning for this read. The caller
can immediately dispatch that meaning. Stored replacement lists and arguments
contain compact token words.

`begin_macro` matches arguments, installs the replacement-list cursor, and
returns. The loop then consumes that source. A normal chain of macro calls
therefore iterates through the loop. Primitive expansion can call scanners and
other expansion functions directly where its operation requires them; those
calls return normally. Expansion-order primitives can use an `expand_once` helper
that dispatches an already-read token through the same macro and primitive
functions. `next_expanded` continues reading afterward, while `expand_once`
returns after the selected token's expansion.

Execution consumes each returned command before asking for the next one. This
preserves the ordering between assignments and later expansion. For example:

```tex
\def\m{\def\m{new}old}
\m \m
```

The first invocation executes the definition and continues reading `old` from
its original body. The next invocation uses the new body. Session-owned immutable
definitions make both reads straightforward.

This follows the useful division between token reading, macro invocation, and
expansion in TeX's `get_next`, `macro_call`, and `get_x_token`, with Rust functions
and contiguous token storage replacing the original implementation choices.
See [TeX's input and expansion code](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web).

## 2. Token identity and current meaning

A character token carries its character and category code, which directly
determine its command. A control-sequence token, including an active-character
identity, carries an interned ID; a direct table lookup supplies its current
meaning. The process interner keeps IDs available across rollback; the document's
meaning table supplies the restored interpretation.

Engine-private frozen commands and permanent font selectors use reserved
identities distinct from ordinary names even when printed alike. They follow
the same compact cursor/meaning path; [formats.md](formats.md) describes their
bootstrap and import mapping.

Macro invocation selects one immutable definition. Literal control sequences
inside its replacement list are resolved when they are read. These are different
lifetimes: the chosen body stays fixed during the invocation, while the commands
it names can acquire new meanings as execution proceeds.

An input cursor carries enough information to produce token words. The resolved
command stays in the reader's local result and is passed directly to its caller.
This keeps stored tokens small and avoids a second meaning lookup in the normal
dispatch path.

Putting a token back into input preserves its identity. Reading it again resolves
its meaning at that moment. One-read expansion suppression is handled separately
by the reader, as described in section 8.

File input is tokenized as it is consumed. Category-code changes affect subsequent
characters, while tokens already captured in a definition or argument keep their
category codes. The file reader retains the current line and lexical cursor;
the detailed character scanner will build on this interface.

## 3. Token sequences and input cursors

A _span_ is a contiguous range of immutable token words. A _sequence_ is empty,
one span in the common case, or an ordered collection of spans when input is mixed.
Definitions, captured arguments, and constructed token lists use this same
sequence representation. Builders retain existing spans and write new or
transformed tokens directly into destination chunks; neighboring spans in the
same chunk can be combined by adjusting their bounds.

A completed sequence owns its span descriptions and retains the backing storage
they need. Session-owned definition storage already has a suitable lifetime;
shorter-lived chunks can use shared ownership. Input cursors carry read positions
separately, so reading advances a cursor while the sequence remains immutable.

Use a small tagged input cursor with source-specific payloads:

| Source               | Cursor data                                          |
| -------------------- | ---------------------------------------------------- |
| File                 | Current line, character position, and lexical state  |
| Immutable token list | Current span and position in its sequence            |
| Macro replacement    | Body sequence cursor and its argument packet         |
| Argument             | Cursor through the selected argument sequence        |
| Inserted tokens      | Short inline tokens or a constructed sequence cursor |

The current source is read directly. When it finishes, the reader retires it and
continues with the source beneath it. A reusable vector holds suspended sources.
The common token-list operation is a load and cursor advance; source changes
perform the push/pop and ownership work.

Token-sequence sources share span traversal: the current span is read directly,
and another is selected when it ends. Macro body reading additionally handles
the internal parameter markers.

The macro body uses compact internal parameter markers. Reading one selects the
corresponding argument sequence and makes it the current source. When that input
finishes, reading continues in the body. The body remains beneath the argument
source until its final substitution has completed, so it retains the argument
storage for all its uses.

Parameter substitution is part of reading the source. Raw argument matching
therefore sees the tokens supplied by an enclosing macro, even though it is
performing no further expansion. Repeated parameters reread sequences; they require
no replacement-list construction.

One- and two-token insertions are common in lookahead and expansion-order
primitives. Give them inline storage in the input representation. Larger
generated results use constructed sequences with the same span-reading path.

Token spans can use indices or internal pointers into stable backing storage.
Definition segments live with the session; captured tokens stay stable once
published to input. A narrow cursor/storage primitive can document these
lifetimes and provide direct reads. Source-stack growth moves cursor records,
while their backing token storage remains in place.

## 4. Definition representation

A definition consists of its flags, literal prefix, parameter descriptions, and
replacement tokens. Its parameter descriptions record the delimiters for the
arguments, up to TeX's nine parameters. Replacement tokens use internal parameter
markers for substitution and ordinary token words for literals.

The definition scanner writes new tokens and parameter markers directly into
session storage, while retaining compatible literal spans from existing sources.
Its completed body is a sequence; its header and parameter descriptions can be
built alongside it. Invocation uses the selected definition directly, with no
compilation step or replacement-list copy on each call. A parameter-free macro
only installs its body cursor.

The definition scanner can prepare delimiter metadata where it helps the matcher.
The literal body remains directly readable. More extensive macro precompilation
can be evaluated later using this execution path as the baseline.

## 5. Argument matching as an ordinary function

`scan_arguments` owns its progress on the Rust stack: current parameter, brace
depth, delimiter position, and capture bounds. It shares the low-level source
helpers behind `next_token()`, using them to read existing spans or produce new
tokens directly into its builder. Both paths use the same meaning resolution,
parameter substitution, and scanner-context checks as the scalar reader. Its
result is an argument packet containing the captured sequences, transferred by
`begin_macro` to the macro input source.

The matcher handles a literal prefix, single-token or grouped undelimited
arguments, and delimited arguments. It compares token identities, including
category codes. Balanced groups are consumed as units while looking for a
delimiter. A single enclosing group is removed from a captured argument according
to TeX's matching rules.

Use separate straightforward loops for undelimited arguments and the common
single-token delimiter. Multi-token delimiters use a streaming overlap matcher,
with a prefix table prepared alongside the definition if useful. Delimiter
progress stays in locals; captured input extends spans or appends new tokens
to the builder. Delimiter removal and enclosing-brace stripping adjust span
bounds. For example, delimiter `aba` in input `aaba` leaves the first `a` in
the argument.

The matching order also determines paragraph-token handling, partial delimiters,
and recovery. Implement these at their corresponding branches, including the
final `#{` parameter-text convention. The delimiter helper belongs inside the
TeX matcher, where brace and scanner behavior are available.

An inherited scanner context supplies restrictions such as `\outer` validity.
The current scanner saves and restores the small context around its ordinary
call. Its intermediate state remains in Rust locals. `\long` handling belongs
to the argument matcher; `\protected` affects expanded-list scanning separately.

## 6. Argument storage follows consumption

The argument builder constructs the packet's sequences in place. Range-backed
token input contributes slices of existing immutable storage. File tokens and
short inline insertions are written directly into destination bump storage.
An argument spanning several sources records those pieces in consumption order,
with the common single-span case kept compact.

While matching within a stored-token source, retain its backing chunk before
consuming it and track the captured span bounds in locals. Record a span when
input moves to another backing span or the argument ends. Parameter substitution
supplies spans from the enclosing argument in the same way. Delimiters and outer
braces are excluded by trimming the resulting sequence. Ownership and descriptor
work happen at span boundaries while the matching loop inspects token words
directly.

The builder retains the backing storage of a span as it captures it. Ownership
then follows the captured input independently of the source cursor, which may
finish during the scan:

```tex
\def\id#1{#1}
\def\pass#1{\id#1}
\pass{X}
```

While matching `\id`'s argument, the reader can exhaust `\pass`'s argument source.
`\id` retains the existing span containing `X`. The consumed cursor can retire,
and the new body uses the same backing tokens.

After capture, parameter substitution reads spans from the completed packet.
For `\def\twice#1{#1/#1}`, file argument `ab` is materialized once and read twice,
producing `ab/ab`. If `ab` is already in a token source, both reads can use that
existing storage. The macro input source retains the packet until its body and
argument sources finish. Unshared backing storage can then return to a reusable
pool.

Retention belongs at backing-chunk granularity. Each packet holds the chunk
owners used by its span descriptors. A child retains the actual backing chunks,
so consumed packet records can retire independently. Session-owned definition
slices already have their longer lifetime. Shorter-lived captured storage can
initially use reference-counted heap chunks, with ownership work paid when chunks
are retained or released.

Chunk-backed bump allocation fits the construction of new tokens and packet
metadata. Finish by retaining or transferring these chunks in place. Reusable
chunks preserve capacity across invocations. Newly built token spans become
immutable when published to input; cursor movement only changes read positions.
The detailed storage design can choose the allocation mechanism while keeping
span reads and ownership transfers the same.

A failed scan releases its builder and retained storage when the function
returns. A completed packet follows the input source consuming it, including
retention by a checkpoint. Composite arguments stay as spans during expansion;
consumers needing a contiguous representation can build that destination directly.

## 7. Source retirement and repeated expansion

Retire exhausted input sources before installing the next macro body, after its
arguments have been captured. This lets a macro ending in another macro call
reuse input-stack space. The captured arguments already have their new owner.

For a tail-recursive body such as `\def\again{A\again}`, each invocation returns
through `begin_macro`; the expansion loop processes the next source. The completed
body cursor is retired before its successor is installed. This keeps normal
macro iteration in the loop with reusable source and argument storage.

Primitive expansion may genuinely nest Rust calls: `\expandafter` expands one
selected token, and a numerical scanner can itself request expanded tokens.
Represent those operations through ordinary calls. Their local progress ends
when they return; the input stack continues to hold only pending token sources.

## 8. Primitive expansion and token-list construction

Primitive functions use the same readers and insertion helpers. Their control
flow encodes how much input to consume and where their result goes:

- `\expandafter` saves the first token locally, expands the second once, and
  places the saved token before the resulting input. It saves the token identity;
  the reader resolves its meaning again when it reaches that token.
- `\noexpand` preserves the next token's identity and suppresses expansion for
  that read. A small input marker or flag supplies its effective meaning; the
  stored control-sequence token keeps its identity.
- `\csname` calls expansion while collecting a name, interns the result, and
  returns it through input with the required meaning-table behavior.
- Conversion primitives write character tokens directly into the destination
  chunks that become their input sequence.

Conditionals use ordinary functions for evaluation and skipping, with their
nesting records in document state. Skip-loop depth and scan progress stay local.
The same readers also serve input primitives and ordinary numerical scanning.

An expanded token-list scanner, used for definitions and related operations,
has its own ordinary collection loop. It shares token reading, macro invocation,
and primitive helpers with normal execution. Its call site determines the
expansion behavior needed for the list it is constructing.

In particular, `\the` of a token list and `\unexpanded` contribute their tokens
directly to an expanded-list builder. Protected definitions remain token
identities in that builder and expand normally during ordinary execution.
The collection loop uses the same sequence-building approach as argument capture
and definitions: it retains existing spans where applicable and writes new or
transformed tokens directly into destination chunks. The result retains any
shorter-lived backing storage it uses.
See the [e-TeX expansion changes](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/etexdir/etex.ch).

The `\expanded` primitive can call this scanner to completion and then insert
the constructed list into ordinary input. This illustrates why the collection
operation can remain an ordinary Rust call even when it invokes expansion.
See [pdfTeX's implementation](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/pdftexdir/pdftex.web).

Recoverable diagnostics and inserted recovery tokens are handled where a scanner
detects the problem. Missing resources and fatal outcomes return through the
Rust calls to the session, which can restart from its retained checkpoint.

## 9. The paragraph checkpoint boundary

The established boundary remains the end of a paragraph with no open groups.
The expansion loop has delivered the paragraph-ending command and paragraph
work has completed. Active Rust scanner and expansion calls have returned.

Pending replacement input can span that boundary:

```tex
\long\def\two#1{#1\par#1\par}
\two{Hello}
```

After the first paragraph, the remaining body cursor and argument `Hello` are
still input data. Retain their positions and backing argument packet in the
checkpoint, alongside the ordinary file input. Resumption reads the next tokens
through the same loop. The checkpoint stores pending input, while the scanners'
temporary values have already disappeared through normal returns.

Immutable definition and argument storage can be shared with the checkpoint.
Cursor positions are small copied values. Source ownership keeps a retained
packet available after execution finishes consuming its own copy of the cursor.
Paragraph scratch is released independently of this input storage.

This also keeps parameter substitution cheap across a checkpoint: it reads the
same ranges as before. Host yields occur at the paragraph boundary; expansion
and argument matching run as ordinary calls between those yields.

## 10. First implementation and measurements

Start with the two readers, immutable definitions, macro argument capture, and
span-based substitution. Add ordering primitives and expanded-list scanning
through the same functions. A small implementation can expose the actual
allocation and dispatch costs before choosing more elaborate representations.

Useful examples to run against TeX/pdfTeX include repeated parameters, nested
argument capture, body redefinition during invocation, overlapping delimiters,
and `\expandafter` chains. The two-paragraph macro above exercises checkpoint
retention while an argument packet remains live. Expanded definitions exercise
one-read suppression and reuse of token-list results.

Measure plain file input, literal macro bodies, parameter-free calls, and calls
with repeated arguments separately. Inspect release code for the token-list load,
meaning lookup, and expansion dispatch. Allocation measurements should show
whether buffers and source records are reused, how often existing token spans
are shared, and how many new tokens are materialized. Mixed-source arguments
will show the cost of span traversal. These results can guide cursor layout
and packet allocation without changing the execution model.
