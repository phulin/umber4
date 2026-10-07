# Native MVP measurements

The [MVP plan](mvp.md) is implemented. At one million work iterations, native
Umber takes 0.17–0.70 times pdfTeX's median time across all eight core workloads.
This pass cuts repeated/nested argument time by 21%, integer assignment time by
35%, and grouped write time by 23% against the previous Umber binary, timed in
the same run. This establishes comparable throughput for the expansion/register
slice. Layout, shipout, formats, checkpoints, and Wasm runtime performance come
later.

## Measurement

Measured on 2026-10-07, Linux x86-64, AMD EPYC 7642, pinned to CPU 0:

- pdfTeX 3.141592653-2.6-1.40.25, TeX Live 2023/Debian, using
  `-ini -etex -no-shell-escape -interaction=nonstopmode -halt-on-error`.
- Rust 1.98.1, release profile with thin LTO, one codegen unit, debug info,
  and no additional `RUSTFLAGS`.
- Identical complete source files for each binary, including category-code
  bootstrap, result messages, and `\end`. Every result agrees before its timing
  counts. Most cases use compact macro fanout; raw input and arguments tokenized
  from bytes use unrolled source.
- One warmup and seven measured runs per engine at 10,000, 100,000, and
  1,000,000 iterations, rotating and reversing the order of Umber, pdfTeX, and
  the previous Umber binary (`b781f6a`). Times include process startup,
  reading the source, initialization, execution, and result output. pdfTeX also
  writes its normal log. Sources are warm in the filesystem cache.

The table shows medians and the full min–max range at one million iterations.
Ratios below 1 favor Umber.

| Workload                       | Umber (s) | pdfTeX (s) | Ratio | Umber range (s) | pdfTeX range (s) |
| ------------------------------ | --------: | ---------: | ----: | --------------- | ---------------- |
| Raw byte dispatch              |     0.249 |      0.566 | 0.441 | 0.241–0.317     | 0.556–0.584      |
| Stored macro bodies            |     0.162 |      0.265 | 0.609 | 0.159–0.169     | 0.260–0.301      |
| Integer assignments            |     0.176 |      0.369 | 0.478 | 0.172–0.230     | 0.358–0.425      |
| Repeated/nested parameters     |     0.406 |      0.650 | 0.624 | 0.398–0.419     | 0.619–0.678      |
| Arguments tokenized from bytes |     0.505 |      0.792 | 0.637 | 0.499–0.510     | 0.775–0.876      |
| Repeated 256-token arguments   |     1.440 |      8.612 | 0.167 | 1.403–1.517     | 8.529–9.373      |
| Overlapping delimiters         |     0.188 |      0.268 | 0.700 | 0.185–0.190     | 0.265–0.286      |
| Local/global grouped writes    |     0.294 |      0.532 | 0.552 | 0.289–0.310     | 0.518–0.621      |

The startup probe takes 1.32 ms in Umber and 5.26 ms in pdfTeX. The growth ratio
between 100,000 and one million iterations ranges from 0.17 to 0.71, so the
throughput conclusion also holds when removing the small-case startup advantage.
All timings, source hashes, binary/compiler identities, and generator identity
are retained in [mvp-native.json](../benchmarks/results/mvp-native.json).

An iteration is the leaf work defined by [the generator](../benchmarks/mvp.py).
The repeated-parameter cases substitute `3` twice into integer assignments.
The historical `file-parameters` case tokenizes those arguments from source
bytes; `parameters` reads already-tokenized arguments from stored macro bodies.
The native adapter reads the complete source into memory before execution and
reuses a prepared line buffer. This distinction measures lexing and argument
construction over resident bytes, with full-process input reading still timed.
The 256-token case passes the argument twice to a macro that increments a
register and discards the payload; it measures capture and substitution.

A separate check unrolls that same 256-token argument in source bytes for
100,000 iterations. Umber takes 0.724 s versus 0.753 s for the previous binary
and 1.125 s for pdfTeX: 3.8% less time, at 0.644 times pdfTeX. Its seven samples,
identities, and complete source recipe are retained in
[mvp-numeric-input-long.json](../benchmarks/results/mvp-numeric-input-long.json).

## What changed

Earlier reports retain the [initial interpreter](../benchmarks/results/mvp-baseline.json),
[cursor optimization](../benchmarks/results/mvp-before-windows.json),
[window-based argument scanning](../benchmarks/results/mvp-before-input.json),
and [source-byte optimization](../benchmarks/results/mvp-before-numeric.json).
This pass compares the last binary directly in the same run. At one million
iterations:

- Repeated/nested parameters: 0.515 s to 0.406 s, 21.2% less time.
- Integer assignments: 0.270 s to 0.176 s, 34.7% less time.
- Local/global grouped writes: 0.381 s to 0.294 s, 22.9% less time.
- Arguments tokenized from bytes: 0.567 s to 0.505 s, 10.9% less time.

Stored bodies improve 11.8%, and repeated 256-token arguments improve 6.4%.
Raw dispatch stays within 0.1% of the previous binary. Delimiter time increases
2.0%, with overlapping timing ranges. All samples are retained in the report.

- Resolved reads carry only a token and meaning. Capture coordinates disappear
  from the ordinary stored-token path after compilation.
- Cursors advance one pointer. Argument capture derives coordinates from the
  window when needed; source transitions refresh the next window.
- Meanings and assignment levels occupy an 8-byte binding. Native source records
  occupy 64 bytes, with compact argument indices.
- Parameter-free calls install existing body storage through a small inline
  path. Argument matching remains an ordinary outlined Rust function.
