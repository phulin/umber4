# Native MVP measurements

The [MVP plan](mvp.md) is implemented. At one million work iterations, native
Umber takes 0.64–1.03 times pdfTeX's median time across all eight core workloads.
The three slightly slower cases are within 3.3%; the other cases are faster.
This establishes comparable throughput for the expansion/register slice.
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
  1,000,000 iterations, alternating engine order. Times include process startup,
  reading the source, initialization, execution, and result output. pdfTeX also
  writes its normal log. Sources are warm in the filesystem cache.

The table shows medians and the full min–max range at one million iterations.
Ratios below 1 favor Umber.

| Workload                     | Umber (s) | pdfTeX (s) | Ratio | Umber range (s) | pdfTeX range (s) |
| ---------------------------- | --------: | ---------: | ----: | --------------- | ---------------- |
| Raw file dispatch            |     0.356 |      0.558 | 0.637 | 0.351–0.366     | 0.552–0.576      |
| Stored macro bodies          |     0.182 |      0.262 | 0.696 | 0.173–0.198     | 0.256–0.281      |
| Integer assignments          |     0.269 |      0.352 | 0.766 | 0.265–0.290     | 0.349–0.367      |
| Repeated/nested parameters   |     0.625 |      0.615 | 1.017 | 0.608–0.640     | 0.609–0.632      |
| Arguments from files         |     0.804 |      0.789 | 1.019 | 0.792–0.835     | 0.772–0.811      |
| Repeated 256-token arguments |     7.738 |      8.349 | 0.927 | 7.666–7.751     | 8.274–8.427      |
| Overlapping delimiters       |     0.274 |      0.266 | 1.033 | 0.268–0.289     | 0.256–0.275      |
| Local/global grouped writes  |     0.363 |      0.516 | 0.703 | 0.355–0.372     | 0.502–0.637      |

The startup probe takes 1.30 ms in Umber and 4.50 ms in pdfTeX. The growth ratio
between 100,000 and one million iterations ranges from 0.64 to 1.05, so the
throughput conclusion also holds when removing the small-case startup advantage.
All timings, source hashes, binary/compiler identities, and generator identity
are retained in [mvp-native.json](../benchmarks/results/mvp-native.json).

An iteration is the leaf work defined by [the generator](../benchmarks/mvp.py).
The repeated-parameter cases substitute `3` twice into integer assignments.
The 256-token case passes the argument twice to a macro that increments a
register and discards the payload; it measures capture and substitution.

## What changed

The [initial interpreter](../benchmarks/results/mvp-baseline.json) already
approached pdfTeX. Optimization improved stored bodies from 0.229 s to 0.182 s,
integers from 0.324 s to 0.269 s, and grouped writes from 0.421 s to 0.363 s.
Short parameter calls remain close to their initial throughput.

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
- Definition tokens and generated numbers are built in destination storage.
  Packet, capture-buffer, and save-stack capacity is reused.

These paths were inspected in the release assembly with `objdump`. Resolved
reads still use a small stack return. This host denies hardware performance
counters, so the evidence is workload timing, assembly, and separate allocation
diagnostics.

The allocation diagnostic covers heap requests during engine construction and
execution, excluding the host's immutable input buffer, Rust stack, and allocator
overhead. Normal CLI execution has no allocation counters. Increasing work
tenfold gives:

| Workload                     | Allocations, 100,000 | Allocations, 1,000,000 | Peak requested heap bytes, 1,000,000 |
| ---------------------------- | -------------------: | ---------------------: | -----------------------------------: |
| Stored bodies                |                  585 |                    586 |                               56,691 |
| Repeated/nested parameters   |                  591 |                    593 |                               57,704 |
| Arguments from files         |                  585 |                    585 |                               56,678 |
| Repeated 256-token arguments |                  593 |                    595 |                               58,136 |

These counts include startup and the result. Invocation storage follows maximum
live nesting and argument size. Definition storage is append-only within a run;
repeated redefinitions accumulate tokens and headers. Capture-buffer selection
currently scans the small pool. Those are the main remaining storage costs to
revisit as longer sessions, formats, and checkpoints arrive.

## Correctness and reproduction

Twelve Rust tests cover expansion order, definitions and meaning snapshots,
argument lifetime/repetition, delimiter overlap, conditional operands, numeric
consumption, grouping, lazy character input, and supported failure boundaries.
The shared native oracle compares 68 marked messages and six rejected programs.
Storage tests include 5,000 nested captures, active definition replacement, and
token ranges crossing the 4,096-token arena chunks. Two independent adversarial
reviews checked semantics, storage validity, generated instructions, and timing
methodology. The portable library builds for `wasm32-unknown-unknown`.

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

The runners accept `--pdftex /path/to/pdftex`. They need the native binary,
not a format or font installation. For this run, Ubuntu Noble's
`texlive-binaries`, `libkpathsea6`, and `libptexenc1` packages
(`2023.20230311.66589-9build3`) were downloaded and extracted under `.tools/pdftex`;
the runner recognizes that checkout-local installation. A normal system pdfTeX
installation works as well.
