# Execution, meanings, and groups

This proposal connects [expansion.md](expansion.md) to mutable TeX state and list
construction. Primitive execution uses ordinary Rust functions. Groups and modes
are semantic TeX state; scanners and the unfinished work of a primitive stay in
Rust control flow.

## Scanner contexts and outer macros

`\outer` is a definition prefix, independently combinable with `\long`,
`\protected`, and local/global assignment. Outer status is part of the macro's
meaning: aliases preserve it, `\meaning` prints it, and `\ifx` distinguishes
otherwise identical outer and non-outer macros. Protected outer macros remain
unexpanded in expanded definitions, subject to the same outer validity checks.

The engine keeps a transient scanner context: normal execution, definition
scanning, macro parameter matching, balanced text absorption, or conditional
skipping. Definition scanning starts after the target control sequence has been
read and includes both signature and body. Matching includes compulsory prefix
tokens, even when the macro has no parameters. Balanced text contexts cover
message/token-list text, case conversion, `\unexpanded`, and `\detokenize`;
their opening-brace scans use the context required by the invoking primitive.
Skipped text is unexpanded and uses non-inserting name lookup.

An outer control sequence read in any non-normal context fails with
`Error::Outer`, whether it comes from a file, active character, stored token list,
or substituted parameter. This is the fail-fast counterpart of upstream's
`check_outer_validity`; no recovery tokens are inserted. Existing EOF and other
scanner errors retain their consumption order. Character-run paths do not
contain control-sequence tokens and need no outer check. Direct emission of an
already-retained token list by `\the` into an expanded list does not reread its
tokens and therefore does not impose another validity check.

The operands of executed `\noexpand`, `\string`, `\meaning`, `\ifdefined`, and
`\ifx` temporarily use normal context. A token already marked for one-read
expansion suppression bypasses the outer check on that suppressed read.
Unexpanded text containing literal `\noexpand` still checks subsequent tokens;
it does not execute the primitive. Other expanded operands, including
`\ifcsname`, keep the enclosing scanner context.

Context scopes restore their caller's context on both success and failure.
They live in ordinary Rust calls, allocate no frames or owners, and are absent
from checkpoints. A public quiescent boundary requires normal context; restore
resets transient context before rebuilding document roots. Future included-file
EOF handling must check context before crossing back to a parent file.
These rules follow the scanner status, token-reader, macro-call, conversion, and
conditional routines in the [TeX source](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web)
and the corresponding [e-TeX extensions](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/etexdir/etex.ch).

## 1. The execution loop

### Horizontal boxes

Executed `\hbox` accepts natural size, `to`, and signed `spread` specifications.
Its brace opens a typed group and a restricted horizontal construction frame.
The frame owns the enclosing text state, packing specification, and destination;
the current text state owns the unfinished contents. Nested boxes use the same
versioned nodes and payload slots. Restricted `\par` does not end a paragraph;
vertical-only material fails at its native mode boundary. Packing restores the
box group's assignments before measuring and transfers the resulting owner once
to its enclosing list or box register. Appending a nonvoid box resets horizontal
space factor or uses the existing vertical baseline-glue path. Void boxes append
nothing. Horizontal `fil`, `fill`, `ss`, and negative `fil` glue retain their
normal TeX orders and signs.

`\setbox` captures its index and effective globalness before constructing its
operand, but replaces the destination only when that construction finishes.
The previous destination remains available to the body. `\everyhbox` is queued
after entering restricted mode; an outstanding `\afterassignment` token is
then placed ahead of that hook and the body. Immediate `\box`/`\copy` operands
finish their register assignment before that token runs. Parent character
shaping completes before box dispatch and cannot observe later assignments in
the box body.

Box registers hold affine singleton-list owners in the existing COW tables.
Local assignment moves the previous owner to a typed save record; replacement,
global override, and group restoration release displaced owners explicitly.
Taking `\box` makes the register void without changing its assignment level;
`\copy` allocates a semantic copy, including all owned children. In particular,
`\setbox0=\box0` consumes its source before installing the destination and is
not an identity assignment. `\wd`, `\ht`, and `\dp` read void as zero. Their
assignments consume a dimension even for void registers and mutate live headers
directly, without group saves or global-prefix effects. Empty boxes remain
nonvoid; `\ifvoid`, `\ifhbox`, and `\ifvbox` inspect the owned header.

Checkpoint capture includes register and save owners only through explicit
snapshot operations paired with the node-store root. Open construction frames
exclude checkpoint eligibility. Restore clears their inert handles while
replacing the abandoned whole roots; it never releases old IDs through restored
storage. `\shipout` supplies a consuming destination to the same box scanner,
including constructed boxes and void register operands. Output groups use the
ordinary assignment/save stack with a separate document-owned enclosing text
frame; their closing order and fail-fast errors are contracted in
[pages.md](pages.md). Math destinations remain outside this execution slice.
Packing retains the dimension/glue baseline in [paragraphs.md](paragraphs.md): exposed `\badness`, packing diagnostics, and
overfull-rule insertion are not implemented. The corresponding initialized
parameters remain state-only. These rules follow `begin_box`, `box_end`,
`package`, `alter_box_dimen`, and `prefixed_command` in the
[TeX source](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web).

