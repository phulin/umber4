# Repository Guidance

Read this file first, then the nearest nested `AGENTS.md` before editing within
a subdirectory. Keep repository maps current when files or directories are
added, removed, or repurposed.

Umber4 is a fresh design for a Rust TeX interpreter targeting native binaries
and WebAssembly, with native pdfTeX performance as the baseline and resumable
checkpoints for incremental compilation. The repository contains design documents
and an initial expansion/register interpreter. The full architecture remains a
proposal until reviewed with the user.

## Repository Map

- `docs/`: architecture proposals and, as implementation proceeds, subsystem
  contracts and performance methodology.
- `crates/umber/`: portable expansion/register MVP, in-memory checkpoints, typed layout storage, TFM metrics/horizontal text, and
  its native CLI.
- `Cargo.toml`, `Cargo.lock`: explicit workspace membership and dependencies.
- `fixtures/mvp/`: same-source INITEX compatibility fixtures.
- `scripts/`: native oracle setup and compatibility comparison.
- `benchmarks/`: source generators, native profile builds, CPU and memory measurements,
  and retained results;
  `corpus/` retains selected Gentle macros and source provenance.
- `.gitattributes`: preserves upstream whitespace in the selected corpus excerpts.
- `.agents/skills/coordinate/`: GitHub Issues coordination and agent dispatch.
- `.agents/skills/parallel/`: isolated persistent worktrees and integration.
- `README.md`: project purpose and entry point to the design.

When a Cargo workspace is introduced, use explicit member paths and update its
manifest and lockfile together when adding or removing crates. Keep native
tools and worktree caches checkout-local.

## General Instructions

- Do not add dependencies.
- Commit logical chunks after validation. Use a one-line summary followed by
  useful details, in rough Conventional Commits style.
- Prefer principled architecture, clean code, and fast implementations. Refactor
  areas that become difficult to understand; cross-cutting changes are appropriate
  when they produce a cleaner design.
- Prioritize exact TeX semantics, clear ownership, semantic-lifetime storage, and
  quiescent checkpoint boundaries. Synthetic benchmarks are diagnostic tools,
  not architectural acceptance gates or a requirement to eliminate every per-case
  CPU regression. Defer extensive optimization until the architecture and
  representative full-document paths mature.
- For complex features, write technical documentation in `docs/` before
  implementation. Do not commit temporary task plans or notes.
- Treat Umber3 as reference material, not an inherited architecture or correctness
  oracle. Use upstream TeX/pdfTeX and independent fixtures for compatibility evidence.
- Preserve exact TeX semantics, ownership, and failure behavior during optimization.
  Measure actual release execution paths and inspect generated instructions when
  tuning hot loops. Avoid repeated classification, bookkeeping, allocation,
  reference counting, and indirection without measured benefit.
- Build data in place in its destination storage. Reuse immutable ranges and
  transfer ownership of existing storage wherever possible to avoid copies.
- Design object stores around semantic lifetimes and quiescent checkpoint
  boundaries. Keep unsafe allocation primitives narrow and document why their
  layout, references, and reclamation are valid.
- Aim to keep source files below roughly 600 lines when a logical split exists.
  Prefer `#[cfg(test)] mod tests;` with separate test files for nontrivial tests.
- Run checks appropriate to the change. Once Rust code exists, run relevant tests,
  rustfmt, and clippy before completing implementation work. Do not invent checks
  or scripts that the repository does not yet provide.
- Reach first for `rg` and limit search output aggressively.
- Codex: spawn independent subagents with `fork_turns: "none"`. Use separate
  worktrees for parallel implementation and keep overlapping work serialized.
- Combine adjacent commands when practical, such as committing after checks pass.

## Writing Markdown

When dprint is introduced, use Markdown that its plugin accepts; do not silence
formatting checks with ignore directives or plugin exclusions.

- Never begin or end an inline code span with a literal space. Use `␣`
  (U+2423 OPEN BOX), so TeX control space is `\␣` and the e-TeX pseudo-file
  trace prefix is `(␣`.
- Keep backticks balanced within each paragraph. Use a double-backtick code span
  for TeX messages containing a literal backtick, such as
  ``You can't use `\eqno' in ... mode``.
- Do not indent fenced-code content beyond its fence.
- Keep each complete `[text](target)` link on one source line.
- Use `_emphasis_`, not `*emphasis*`.

## GitHub Issue Tracker

Use GitHub Issues for task tracking and durable project memory once this checkout
has a GitHub remote. This repository is initially local: do not send Umber4 work
to Umber3's tracker or create an upstream repository without user authorization.
Until a remote exists, do the directly requested work without inventing a local
issue database or Markdown TODO lists. Committed architecture documents remain
the record of design proposals and reviewed contracts.

Resolve the repository from this checkout with
`gh repo view --json nameWithOwner --jq .nameWithOwner`; pass the resolved
`--repo {REPO}` to issue and label commands. Never hardcode Umber3's repository.

```bash
gh issue list --repo {REPO} --state open --label ready --limit 5 --search 'no:assignee -label:in-progress -label:blocked -label:epic sort:created-asc'
gh issue view {NUMBER} --repo {REPO} --comments
gh issue edit {NUMBER} --repo {REPO} --add-assignee @me --add-label in-progress --remove-label ready
gh issue comment {NUMBER} --repo {REPO} --body-file {WRITEBACK_FILE}
gh issue close {NUMBER} --repo {REPO} --reason completed
gh issue edit {NUMBER} --repo {REPO} --remove-label in-progress
```

Use `--body-file` for multiline bodies and comments, with temporary files outside
the checkout. The `coordinate` skill defines readiness, priorities, dependency
links, claims, recovery, and writeback. Create missing workflow labels before use.
A ready issue has no open prerequisites or active claim. Record parent and
prerequisite links in bodies and check their state before dispatch; GitHub does
not infer readiness from those links.
