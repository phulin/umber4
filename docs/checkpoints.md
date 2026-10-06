# Paragraph checkpoints and restart

This proposal develops the in-memory checkpoint boundary in
[architecture.md](architecture.md). A checkpoint resumes ordinary execution from
the outer loop. It does not serialize Rust calls, scanners, or the expansion
machine's intermediate work.

## 1. The boundary is fixed

The boundary is the end of a paragraph with no open groups. Line breaking and
synchronous paragraph-finishing work have returned, their working memory has
been released, and the driver is about to request the next command. Closing a
group alone does not create a checkpoint. A paragraph ending inside a group is
ineligible; execution continues to a later eligible paragraph.

The outer driver owns the handoff. Paragraph finishing reports completion to it
through ordinary return values. If page building installs an output routine,
that routine's semantic group is open and prevents a checkpoint at this handoff.
The interpreter continues executing it through the same loop. Nested scanners
and expansion helpers never call the scheduler.

This makes the boundary simple to recognize without preserving scanner progress.
It does not imply empty input or a finished page. A file can still be open, a
macro body can have remaining tokens, and a conditional can span paragraphs.
Those are persistent TeX state.

Initialization also provides a restart root before document execution, after the
chosen format is ready. It is the fallback when an edit or resource request
precedes the first paragraph checkpoint. Completion returns a result normally;
it does not introduce another mid-execution yield point.

## 2. Retain roots according to their storage

The checkpoint has a compact scalar state and owners for subsystem roots:

| State                                         | Snapshot choice                                                           |
| --------------------------------------------- | ------------------------------------------------------------------------- |
| Small parameters and character tables         | Copy arrays and values                                                    |
| Meanings and registers                        | Share table blocks; copy a block on its next write                        |
| Mutable nodes and unfinished page state       | Retain the node-store root and list roots                                 |
| Pending token input                           | Copy cursor positions; share immutable sequences and argument packets     |
| File and read-stream input                    | Copy positions and lexical state; retain current lines and resource views |
| Conditional nesting and other semantic stacks | Copy their compact entries                                                |
| Document backend and generated streams        | Retain their versioned roots, handle positions, and buffers               |
| Completed pages and diagnostics               | Retain immutable prefixes                                                 |

The group restoration stack and paragraph algorithm scratch are empty at this
boundary. Process identities and reusable immutable resources are outside the
snapshot; rollback preserves them. Session definition storage remains available
for any restored meaning or input cursor. [storage.md](storage.md) defines the
table, node, and token ownership rather than assigning every state field the
same persistence strategy.

Start by copying the small root directories as well as their scalar metadata.
If directory copying dominates checkpoint cost, change that directory's
representation. Snapshot creation does not traverse all nodes or recopy all
replacement tokens.

## 3. Pending input is not a continuation

For example:

```tex
\long\def\two#1{#1\par#1\par}
\two{Hello}
```

The first paragraph can establish a checkpoint while the invocation still has
input remaining. The snapshot retains its body position and immutable argument
packet. Resume enters `next_expanded` normally and reads the remaining tokens;
there is no suspended argument matcher or paragraph function.

File cursors similarly retain the current prepared line, byte position, and
lexical state. Stream objects are logical cursors over immutable or versioned
bytes, not borrowed operating-system file descriptors. EOF insertion and
unfinished conditionals keep the semantic records that their normal input and
expansion loops already use.

Transient effective commands such as one-read `\noexpand` state stay with the
pending input representation if not yet consumed. A dispatched command and its
scanner locals have completed before the handoff. The checkpoint does not retain
an arbitrary already-read command waiting on the Rust stack.

## 4. Restore is selection and bulk release

To restore, replace the live subsystem roots with those retained by the
checkpoint, restore scalar values and cursor positions, and reset scratch lengths.
Dropping the abandoned roots releases uniquely owned chunks and auxiliary
objects. Reusable scratch capacity can remain. Interned names discovered later
remain stable, while their document meanings again come from the restored table.

Each checkpoint also records the session definition store's append boundary.
Discard younger execution checkpoints and owners before rewinding an abandoned
tail to that boundary. Earlier definitions and their spans remain in place;
host-visible output holds detached bytes, so it carries no tail references.
This recovers repeated preamble construction when several missing resources cause
replay before the first paragraph. Successful execution can still use simple
session-lifetime definition storage without collecting individual definitions.

Output pages, diagnostics, and generated-file contents become the checkpoint's
prefixes. Backend object numbering and resources return to their retained state.
The session then enters the same outer execution loop used by a fresh run.
The host sees a replacement result prefix when necessary, so abandoned pages
are not mistaken for new output.