### Vertical boxes and owned transfers

`\vbox` and `\vtop` use the same construction frames and packing specifications.
Their internal vertical mode begins with ignored previous depth and normal
paragraph parameters, then runs `\everyvbox`. Paragraphs inside this mode are
ordinary horizontal mode: `\ifinner` is false during their text and true again
after they end. The first paragraph of an empty internal vertical list omits
paragraph skip. Internal lists never enter the external page builder. A closing
vertical-box brace finishes its paragraph before restoring assignments, captures
the effective `\boxmaxdepth`, then restores the group and packs. `\vtop` finally
uses the first node's height if it is a box or rule, otherwise zero, preserving
total height plus depth; its resulting depth can exceed the earlier depth cap.

Unboxing validates type before taking its register owner. Consuming operations
move the child list once and reclaim only the old header; copying operations
copy only the children. Header dimensions and glue settings do not affect the
spliced contents. Neither operation changes space factor or previous depth.
Void and empty operands still obey mode transitions: horizontal unboxing in
vertical mode backs its command before starting a paragraph and `\everypar`;
vertical unboxing in ordinary horizontal mode backs its command behind the
current `\par` token. Both reread that command's meaning and scan operands only
after intervening hooks. Vertical unboxing in restricted horizontal mode fails.

`\lastbox` takes only a genuine trailing box outside discretionary replacement
ranges, clears its shift, and preserves list auxiliary state. Empty or nonbox
tails give void; an empty external contribution list is an error. `\raise` and
`\lower` accept horizontal mode, while `\moveleft` and `\moveright` accept
vertical mode. They capture their signed displacement before constructing the
box; an invalid axis fails before scanning operands. All these transfers retain
the existing affine register, child-list, snapshot, and failure-cleanup rules.

The outer loop asks `next_expanded` for the next command and its current meaning,
then dispatches using the current mode. A primitive scans its operands, performs
its immediate effects, and returns. Most assignments take effect before the next
command is expanded; constructed box assignments finish when their semantic
group closes, as described below. In horizontal mode, character commands use the
text character path; in math mode they build math atoms. Finishing an unrestricted
paragraph returns to its enclosing vertical mode.

Use a direct command dispatch with a few specialized inner loops where they help:
character runs, consecutive assignments with prefixes reset for each command,
and list traversal during packing. A local already-read token can pass to the
next dispatch rather than
being resolved again. If TeX puts it back into input, it retains its identity and
gets its meaning again on the subsequent read, as the expansion design requires.

The loop distinguishes expansion from execution but does not queue expanded
commands. A definition, category-code change, or register assignment can affect
the very next token. Helpers such as `scan_int`, `scan_dimen`, `scan_glue`, and
`scan_toks` are calls into the same input and expansion routines.

## 2. Meanings and values

A meaning table entry records the command kind, its compact payload, and the
assignment level. A macro meaning points to an immutable definition. A register
alias points to the selected register, not a copy of its value. Character and
math-character definitions carry their command and code; primitive aliases carry
the primitive's command and modifier.

`\let` copies the selected meaning at assignment time. It does not create a late
binding to another control-sequence name. Ordinary macro-body control sequences
retain their names and resolve current meanings when read. For example:

```tex
\def\a{old}\let\b=\a\def\a{new}
\b\a
```

The result uses the old body for `\b` and the new body for `\a`, with no body
copy. `\futurelet` takes the lookahead meaning while preserving the required token
order. Undefined identities, `\relax`, and one-read `\noexpand` behavior remain
distinct even when they take similar execution paths.

Registers and parameters use typed payloads: integers, scaled dimensions, glue,
token sequences, boxes, font selectors, and the other profile-specific values.
The assignment layer owns restoration; each payload's store owns its lifetime.
See [storage.md](storage.md) for dense tables and node ownership.

Binding replacement and mutation of an owned value are separate operations.
Changing `\wd0` mutates that box even inside a group; taking `\box0` empties the
register without creating a binding-restoration record and preserves its
assignment level. Mode fields such as `\spacefactor` and `\prevdepth`, page
dimensions/counters, and globally effective font parameters follow their owner's
rules. `\global` controls the assignments for which TeX defines it, rather than
turning every field mutation into a binding operation.

## 3. Group restoration

Use a reusable save stack with typed records for group boundaries, saved values,
after-group tokens, and the small data needed when a semantic group closes.
An entry's assignment level identifies the most recent group that wrote it.
Use our own outer-level convention consistently; TeX's source calls its outer
level `level_one`.

On a local assignment within an open group, save the previous value and level when
the entry was last assigned outside the current group. At outer level, replacement
saves nothing. Repeated local assignments in that same group
replace the current value without repeatedly saving it. A global assignment sets
the outer level. On group exit, restore a saved value unless the current entry is
global; then discard the saved owner instead. Restoring also restores its saved
assignment level.

