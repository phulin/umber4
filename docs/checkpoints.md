# Quiescent checkpoints and restart

This proposal develops the in-memory checkpoint boundary in
[architecture.md](architecture.md). A checkpoint resumes ordinary execution from
the outer loop. It does not serialize Rust calls, scanners, or the expansion
machine's intermediate work.

## Implemented expansion/register boundary

The first executable slice supplies an in-memory `Checkpoint` for unchanged
source bytes. `Engine` is the session and non-consuming driver; its document
root contains all restorable semantic state. `advance(command_budget)` executes
complete outer commands and returns `Progress::Boundary` or `Progress::Complete`.
`can_checkpoint()` reports eligibility. `checkpoint` is legal only at a successful
driver boundary (including startup and completion). An error leaves the attempt ineligible until a retained root is
restored. `restore` rejects checkpoints from another session before changing live
state. Source bytes remain borrowed for the engine/checkpoint lifetime; the
checkpoint does not borrow the engine. `outcome` publishes a replacement message
prefix and register values in independently owned host buffers. Those buffers may
outlive or be mutated independently of all engine roots. `run` remains the
consuming convenience adapter. A command budget is a coarse handoff request, not a bound on
macro expansion or scanner work.

This interpreter has no layout nest: all its successful outer boundaries are
preamble-like. Ordinary assignment groups and conditionals may remain open.
Future layout state must extend the document root and eligibility predicate with
external vertical mode, construction, and output-routine checks. This slice does
not claim to capture those future roots.

Meanings and their payload owners share 64-entry COW pages; token registers use
the same page mechanism. Small scalar arrays, save records, and conditional
records are copied. Page copies reuse bounded session caches and retain immutable
payload owners once per copied physical page. Session spelling identities and
value/metadata registries survive rollback; scratch and free lists are not
snapshot state. Names discovered on an abandoned branch remain interned with
undefined restored meanings until assigned again.

Live packet slots remain reusable expansion storage. A checkpoint copies only
active argument span descriptors into its own immutable metadata buffers and
retains their token owners. Saved cursors contain logical positions, span indices,
and checkpoint-local packet coordinates, never raw pointers or reusable live
packet IDs. Restore checks out live packet slots, remaps those coordinates, and
reconstructs raw token windows from retained owners. Definitions and argument
token words are shared without flattening or per-token reference counting.
Snapshots retain their backing stores and can safely outlive the engine.

The prepared input line is copied with its lexical position/state, including any
committed superscript translations. Source bytes remain borrowed and unchanged.
Message history shares immutable append blocks; appending after a checkpoint
starts a new block rather than copying old message bytes. Log and snapshot
descriptors use pooled backing; new message byte vectors retain the existing
durable-output allocation behavior, so repeated message production is not
allocation-free. Explicit host publication copies shared bytes as necessary.
Repeated restore
selects the same roots and releases the abandoned suffix. This slice excludes
source edits, filesystem effects, persisted serialization, and scheduler policy.

For example (the API documentation also compiles this pattern):

```rust
use umber::{Engine, Progress};
let mut engine = Engine::new(br"\count0=7\relax\count0=9\relax\end");
engine.advance(1)?;
let saved = engine.checkpoint()?;
while engine.advance(32)? != Progress::Complete {}
assert_eq!(engine.outcome().counts[0], 9);
engine.restore(&saved)?;
assert_eq!(engine.outcome().counts[0], 7);
```

The `native-profile` feature uses this same driver and snapshot code. Adding a
subsystem requires adding its semantic roots to `Document` and to the centralized
checkpoint capture/restore, then extending `can_checkpoint()` when unfinished
construction can span outer commands. No second session layer is needed.

## 1. Eligibility and cadence

A checkpoint is eligible when all of the following hold:

- Control is at the outer driver, before requesting the next command; no
  already-read command or unfinished scanner remains in a Rust local.
- The semantic nest is in external vertical mode, with no unfinished paragraph,
  box, math, alignment, insertion construction, or output routine.
- Operation scratch has been released. Persistent owners and all pending input
  are represented in document state.

Ordinary brace and `\begingroup` assignment groups may remain open. Their boundary
records, saved values, assignment levels, and after-group tokens are snapshot
state. Construction groups remain excluded. This permits paragraphs inside ordinary
LaTeX environments without making scanners resumable.

Paragraph completion is the preferred cadence. The driver can also checkpoint
after an output routine closes, after returning to eligible vertical mode from a
completed construct, or during preamble assignments. A coarse work budget requests
another checkpoint; eligibility determines when it can actually be taken. Closing
an arbitrary group does not by itself establish eligibility. Nested helpers never
create snapshots.