Published host results contain immutable page records and bytes, rather than
live node IDs. An old result can remain displayed while a new branch compiles
without holding the mutable interpreter state alive.

## 5. Choosing a restart after an edit

The first implementation reuses a valid prefix and executes the remaining suffix.
It does not try to splice in a previously computed suffix or infer semantic
equivalence between two arbitrary engine states.

Choose the latest retained checkpoint whose already-observed inputs are unchanged.
First revalidate the selected format baseline and its setup dependencies. If they
changed, rebuild/reimport it, discard the old document checkpoints, and create a
fresh initialization root; restarting from the old root would retain stale format
semantics. [formats.md](formats.md) defines baseline identity and setup.
For character input, the relevant prefix includes whole prepared lines, since
line loading has already fixed trailing-blank handling and `\endlinechar`.
Track the furthest prepared byte boundary for each source, including sources
already closed. For a resource consumed as a whole, such as font metrics or an
image, retain its immutable version. Retain file-existence and failed-open
observations as well: a newly created file can change execution before any bytes
are read from it. An observed EOF must remain at the same byte boundary, including
closed text sources, read streams, and a final line terminated by EOF. Appending
bytes after that boundary changes execution even when the old prefix is identical.
Intentional `\endinput` closure is distinct from observing actual EOF and can
leave a suffix unobserved. Resource metadata queried by pdfTeX also counts as an
input observation, independently of byte contents.

This is small per-resource information accumulated by the resource layer, not a
record of expansion steps. If an input observation changed, choose an earlier
checkpoint or the initialization root. Ordinary editor changes can use their
known changed byte ranges; externally supplied resources can compare versions or
the required prefixes. Conservative rejection is an acceptable initial choice.

When only a file's unread suffix changed, a resumed cursor uses the new resource
version at the same raw byte boundary. Its retained prepared line is unchanged
because that whole line belonged to the validated prefix. Future line loads see
the edited bytes. Resource names and lookup outcomes are revalidated against the
new input view, as described in [resources.md](resources.md).
Revalidate retained prefix checkpoints too, replacing their source-view owners
with the new versions when valid and keeping exact prepared-line contents and
lexical state. Rebind raw-line spans to the validated new bytes too; independently
owned normalized or rewritten lines can keep their existing owners.
Release invalid younger checkpoints and obsolete source buffers. Otherwise a
small suffix edit could retain an entire old source version per editing branch.

Generated files are part of restored document output, not editor inputs. Their
logical contents and read positions return to the checkpoint's versions. A later
pass starts with the accepted output of the preceding pass as its input view.

## 6. Resource requests and scheduling

The browser adapter may discover a resource it does not have. The request travels
up as an ordinary error result, unwinding the current Rust calls. The session
restores the preceding checkpoint or initialization root before returning the
request to the host. Supplying the resource then restarts execution from that
root. There is no yield at the unresolved primitive itself.

Generated streams, pages, and diagnostics are branch-owned, so replay restores
their prefixes rather than duplicating their effects. Immutable resources fetched
for the abandoned suffix stay cached for the replay. Request identity includes
the logical operation and input view needed to supply the same answer.

Scheduler yields and cooperative cancellation occur only at checkpoint handoffs.
Running the browser engine in a worker handles long intervals without a boundary;
terminating a worker is a host action, not a resumable interpreter state.
Checkpoint retention may be less frequent than eligible boundaries, but a yield
retains the state from that boundary.

## 7. Retention and cost

A one-shot native compile can use the same interpreter without retaining every
eligible checkpoint. An editing session keeps a bounded selection of recent and
older checkpoints according to its memory budget. Releasing a checkpoint drops
its roots; shared objects disappear when their last owner goes away.

The boundary can leave a large unfinished page or saved boxes alive. Measure
those actual owners, table/node chunk copies after snapshots, and retained input
packets, definition tails during resource replay, and whole source-version bytes.
Allocation between checkpoints remains linear and mostly bump-based;
snapshot sharing changes only the stores that are subsequently mutated.

The useful correctness comparison is a resumed run versus a fresh run on the
same edited input view and retained run environment. Include a macro spanning
paragraphs, a conditional spanning
paragraphs, a saved box, a partially accumulated page, a generated file read back,
an append after a source's observed EOF, and several missing resources discovered
before the first paragraph or during shipout. These cases expose
the retained roots directly without changing the checkpoint boundary.
