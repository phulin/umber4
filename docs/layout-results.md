# Paragraph, page, and checkpoint measurements

Measured on 2026-10-31 at engine revision `159ebe5f114ce6621c59909f2afe9f8fb1f90a00`,
using ordinary `cargo build --release` with default features, fat LTO, one codegen
unit, no extra Rust flags, and rustc 1.98.1. The native baseline is pdfTeX
1.40.25 (TeX Live 2023/Debian), running INITEX with e-TeX. The machine is Linux
6.8, x86-64, AMD EPYC 7642; sequential processes were pinned to allowed CPU 0.
The [raw report](../benchmarks/results/layout-native.json) retains binary/source
hashes, all samples, geometry, allocation phases, and runtime identities.

This is a bounded measurement of the implemented text slice, after paragraph,
page, detached-resource, and checkpoint correctness validation. No interpreter
optimization or architecture changes accompanied this pass.

## Prose and native comparison

The [runner](../benchmarks/layout.py) extracts 194 distinct prose paragraphs from
the repository's architecture, storage, checkpoint, font/text, paragraph, and page
documents. It removes fenced examples, Markdown links/notation, and TeX
metacharacters, and repeats that text sequence to 600 paragraphs, about 271 KB,
per document. Every paragraph is actual repository prose; repetition supplies
document scale without synthetic macro loops. A small paragraph-ending macro
maintains the paragraph counter. Font loading, shaping, line breaking, page
construction, final flushing, and result ownership all execute.

Each input explicitly sets the same font, geometry, baseline/paragraph glue,
paragraph-fill glue, tolerances, and large horizontal/vertical fuzz in both engines.
Fonts are the existing real TFM resources: `ae/aer10`, `latex-fonts/lcmss8`, and
`latex-fonts/icmtt8`, each scaled to 10 pt. INITEX has no hyphenation patterns.
Native-only startup sets `\hbadness=10000` and `\vbadness=10000` to suppress
diagnostic logging; Umber does not implement those diagnostic thresholds. The
typesetting file remains identical. Every page and glyph character/x/y matched
native DVI, using an independent DVI reader and raw-TFM width scaling, before the
initial timings and again for the quiet final comparison.

One warmup precedes five measured samples, alternating process order. CPU is
user plus system time; wall time covers the complete process. Inputs/resources
are warm in the OS cache. Umber's normal CLI reads/registers metrics, constructs
and retains detached pages, then releases them. Native pdfTeX writes DVI and a
log. These endpoints perform different output work; this is not a PDF/backend
comparison or a broad TeX/LaTeX corpus result.

| Case | Width / page height / baseline / paragraph gap (pt) | Pages / text lines / glyphs | CPU Umber / native (ms) | CPU ratio | Umber glyphs / CPU second |
| ---- | -------------------------------------------------- | --------------------------- | ----------------------- | --------- | ------------------------ |
| Book | 360 / 540 / 12 / 0 | 79 / 3,517 / 227,323 | 69.235 / 46.516 | 1.488 | 3.28 million |
| Narrow | 180 / 360 / 12 / 3 | 295 / 8,533 / 227,323 | 67.230 / 49.041 | 1.371 | 3.38 million |
| Dense | 420 / 600 / 10 / 0 | 63 / 3,775 / 228,277 | 62.066 / 43.190 | 1.437 | 3.68 million |

Median wall times were respectively 69.501/46.834, 67.543/49.391, and
62.391/43.557 ms. These are short process measurements on a shared machine:
Umber CPU ranges were 68.300–69.867, 66.226–71.416, and 61.480–68.828 ms;
native ranges were 45.973–48.301, 47.353–49.805, and 40.295–56.046 ms.
The raw report preserves the initial comparison with native underfull logging
(513/4,572/3,091 diagnostics); those CPU ratios were 1.210/1.033/1.233 and
should not be used as the quiet composition comparison.

## Checkpoint capture and resumed suffixes