An unfinished page, open file, remaining macro body, or conditional stack is valid
persistent state. Initialization supplies a restart root after format setup, before
document execution. It remains the fallback for work preceding the first eligible
checkpoint. Completion returns a result normally.

## 2. Retain roots according to their storage

The checkpoint has a compact scalar state and owners for subsystem roots:

| State                                         | Snapshot choice                                                           |
| --------------------------------------------- | ------------------------------------------------------------------------- |
| Small parameters and character tables         | Copy arrays and values                                                    |
| Meanings and registers                        | Share table blocks; copy a block on its next write                        |
| Mutable nodes and unfinished page state       | Retain the node-store root and list roots                                 |
| Pending token input                           | Copy cursor positions; share immutable sequences and argument packets     |
| File and read-stream input                    | Copy positions and lexical state; retain current lines and resource views |
| Ordinary groups and conditional stacks        | Copy compact records and retain saved payload owners                     |
| Document backend and generated streams        | Retain their versioned roots, handle positions, and buffers               |
| Completed pages and diagnostics               | Retain immutable prefixes                                                 |

Operation scratch is empty at this boundary. Ordinary group save records can be
nonempty and retain their saved owners independently of the live stack. Session
identities and reusable immutable resources are outside the snapshot; rollback
preserves them. Restored meanings and input cursors retain
their immutable definitions and token values. [storage.md](storage.md) defines the
table, node, and token ownership rather than assigning every state field the
same persistence strategy.

Start by copying the small root directories as well as their scalar metadata.
If directory copying dominates checkpoint cost, change that directory's
representation. Snapshot creation does not traverse all nodes or recopy all
replacement tokens. Append-only output and descriptor histories share blocks and
retain an endpoint, rather than copying their whole prefix into each checkpoint.
Snapshot records and copied arrays use reusable allocation blocks.

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

Immutable definitions and token lists follow the retained roots. Dropping younger
checkpoints and the abandoned live state releases their owners; values shared
with an older checkpoint remain allocated. Empty backing chunks return to a
bounded reuse pool. Repeated preamble execution can therefore reclaim obsolete
values even before the first eligible checkpoint, while names remain available
for replay.

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
changed, rebuild or reload it, discard the old document checkpoints, and create a
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
Rebind ordinary external readers' unread read-ahead ranges as well, recomputing
window/refill bounds for a shortened suffix while preserving the prepared line
and logical cursor. Already-aliased external windows keep their backing under
the full-version validation rule in [resources.md](resources.md).
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

Normal scheduler yields occur at eligible handoffs and retain that boundary's
state. Cancellation checks are independent: expansion dispatch and long scanning,
skipping, layout, and output loops check at coarse work intervals. An interruption
unwinds ordinary calls and restores the last checkpoint; it creates no new resume
point. Keep per-token host calls and clock reads out of these checks.

A host can expose a cancellation flag when its execution environment supports it.
Otherwise a configured work limit can terminate an attempt and return control.
Browser message delivery alone cannot interrupt synchronous Wasm in the same
worker. Worker termination remains a fallback. A work-limit interruption is not
an automatic replay loop: repeating an unchanged budget before any new checkpoint
would repeatedly discard the same work. The caller must change the limit/input or
stop that attempt.

Prepared bundles and prefetching dependencies from the previous successful run
reduce sequential resource misses. Prefetch does not count as a TeX observation;
only executed resource operations affect restart validation.

## 7. Retention and cost

A one-shot native compile can use the same interpreter without retaining every
eligible checkpoint. An editing session keeps a bounded selection of recent and
older checkpoints according to its memory budget. Releasing a checkpoint drops
its roots; shared objects disappear when their last owner goes away.

The boundary can leave a large unfinished page or saved boxes alive. Measure
those actual owners, table/node chunk copies after snapshots, and retained input
packets, immutable values during resource replay, and whole source-version bytes.
Allocation between checkpoints remains linear and mostly bump-based;
snapshot sharing changes only the stores that are subsequently mutated.

The useful correctness comparison is a resumed run versus a fresh run on the
same edited input view and retained run environment. Include a macro and a
conditional spanning paragraphs, an ordinary open group with local/global/local
assignments and after-group tokens, a saved box, an unfinished page, generated-file
readback, an append after observed EOF, and resource misses before the first
checkpoint or during shipout. Check interruption during paragraph-free expansion
and long scans separately from successful checkpoint creation.
