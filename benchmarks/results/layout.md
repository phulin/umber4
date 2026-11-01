# Paragraph, page, and checkpoint measurements

## Current cumulative milestone

Measured on 2026-11-01 at `03148ba`, ordinary default-feature release with fat LTO,
one codegen unit, and rustc 1.98.1 on the same AMD EPYC 7642 host. The
[current raw result](layout-current.json) retains nine rotating samples after one
warmup, all run sequentially on CPU 3. The original `930cc4d` executable matches
the initial page-capable binary below. The three fixed prose source hashes are
unchanged; inputs were not regenerated from revised documents. Mixed uses the
existing local Gentle generator output with three AE fonts.

| Case | Original / current / native CPU (ms) | Current vs original | Current/native |
| ---- | ------------------------------------ | ------------------- | -------------- |
| Book | 69.608 / 48.397 / 47.421 | -30.5% | 1.021 |
| Narrow | 69.878 / 49.128 / 49.958 | -29.7% | 0.983 |
| Dense | 63.264 / 43.602 / 41.464 | -31.1% | 1.052 |
| Mixed | — / 47.438 / 40.890 | — | 1.160 |

All four current outputs match independent native DVI glyph/font/coordinate and
rule records exactly; mixed has 106 pages, 214,500 glyphs, and 300 rules. Complete
output signatures match the pre-metadata adapter, and retained-root checkpoint
replays equal uninterrupted execution. The fixed cases also directly match the
preserved original adapter's glyph coordinates and paragraph/page counts.

Separate current book/mixed Callgrind totals are 318.255/322.279 million
instructions. Book is down 34.4% from the original 485.129 million. Inclusive
character-construction paths account for 29.9%/28.6%, active-list search for
13.9%/8.2%, and page work for 18.8%/19.4%. These paths overlap and are instruction
shares, not CPU-time shares. Remaining costs span node operations, exact search,
shaping, packing, and detached output; this attribution does not assign the entire
remaining native gap to one phase.

Native suppresses underfull diagnostics through startup settings only and writes
DVI/log output; Umber retains detached artifacts through completion and releases
them. Every fixture emits its final counter message. The existing late-book
suffix audit finds six requests for 32,205 bytes: output-header backing 17,712,
node backing 14,360, message growth 8+16, and owned message publication 13+96.
Restore itself allocates zero; the node pool reuses backing on 114 of 115 takes within its
bounded 16-chunk cache. These are audited boundary facts, not a new allocation run
or a cache-sizing recommendation.

This bounded cumulative pass places the original three text cases near quiet
native CPU time and retains a 16.0% mixed-workload gap. Short shared-machine
measurements do not establish broad TeX/LaTeX or backend parity. Earlier
expansion-only timings cover a different feature scope and cannot establish a
layout regression. Original measurements and focused change reports remain below
and in the [benchmark guide](../README.md).

## Initial page-capable baseline

Measured on 2026-10-31 at engine revision `159ebe5f114ce6621c59909f2afe9f8fb1f90a00`,
using ordinary `cargo build --release` with default features, fat LTO, one codegen
unit, no extra Rust flags, and rustc 1.98.1. The native baseline is pdfTeX
1.40.25 (TeX Live 2023/Debian), running INITEX with e-TeX. The machine is Linux
6.8, x86-64, AMD EPYC 7642; sequential processes were pinned to allowed CPU 0.
The [raw report](layout-native.json) retains binary/source
hashes, all samples, geometry, allocation phases, and runtime identities.

This is a bounded measurement of the implemented text slice, after paragraph,
page, detached-resource, and checkpoint correctness validation. No interpreter
optimization or architecture changes accompanied this pass.