The uninstrumented `layout` example uses the book input and retains a checkpoint
and old output prefix every ten paragraphs: 60 checkpoints, including unfinished
page state and pending paragraph-ending macro input. It retains the uninterrupted
completed output. Each early/middle/late checkpoint has a warm replay followed by
five samples, retaining all six generated results within that position. Complete
counts, messages, page boxes/glue, placements, penalties, and font bytes/names/sizes
equal a fresh uninterrupted run; older output prefixes remain unchanged.

Capture/release and unchanged-root restore are batches of 200 operations, reported
per operation. Changed-state restore is measured once after completion of the
preceding suffix. Suffix wall time includes execution through completion and
`outcome` publication, after restore; it excludes equality checks. These in-process
latencies omit CLI startup and metric registration, unlike the full-process table.

| Checkpoint after paragraph | Capture + release (µs) | Unchanged-root restore (µs) | Changed-state restore (µs) | Resumed suffix (ms) |
| -------------------------- | ----------------------- | --------------------------- | ------------------------- | ------------------- |
| 60 (10%) | 2.292 | 2.015 | 21.707 | 60.200 |
| 300 (50%) | 2.268 | 1.978 | 15.623 | 30.470 |
| 540 (90%) | 2.122 | 1.858 | 6.325 | 5.789 |

This exercises same-source replay, mutation after capture, restoration, retained
old outputs, and eventual root release. Source edits, input rebinding, resource
changes, browser/Wasm runtime costs, and a production scheduler are outside this
implemented checkpoint API and this measurement.

## Allocation and retention

Separate `layout_allocations` runs count successful system allocation/reallocation
requests and requested payload bytes, including engine construction, metric
registration, execution, and teardown. The immutable host input/TFM buffers,
allocator headers, stack, and OS mappings are excluded. These counters are absent
from both ordinary CLI and checkpoint timing binaries. Requested live heap
includes pooled reserved backing; it does not measure only initialized objects.

| Case | Requests through completion / teardown | Peak requested heap (bytes) | RSS Umber / native (KiB) |
| ---- | -------------------------------------- | --------------------------- | ----------------------- |
| Book | 541 / 553 | 7,393,832 | 8,320 / 6,768 |
| Narrow | 855 / 867 | 7,418,880 | 9,472 / 6,768 |
| Dense | 511 / 523 | 6,240,064 | 8,192 / 6,764 |

RSS is a separate GNU `time` run, whose small post-exec process forks the engine;
this avoids inheriting the inspecting Python parent's high-water RSS. Single RSS
samples are approximate. The retained checkpoints and output prefixes raise book
live requested heap from 7,379,472 to 15,545,613 bytes. The temporary independent
fresh-run comparison reaches the diagnostic's 22,939,445-byte peak.

At the late checkpoint, 200 warmed capture/release operations and 200 warmed
unchanged-root restores each make zero additional system allocation requests.
After initial replay growth, releasing each regenerated 60-paragraph suffix keeps
live requested heap stable at 16,301,269 bytes. The next 20 replays still make
120 requests for 644,100 bytes total: six requests and 32,205 bytes per replay.
Thus retained capacity plateaus here, while allocation requests do not fully
plateau. These aggregate counters do not identify each request's internal source.

Deliberately retaining five regenerated suffix outputs raises live requested heap
to 19,142,702 bytes. Dropping those outputs leaves 19,065,949 bytes while the engine,
checkpoints, and old outputs remain. Pooled page-buffer backing stays reserved,
and shared partially filled page chunks can retain invisible later records; these
counters do not separate those contributions. After releasing all engine,
checkpoint, and output owners, counted live heap is zero in every diagnostic run.

Reproduce with the existing local pdfTeX and TFM corpus setup:

```bash
cargo build --release --bin umber --example layout --example layout_allocations
python3 benchmarks/layout.py --report target/benchmark/layout.json
```

The runner validates native placements and replay equality itself. The helper
changes passed rustfmt, release Clippy with warnings denied, Python compilation,
and whitespace checks. This completed pass supplies measurements; it adds no
benchmark gates and starts no optimization work.
