# Native MVP measurements

The [MVP plan](mvp.md) is implemented. At one million work iterations, native
Umber takes 0.18–0.78 times pdfTeX's median time across all eight core workloads.
This pass cuts time for arguments tokenized from bytes by 20%, raw byte dispatch
by 31%, and repeated/nested parameters by 7% against the previous Umber binary,
timed in the same run. This establishes comparable throughput for the expansion/register slice.
Layout, shipout, formats, checkpoints, and Wasm runtime performance come later.

## Measurement

Measured on 2026-10-06, Linux x86-64, AMD EPYC 7642, pinned to CPU 0:

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
  the previous Umber binary (`d74a61c`). Times include process startup,
  reading the source, initialization, execution, and result output. pdfTeX also
  writes its normal log. Sources are warm in the filesystem cache.

The table shows medians and the full min–max range at one million iterations.
Ratios below 1 favor Umber.

| Workload                       | Umber (s) | pdfTeX (s) | Ratio | Umber range (s) | pdfTeX range (s) |
| ------------------------------ | --------: | ---------: | ----: | --------------- | ---------------- |
| Raw byte dispatch              |     0.257 |      0.588 | 0.437 | 0.244–0.290     | 0.567–0.615      |
| Stored macro bodies            |     0.186 |      0.271 | 0.686 | 0.183–0.199     | 0.262–0.306      |
| Integer assignments            |     0.268 |      0.367 | 0.729 | 0.259–0.271     | 0.351–0.406      |
| Repeated/nested parameters     |     0.499 |      0.638 | 0.782 | 0.495–0.505     | 0.622–0.654      |
| Arguments tokenized from bytes |     0.575 |      0.795 | 0.723 | 0.555–0.594     | 0.778–0.828      |
| Repeated 256-token arguments   |     1.532 |      8.591 | 0.178 | 1.482–1.593     | 8.446–8.720      |
| Overlapping delimiters         |     0.183 |      0.269 | 0.683 | 0.180–0.195     | 0.263–0.287      |
| Local/global grouped writes    |     0.373 |      0.530 | 0.705 | 0.359–0.428     | 0.508–0.579      |

The startup probe takes 1.42 ms in Umber and 4.85 ms in pdfTeX. The growth ratio
between 100,000 and one million iterations ranges from 0.18 to 0.78, so the
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
100,000 iterations. Umber takes 0.729 s versus 0.808 s for the previous binary
and 1.144 s for pdfTeX: 9.8% less time, at 0.637 times pdfTeX. Its seven samples,
identities, and complete source recipe are retained in
[mvp-input-long.json](../benchmarks/results/mvp-input-long.json).

## What changed

The [initial interpreter](../benchmarks/results/mvp-baseline.json) already
approached pdfTeX. [Cursor optimization](../benchmarks/results/mvp-before-windows.json)
and [window-based argument scanning](../benchmarks/results/mvp-before-input.json)
established comparable times across all eight workloads. This pass compares the
window-scanning binary directly in the same run, with its identity and samples
in the current report. At one million iterations:

- Arguments tokenized from bytes: 0.719 s to 0.575 s, 20.0% less time.
- Raw byte dispatch: 0.374 s to 0.257 s, 31.4% less time.
- Repeated/nested parameters: 0.537 s to 0.499 s, 7.1% less time.

The other five workload medians stay within 1.6% of the previous binary, with
overlapping timing ranges. All raw samples and timing ranges are retained in
the report.

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
- One newly scanned token lives directly in its argument packet. Nested capture
  carries that token value, avoiding buffer selection and reference counting.
  A second token or range promotes the first value to normal capture storage.
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

This pass grows `next_expanded` from 1,588 to 1,593 bytes of release code, the
file reader from 998 to 1,141 bytes, and retained argument scanning from 8,948
to 9,734 bytes. The discard specialization occupies 5,478 bytes. Total text grows
about 0.5%, and the retained scanner reserves 64 more stack bytes. Larger code
is a cost to watch with a broader corpus, particularly larger arguments tokenized
from bytes. The contiguous group loops still carry no per-token capture
bookkeeping; the discard specialization performs no packet allocation or
materialization.

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

These counts include startup and the result. Invocation storage follows maximum
live nesting and argument size. Definition storage is append-only within a run;
repeated redefinitions accumulate tokens and headers. Capture-buffer selection
currently scans the small pool. Those are the main remaining storage costs to
revisit as longer sessions, formats, and checkpoints arrive.

## Correctness and reproduction

Fifteen Rust tests cover expansion order, definitions and meaning snapshots,
argument lifetime/repetition, delimiter overlap, conditional operands, numeric
consumption, grouping, lazy character input, and supported failure boundaries.
The shared native oracle compares 100 marked messages and eight rejected programs.
Storage tests include 5,000 nested captures, active definition replacement, and
token ranges crossing the 4,096-token arena chunks. Two independent adversarial
reviews checked semantics, storage validity, generated instructions, and timing
methodology. The portable library builds for `wasm32-unknown-unknown`.

Additional fixtures cover singleton repetition and suppression, mixed-source
two-parameter captures, unused arguments, markers erased or produced by `\edef`,
and paragraph rejection while consuming retained or discarded arguments.
The inline-value fixture also exercises promotion into mixed argument ranges,
frozen categories and current meanings, single-digit number insertion, and
committed/chained superscript translation across category changes.

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