The tables below retain that initial baseline. The [dense-metric follow-up](#dense-scaled-metrics-follow-up)
compares its preserved executables with the first measured layout optimization
on exactly the same input files.

## Prose and native comparison

The [runner](../layout.py) extracts 194 distinct prose paragraphs from
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

## Dense scaled metrics follow-up

Measured on 2026-11-01, comparing preserved baseline executables from `930cc4d`
(the same engine binary as the initial `159ebe5` report) with `02c29ec`.
The [raw comparison](layout-dense-metrics.json) records
binary and input hashes, every sample, allocation stages, checkpoint results,
and instruction attribution. Both use ordinary release builds with the settings
and toolchain above. No instrumentation executes in timing binaries.

The original three 600-paragraph input files were reused without regeneration;
their hashes equal the initial report. This matters because the runner extracts
prose from documentation that has since changed. Processes ran sequentially on
CPU 0, rotating baseline, new engine, and native order after one warmup, with nine
measured samples per engine and case. Native again suppresses underfull diagnostics
through startup settings only. Umber retains detached output; pdfTeX writes DVI
and a log. This is still a bounded INITEX text comparison, not backend parity.

| Case | Baseline CPU (ms) | Dense metrics CPU (ms) | Change | Native CPU (ms) | New/native |
| ---- | ----------------- | ---------------------- | ------ | --------------- | ---------- |
| Book | 70.669 | 59.290 | -16.1% | 48.317 | 1.227 |
| Narrow | 70.247 | 59.069 | -15.9% | 49.948 | 1.183 |
| Dense | 62.790 | 52.366 | -16.6% | 41.595 | 1.259 |

Median wall times before/after/native were 70.936/59.624/48.778 ms,
70.693/59.415/50.360 ms, and 63.097/52.716/41.954 ms. Shared-machine CPU ranges
for the new engine were 57.748–64.122, 56.909–66.562, and 50.805–53.821 ms;
the raw report retains all old/native ranges too. The improvement appears in
every case; the short timings do not establish a universal speedup.

Old and new inspection results agreed exactly on paragraph/page/box counts and
glyph character, coordinates, and width. Every page and glyph character/x/y also
matched native DVI through the existing independent reader and raw-TFM scaling.
The checkpoint adapters separately check complete boxes, placements, penalties,
counts, messages, and font resources against uninterrupted execution.

### Attribution and change

Hardware sampling was unavailable (`perf_event_paranoid=4`). Valgrind 3.22.0
Callgrind profiled the actual release CLI on each existing input, separately from
timing. Its instruction counts describe executed work, not CPU-time shares.
Before the change, the complete `character_metrics` path accounted for
26.4–26.7% of instructions. Book made 966,804 metric calls, about 4.25 per
emitted glyph, across character construction, paragraph passes, packing, and
page lowering. Its instruction shares were 6.17% document-font lookup, 6.78%
catalog existence checks and repeated descriptor resolution, and 13.75% metric
table decoding and four dimension gathers.

The change replaces compressed scaled character-dimension tables with dense
immutable records in the existing typed pools. It retains separate kern and
baseline-parameter arrays and a presence bitmap, preserving absent-glyph field
queries independently of glyph existence. Character construction uses its already
resolved scaled-font ID for existence checks. Engine and detached-resource
lookups share the immutable representation; nodes do not acquire metric copies
or additional owners.

| Case | Instructions before/after (million) | Change | Metric-path instructions before/after (million) |
| ---- | ----------------------------------- | ------ | ---------------------------------------------- |
| Book | 485.129 / 401.112 | -17.3% | 129.552 / 45.050 |
| Narrow | 477.083 / 395.117 | -17.2% | 125.870 / 43.374 |
| Dense | 480.750 / 397.942 | -17.2% | 127.959 / 44.325 |

The residual book profile has 44.274 million instructions in active-list search
and 37.610 million in the principal node-read path; their absolute counts are
unchanged. Remaining work spans character dispatch, node allocation/access/release,
line breaking, packing, and detached page publication. Smaller repeated node
lookups and a paragraph validation traversal remain, but this profile supplies no
similarly clear substantial simplification. It does not attribute the entire
remaining native gap: native phase costs were not profiled. This pass stops after
the measured metric change rather than redesigning nodes or exact line search.

### Storage and checkpoints

Initialized character-metric payload grows from 320 to 4,096 bytes for aer10,
324 to 2,048 for lcmss8, and 124 to 2,048 for icmtt8, plus a 32-byte presence
bitmap per scaled header. These replace compressed character dimensions; raw
TFM bytes, kerns, and baseline parameters retain their separate roles. Pooled
backing reservation differs from initialized payload, so this growth does not
imply higher aggregate requested heap.

| Case | Allocation requests through completion before/after | Peak requested heap before/after (bytes) |
| ---- | --------------------------------------------------- | ---------------------------------------- |
| Book | 541 / 530 | 7,393,832 / 7,383,152 |
| Narrow | 855 / 844 | 7,418,880 / 7,406,104 |
| Dense | 511 / 503 | 6,240,064 / 6,231,512 |

Requested heap excludes allocator headers and host buffers, as above. Separate
RSS samples before/after were 8,320/8,192, 9,472/9,472, and 8,192/8,192 KiB;
these small differences should not be interpreted as precise memory savings.
Counted live heap returned to zero after all owners released in every run.

Book checkpoint capture/release and unchanged-root restore remain around 2 µs,
with zero additional allocations in both warmed 200-operation batches. Median
suffix replay before/after was 57.400/48.118 ms after paragraph 60,
29.872/24.554 ms after 300, and 5.693/4.532 ms after 540. Retained checkpoint
and prefix heap fell from 15,545,613 to 15,534,933 bytes. Fixed late-suffix
replay still makes six requests for 32,205 bytes each; live heap plateaus at
16,290,589 bytes rather than 16,301,269. Retaining five suffix outputs and then
releasing them preserves the same retention behavior with 10,680 fewer requested
live bytes. This change does not resolve the preexisting replay allocations or
partially shared output-chunk retention.

The implementation passed 162 library tests, doctests, all three existing native
font/paragraph/page oracles, rustfmt, release Clippy, a WebAssembly check, and
retained-font Miri coverage before measurement. This report adds same-input output
and replay comparisons, ordinary release timing, and separate allocation/profile
runs. The earlier expansion-only results execute a different feature scope and
cannot establish a layout regression; the initial layout baseline measured newly
implemented font, paragraph, page, and detached-output work.

For reproduction, preserve `target/benchmark/layout/*.tex` from the initial
runner at `930cc4d`, or generate them in a detached checkout of that revision.
Build both revisions with the ordinary release command above, then invoke each
CLI with the same input and `--tfm NAME=PATH`, rotating old/new/quiet-native
processes sequentially. The unchanged example modes `inspect`, `time`,
`alloc-full`, and `alloc` supply the same correctness, checkpoint, and allocation
checks. Running the current runner without preserving those inputs generates a
different documentation-prose corpus.
