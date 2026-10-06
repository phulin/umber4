# Native MVP measurements

The [MVP plan](mvp.md) is implemented. At one million work iterations, native
Umber takes 0.17–0.80 times pdfTeX's median time across all eight core workloads.
The argument optimization pass cuts long-argument time by a factor of 5.2 and
short-argument time by about 10% against the previous Umber binary, timed in the
same run. This establishes comparable throughput for the expansion/register slice.
Layout, shipout, formats, checkpoints, and Wasm runtime performance come later.

## Measurement

Measured on 2026-10-06, Linux x86-64, AMD EPYC 7642, pinned to CPU 0:

- pdfTeX 3.141592653-2.6-1.40.25, TeX Live 2023/Debian, using
  `-ini -etex -no-shell-escape -interaction=nonstopmode -halt-on-error`.
- Rust 1.98.1, release profile with thin LTO, one codegen unit, debug info,
  and no additional `RUSTFLAGS`.
- Identical complete source files for both engines, including category-code
  bootstrap, result messages, and `\end`. Every result agrees before its timing
  counts. Most cases use compact macro fanout; raw input and file arguments
  use unrolled source.
- One warmup and seven measured runs per engine at 10,000, 100,000, and
  1,000,000 iterations, rotating and reversing the order of Umber, pdfTeX, and
  the previous Umber binary (`6713186`). Times include process startup,
  reading the source, initialization, execution, and result output. pdfTeX also
  writes its normal log. Sources are warm in the filesystem cache.

The table shows medians and the full min–max range at one million iterations.
Ratios below 1 favor Umber.

| Workload                     | Umber (s) | pdfTeX (s) | Ratio | Umber range (s) | pdfTeX range (s) |
| ---------------------------- | --------: | ---------: | ----: | --------------- | ---------------- |
| Raw file dispatch            |     0.371 |      0.559 | 0.663 | 0.366–0.403     | 0.556–0.582      |
| Stored macro bodies          |     0.196 |      0.268 | 0.731 | 0.180–0.214     | 0.260–0.290      |
| Integer assignments          |     0.273 |      0.364 | 0.750 | 0.268–0.296     | 0.351–0.388      |
| Repeated/nested parameters   |     0.500 |      0.631 | 0.792 | 0.491–0.520     | 0.620–0.635      |
| Arguments from files         |     0.644 |      0.804 | 0.800 | 0.638–0.665     | 0.785–0.822      |
| Repeated 256-token arguments |     1.504 |      8.596 | 0.175 | 1.459–1.585     | 8.476–10.057     |
| Overlapping delimiters       |     0.185 |      0.278 | 0.667 | 0.182–0.221     | 0.267–0.322      |
| Local/global grouped writes  |     0.367 |      0.525 | 0.699 | 0.357–0.409     | 0.517–0.547      |

The startup probe takes 1.57 ms in Umber and 6.00 ms in pdfTeX. The growth ratio
between 100,000 and one million iterations ranges from 0.18 to 0.80, so the
throughput conclusion also holds when removing the small-case startup advantage.
All timings, source hashes, binary/compiler identities, and generator identity
are retained in [mvp-native.json](../benchmarks/results/mvp-native.json).

An iteration is the leaf work defined by [the generator](../benchmarks/mvp.py).
The repeated-parameter cases substitute `3` twice into integer assignments.
The 256-token case passes the argument twice to a macro that increments a
register and discards the payload; it measures capture and substitution.

## What changed

The [initial interpreter](../benchmarks/results/mvp-baseline.json) already
approached pdfTeX. The [previous optimization round](../benchmarks/results/mvp-before-windows.json)
reached comparable times across all eight workloads. This pass compares that
binary directly in the same run, with its identity and samples in the current
report. At one million iterations:

- Repeated 256-token arguments: 7.820 s to 1.504 s, 5.2 times faster.
- Repeated/nested parameters: 0.553 s to 0.500 s, 9.7% less time.
- Arguments from files: 0.723 s to 0.644 s, 10.9% less time.
- Overlapping delimiters: 0.251 s to 0.185 s, 26.3% less time.

Raw input and grouped writes are essentially unchanged. Stored-body median time
increases 4.6%, from 0.188 s to 0.196 s, with overlapping timing ranges. All raw
samples and timing ranges are retained in the report.

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
- One-token substitutions read the existing token and range directly, preserving
  their parent owner until child capture. They need no temporary input source.
- Definitions record whether their body substitutes any parameter. Calls whose
  body uses none still validate their arguments, with capture storage compiled
  out of the scanner. The flag fits existing header padding; a header stays 88 bytes.
- Definition tokens and generated numbers are built in destination storage.
  Packet, capture-buffer, and save-stack capacity is reused.

These paths were inspected in the release assembly with `objdump`. Resolved
reads still use a small stack return. This host denies hardware performance
counters, so the evidence is workload timing, assembly, and separate allocation
diagnostics.

`next_expanded` grows from 1,135 to 1,588 bytes of release code, and retained
argument scanning from 5,889 to 8,948 bytes, alongside a 5,446-byte discard
specialization. Larger code is a cost to watch with a broader corpus. Neither
group loop performs per-token capture bookkeeping, and the discard specialization
contains no packet allocation or materialization calls.

The allocation diagnostic covers heap requests during engine construction and
execution, excluding the host's immutable input buffer, Rust stack, and allocator
overhead. Normal CLI execution has no allocation counters. Increasing work
tenfold gives:

| Workload                     | Allocations, 100,000 | Allocations, 1,000,000 | Peak requested heap bytes, 1,000,000 |
| ---------------------------- | -------------------: | ---------------------: | -----------------------------------: |
| Stored bodies                |                  585 |                    586 |                               56,691 |
| Repeated/nested parameters   |                  591 |                    593 |                               57,704 |
| Arguments from files         |                  585 |                    585 |                               56,678 |
| Repeated 256-token arguments |                  592 |                    594 |                               57,976 |
| Overlapping delimiters       |                  588 |                    589 |                               56,787 |

These counts include startup and the result. Invocation storage follows maximum
live nesting and argument size. Definition storage is append-only within a run;
repeated redefinitions accumulate tokens and headers. Capture-buffer selection
currently scans the small pool. Those are the main remaining storage costs to
revisit as longer sessions, formats, and checkpoints arrive.

## Correctness and reproduction

Fourteen Rust tests cover expansion order, definitions and meaning snapshots,
argument lifetime/repetition, delimiter overlap, conditional operands, numeric
consumption, grouping, lazy character input, and supported failure boundaries.
The shared native oracle compares 88 marked messages and eight rejected programs.
Storage tests include 5,000 nested captures, active definition replacement, and
token ranges crossing the 4,096-token arena chunks. Two independent adversarial
reviews checked semantics, storage validity, generated instructions, and timing
methodology. The portable library builds for `wasm32-unknown-unknown`.

Additional fixtures cover singleton repetition and suppression, mixed-source
two-parameter captures, unused arguments, markers erased or produced by `\edef`,
and paragraph rejection while consuming retained or discarded arguments.

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