The extended pdfTeX profile first recognizes an identical local assignment,
using the scalar value or command/payload identity, and leaves its level and save
stack unchanged. Any incoming owner is released as the primitive requires.
This also preserves restoration tracing behavior.

This naturally handles a global write followed by another local write:

```tex
\count0=1
{\count0=2 \global\count0=3 \count0=4}
```

The last local write saves the global value `3`. Unwinding that record restores
`3`; unwinding the earlier record sees a global entry and keeps it. A shortcut
that remembers only the first write in the group would get this wrong.

Saved payloads move into the stack rather than being copied when ownership can
transfer. Restoration moves them back. Discarding a current or saved value follows
its payload semantics: definitions and shared immutable values release their
owner at that boundary, and a box relinquishes its list.

`\aftergroup` stores token identities and inserts them in TeX's order as the group
closes. `\afterassignment` is a separate pending token inserted at the
assignment-specific point. Its pending state belongs to the document even at outer
level. Prefix parsing, including `\globaldefs`, uses ordinary locals and preserves
the command-specific restrictions and error recovery.

The restoration algorithm is based on `eq_define`, `geq_define`, and `unsave`,
including their assignment-level behavior.
See the [TeX save-stack implementation](https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/tex.web).

## 4. Modes and semantic groups

Keep a compact semantic nest for vertical, horizontal, and math lists, including
their restricted/internal forms. Each entry owns a list builder and the mode's
actual persistent values: previous depth, space factor, paragraph bookkeeping,
or math-list state. Entering a box, alignment entry, or formula pushes that state;
closing it returns the resulting list to its semantic owner.

The group kind selects the finishing action for `}`, `\endgroup`, math delimiters,
or alignment boundaries. TeX's closing actions differ: a simple group restores
bindings, an hbox group packs a list, and an insertion group transfers its list.
This is a small semantic dispatcher, not an interpreter continuation.
Temporary dimensions or packing options needed until a group closes live with
that group, just as other TeX state does.

For `\setbox0=\hbox{...}`, the box group retains the target register and
local/global assignment choice along with its packing options. Closing the group
restores bindings, packs the box, pops the semantic nest, and performs the box
assignment. A pending `\afterassignment` token is inserted after construction
starts, before the box body and `\everyhbox`, while the register still holds its
old value. Immediate box-taking operations complete directly. This is semantic
group data, with no suspended assignment call.

Starting a paragraph constructs its horizontal list directly in node storage,
sets paragraph state, and installs `\everypar` through ordinary token input.
Finishing it runs line breaking to completion, appends the resulting lines and
adjustments to the vertical list, and invokes page building in TeX's order.
These functions release algorithm scratch before they return.

The outer loop, rather than a recursive invocation of the whole interpreter,
continues to consume tokens for a box or output routine. Semantic nesting supplies
their current mode and ending action. Expansion-order primitives and scanners
still use ordinary nested Rust calls where needed.

Ordinary assignment groups can remain open at checkpoints. Snapshot copies retain
their saved values and after-group tokens independently of the reusable live stack.
Construction groups still prevent eligibility until their finishing action returns.

## 5. Arithmetic and scanning

Use explicit integer routines for TeX's arithmetic. Dimensions are scaled integer
values; glue has separate stretch and shrink orders. Scanning signs, decimal
fractions, internal quantities, units, and optional keywords must preserve TeX's
token consumption and rounding. An unsuccessful keyword match backs up its
matched prefix and mismatching token after expansion, in their original order.
The leading spaces that keyword scanning skipped remain discarded.

Avoid routing every numeric operation through a generic value object. The common
integer and dimension scanners return compact typed values. Internal-quantity
scanning uses a small tagged result where TeX genuinely accepts different value
levels, then applies the required coercions. Overflow and bad-unit recovery occur
at the operation TeX specifies, including inserted or backed-up tokens.

Expression primitives can use local stacks or recursion for nested parentheses.
Their intermediate state is gone when the primitive returns. The storage and
checkpoint design does not impose resumable expression frames.

## 6. Driver handoff

Checkpoint eligibility is checked at the outer loop before reading a command,
after any already-read command has been consumed. The driver must be in external
vertical mode with no unfinished construction or output routine and no operation
scratch. Ordinary assignment groups may remain open; their save records, levels,
and pending tokens are snapshot state. See [checkpoints.md](checkpoints.md).

Paragraph completion requests a handoff. If page building installed an output
routine, execution continues through that routine first. Its closing action can
provide the next eligible handoff without waiting for another paragraph. A coarse
work budget also requests handoffs during eligible preamble/vertical execution.

Cancellation checks can return through scanners and execution independently of
eligibility. An interrupted attempt restores the last checkpoint; it does not
preserve the interrupted call's locals.

## 7. Compatibility examples that shape the design

Local/global/local writes exercise restoration levels. An active macro that
redefines its own name exercises body lifetime. `\let` and register aliases
exercise the distinction between identity, meaning, and value. Taking versus
copying a saved box exercises node ownership. Output routines exercise mode and
group transitions while a page remains unfinished.

Use these small cases alongside actual format loading and paragraph construction.
These cases exercise the state representation and ownership transitions.