- Contiguous captured ranges extend directly. Backing ownership is retained
  at range transitions; delimiter matching tracks outer groups during its scan.
- Grouped arguments inspect an immutable source window with a local pointer and
  brace depth. They retain one range after scanning it, with parameter and source
  transitions returning to window scanning.
- One-token arguments stored as ranges return the existing token and range, preserving
  their parent owner until child capture. They need no temporary input source.
- One newly scanned token lives directly in its argument packet. A grouped
  capture that finishes with one stored token and an empty destination also
  keeps its value inline. Nested capture carries that identity without buffer
  selection or arena lookups. Larger captures retain their existing ranges;
  appending to an inline value promotes it to normal capture storage.
- Integer scans walk literal digits directly in stored windows after reading
  their first digit. They consume a terminating space and leave an unexpandable
  nonspace terminator at its cursor, preserving identity and live meanings.
  Other reads use ordinary expansion and substitution.
- Delimited matching uses a separate Rust function. The common undelimited
  scanner carries less code and fewer stack locals. Packet reuse has a small
  inline equal-count gate, with resizing and allocation in an outlined helper.
- Ordinary file bytes and control-word letters use inline classification with one
  category load. Only superscript-category bytes enter the shared translation
  routine; line scanning publishes its byte offset once per physical line.
- Buffer publication has an inline presence check. Calls that build no buffer
  continue without entering its ownership routine.
- Definitions record whether their body substitutes any parameter. Calls whose
  body uses none still validate their arguments, with capture storage compiled
  out of the scanner. The flag fits existing header padding; a header stays 88 bytes.
- Definition tokens and generated numbers are built in destination storage.
  Packet, capture-buffer, and save-stack capacity is reused.

These paths were inspected in the release assembly with `objdump`. Resolved
reads still use a small stack return. This host denies hardware performance
counters, so the evidence is workload timing, assembly, and separate allocation
diagnostics.

This pass reduces retained argument scanning from 9,734 to 7,746 bytes of
release code and its stack reservation from 280 to 216 bytes. The discard
specialization shrinks from 5,478 to 4,443 bytes; separate delimiter helpers
occupy 5,204 bytes. Integer scanning grows from 1,753 to 2,070 bytes, with its
24-byte stack reservation unchanged. `next_expanded` and the file reader retain
their sizes. Total executable text grows 0.75%, a cost to watch with a broader
corpus. Group scanning still carries no per-token capture bookkeeping, and
the discard specialization performs no packet allocation or materialization.

The allocation diagnostic covers heap requests during engine construction and
execution, excluding the host's immutable input buffer, Rust stack, and allocator
overhead. Normal CLI execution has no allocation counters. Increasing work
tenfold gives:

| Workload                       | Allocations, 100,000 | Allocations, 1,000,000 | Peak requested heap bytes, 1,000,000 |
| ------------------------------ | -------------------: | ---------------------: | -----------------------------------: |
| Stored bodies                  |                  585 |                    586 |                               56,691 |
| Repeated/nested parameters     |                  591 |                    593 |                               57,704 |
| Arguments tokenized from bytes |                  584 |                    584 |                               56,614 |
| Repeated 256-token arguments   |                  592 |                    594 |                               57,976 |
| Overlapping delimiters         |                  588 |                    589 |                               56,787 |

The diagnostic samples and its binary identity are retained in the native report.
These counts include startup and the result. Invocation storage follows maximum
live nesting and argument size. Definition storage is append-only within a run;
repeated redefinitions accumulate tokens and headers. Capture-buffer selection
currently scans the small pool. Those are the main remaining storage costs to
revisit as longer sessions, formats, and checkpoints arrive.

## Correctness and reproduction

Sixteen Rust tests cover expansion order, definitions and meaning snapshots,
argument lifetime/repetition, delimiter overlap, conditional operands, numeric
consumption, grouping, lazy character input, and supported failure boundaries.
The shared native oracle compares 114 marked messages and ten rejected programs.
Storage tests include 5,000 nested captures, active definition replacement, and
token ranges crossing the 4,096-token arena chunks. Two independent adversarial
reviews checked semantics, storage validity, generated instructions, and timing
methodology. The portable library builds for `wasm32-unknown-unknown`.

Additional fixtures cover singleton repetition and suppression, mixed-source
two-parameter captures, unused arguments, markers erased or produced by `\edef`,
and paragraph rejection while consuming retained or discarded arguments.
The inline-value fixture also exercises promotion into mixed argument ranges,
frozen categories and current meanings, single-digit number insertion, and
committed/chained superscript translation across category changes. It now
covers completed singleton capture from a reusable source buffer and changing
packet arity from nine to one and back. Numeric-window fixtures cover radix and
category distinctions, space aliases, parameter and active-token expansion,
conditional operands, changing terminator meanings, suppression, overflow,
and multi-chunk constants.

```bash
cargo test
cargo test --release
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo build --release
python3 scripts/oracle.py
python3 benchmarks/mvp.py --sizes 10000 100000 1000000 --repeats 7
cargo build --release --lib --target wasm32-unknown-unknown
cargo run --release --example allocations -- target/benchmark/parameters-1000000.tex
```

The benchmark also accepts `--baseline /path/to/previous/umber` to time a previous
binary on identical input in the same run. Its raw samples and binary hash are
included in the report.

The runners accept `--pdftex /path/to/pdftex`. They need the native binary,
not a format or font installation. For this run, Ubuntu Noble's
`texlive-binaries`, `libkpathsea6`, and `libptexenc1` packages
(`2023.20230311.66589-9build3`) were downloaded and extracted under `.tools/pdftex`;
the runner recognizes that checkout-local installation. A normal system pdfTeX
installation works as well.
