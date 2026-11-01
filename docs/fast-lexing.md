# Whole-file lexing under fixed category codes

This document explores a fast path for file input that holds while category codes and
`\endlinechar` stay unchanged. It explores alternatives to the lazy reader in
[input.md](input.md) section 3 and reports a measured prototype. The engine
does not use the prototype. Sections 1–4 describe and measure what is built.
Sections 5–7 are experiments and open questions, not required runtime architecture.
Line-local batching and portable hashing come first. Cross-line caches and a lexing
thread remain deferred until full-engine profiles justify their complexity.

## 1. What a fixed table allows

Each prepared line starts in state N. Comments, `^^` translation, and control
words end within their line. A control word can take a letter end-of-line
character, but that character is part of the prepared line. Given the category
table and `\endlinechar`, a line's tokens therefore depend only on its bytes.
Name identities are the exception: they depend on the order in which names
enter the table (section 5.2). Lines are independent lexing units, so a fixed
table lets a reader lex a whole file in one pass, in any order, or on another
thread.

A fixed-table pass can produce exactly the tokens that the lazy reader would
return. Two rare cases need the prepared line rather than the raw bytes: a `^^`
pair that the reader would translate, since translation rewrites the line, and a
control word that continues into a letter end-of-line character, which exists
only in the prepared line. Two side effects must also keep their places in
consumption order: the error for an invalid character and the insertion of a new
name (section 5.2).

## 2. Prototype

`crates/umber/src/lexing/` builds only with the `lexing-prototype` feature. It
lexes a complete file under one `Table` (256 categories and an end-of-line
value) and appends `u32` tokens. Five lexers share one interface:

| Lexer        | Lines and dispatch                    | Runs                      | Name hash |
| ------------ | ------------------------------------- | ------------------------- | --------- |
| `current`    | `FileInput::read`, one call per token | none                      | FNV-1a    |
| `scalar-fnv` | whole file, lines read in place       | byte loop                 | FNV-1a    |
| `scalar`     | whole file, lines read in place       | byte loop                 | word      |
| `avx2-fnv`   | whole file, lines read in place       | stage-1 masks, AVX2 write | FNV-1a    |
| `avx2`       | whole file, lines read in place       | stage-1 masks, AVX2 write | word      |

The current reader's invalid-character error becomes an `INVALID` marker so
that a comparison can continue past it.

### 2.1 Source and lines

A `Source` holds the file bytes followed by 64 zero bytes. Word and vector loads
that start in the text therefore stay in the allocation. A host adapter could
allocate resource buffers with this padding and avoid a copy. The pass finds
each terminator with `memchr2`, folds `\r\n` as the loader does, and strips
trailing byte-32 spaces. It then lexes `[start, content_end)` in the source,
followed by a virtual end-of-line character. No line is copied unless it falls
back (section 2.5).

### 2.2 Stage 1: classification masks

The AVX2 lexers first classify the whole file into bitmasks with one bit per
byte. This follows the two-stage structure of
[simdjson](https://arxiv.org/abs/1902.08318). A category table is arbitrary, so
each class is an arbitrary 256-byte set. The prototype tests membership with two
`pshufb` lookups indexed by the low nibble: one table for bytes below 128 and
one for the rest, selected by the high bit. Each table entry holds one bit per
remaining high-nibble value. A test costs two shuffles, a blend, an `and`, a
compare, and a mask extraction per 32 bytes. Its tables are rebuilt for each
category table at negligible cost. The prototype keeps four masks:

- run bytes, defined below;
- category-11 bytes, which measure control words;
- special run bytes, those other than letters and other characters;
- category-9 and category-10 bytes.

A _run byte_ yields exactly one token and leaves state M: a character of
category 1–4, 6, 8, 11, or 12, or an active character. A byte-32 space of category
10 between two run bytes is also a run byte. It follows state M, so it yields one
space token, and the next byte restores state M. Inside a run, every byte is
therefore one token, and run lengths come from `trailing_ones` over the mask.
The masks carry state across 64-bit words with one word of lookahead.

### 2.3 Stage 2: events and runs

The second stage is a scalar state machine over line positions. At a run byte,
it writes the whole run. Runs of eight or more bytes are written eight tokens at
a time: zero-extend eight bytes to `u32`, then form each category byte with
XOR from the letter mask and a space comparison. Special bytes are patched
afterwards from a 256-entry token table, one store per set bit. Shorter runs are
written from that table by a scalar loop. Under `\ExplSyntaxOn` most braces are
runs of one, and there the vector write cost more than it saved. After a
category-10 space, the stage skips all following category-9 and category-10
bytes in one mask count, because they cannot change the state or produce a
token. The scalar lexers take the same steps with byte loops. Escapes,
end-of-line characters, comments, ignored and invalid characters, the first space
after a non-run byte, and category-7 characters remain individual events.

### 2.4 Control words and the word hash

A control word's length is a count of letters after its first one, which the
AVX2 lexers take from the letter mask. The pass then hashes the name in place and probes the existing `Names` table. FNV-1a
costs one dependent multiply per byte. The word hash loads one masked
little-endian word for names of up to eight bytes, or two overlapping words for up
to sixteen. It folds them with one 64×64→128-bit multiply. Longer names loop over
16-byte blocks. The padded form reads past the name and must equal the
exact-slice form; a test checks this for every length up to 80 at eight alignments.
`Names` uses only the low 32 bits of a hash. Name identities depend only on
insertion order, so lexers with different hashes return identical tokens.

### 2.5 Fallback lines

A line falls back to a prepared-line reader in two cases: a `^^` pair the
reader would translate, or a control word that ends at the content end before a
letter end-of-line character. The fast path stops at the first such token, and
the line's content plus end-of-line character is copied to a scratch buffer.
That reader mirrors `FileInput::read` from the recorded position and state.
Fallbacks covered 0–0.8% of corpus lines (table below).

### 2.6 Output bound

Every token consumes at least one source byte, except the one end-of-line
character per line. Every line but the last consumes a terminator byte, which
yields no token. A file of `n` bytes therefore yields at most `n + 1` tokens. The
pass reserves that bound plus eight cells once, then writes without capacity
checks; a vector write may store up to seven cells past a run's end. Debug
builds assert the bound at every write.

### 2.7 Exactness evidence

`lexing/tests.rs` compares every available lexer with the current reader on:

- every MVP fixture under each of four tables (INITEX, plain, package with
  `@` as a letter, and `\ExplSyntaxOn`);
- 34 line-edge texts under 4 tables × 11 end-of-line values;
- 4,000 random texts with random category mutations and end-of-line values;
- long runs that cross mask words.

A comparison requires equal token vectors and equal name counts, both cold and
on a warm rerun. Disabling the special-byte patch fails four of the five
tests. The benchmark runner repeats the comparison on every corpus file before
timing. Every lexer matches the current reader on all 384 files.

## 3. Measurements

### 3.1 Setup

- Processor: AMD EPYC 7642 (Zen 2), with the runner pinned to CPU 5 by `taskset`.
- Toolchain and build: rustc 1.98.1 with the workspace release profile (fat
  LTO, one codegen unit).
- Corpus: Ubuntu noble's `texlive-latex-base`, `texlive-latex-recommended`, and
  `texlive-pictures` 2023.20240207-1, plus Doob's
  [gentle.tex](https://ctan.net/info/gentle/gentle.tex).

[fast-lexing.json](../benchmarks/results/fast-lexing.json) retains every
sample, the token mix, and each file's path, size, and FNV-1a hash.

| Case       | Table   | Source                                                    | Files |   KiB |  Tokens | Bytes/token |  Lines | Fallbacks |
| ---------- | ------- | --------------------------------------------------------- | ----: | ----: | ------: | ----------: | -----: | --------: |
| `kernel`   | package | `latex/base/latex.ltx`                                    |     1 |   597 | 255,827 |        2.39 | 18,513 |       148 |
| `expl3`    | expl3   | `latex/l3kernel/expl3-code.tex`                           |     1 | 1,229 | 235,782 |        5.34 | 36,765 |       109 |
| `packages` | package | `latex/{amsmath,hyperref,listings,tools,graphics,beamer}` |   196 | 1,967 | 736,235 |        2.74 | 58,755 |       235 |
| `pgf`      | package | `generic/pgf`                                             |   185 | 3,259 | 682,792 |        4.89 | 98,543 |        26 |
| `gentle`   | plain   | `gentle.tex`                                              |     1 |   195 | 150,575 |        1.33 |  5,397 |         0 |

Paths are relative to `texmf-dist/tex`. Directory cases include their `.tex`,
`.sty`, `.cls`, and `.ltx` files. Each case is lexed under one fixed table, so
the token streams are the one table's view of each file, not TeX's view.

### 3.2 Time

The runner times 21 repetitions per case, rotating the lexer order. _Warm_ runs
reuse the name table from verification, so every lookup hits. _Cold_ runs start
each repetition with an empty table, so first occurrences insert names.

| Case       | `current` ms | `scalar-fnv` | `scalar` | `avx2-fnv` | `avx2` | `avx2` ms | `avx2` ns/token | `avx2` cold |
| ---------- | -----------: | -----------: | -------: | ---------: | -----: | --------: | --------------: | ----------: |
| `kernel`   |        4.124 |        1.38× |    1.51× |      1.65× |  1.85× |     2.225 |            8.70 |       1.74× |
| `expl3`    |        6.296 |        1.12× |    1.25× |      1.16× |  1.32× |     4.769 |           20.22 |       1.33× |
| `packages` |       11.864 |        1.33× |    1.44× |      1.59× |  1.74× |     6.819 |            9.26 |       1.69× |
| `pgf`      |       15.459 |        1.27× |    1.39× |      1.46× |  1.59× |     9.740 |           14.26 |       1.56× |
| `gentle`   |        1.847 |        1.75× |    1.78× |      2.73× |  2.90× |     0.636 |            4.23 |       2.88× |

The speedup columns compare warm medians against `current`; the cold column
compares cold medians. On this shared host, repeated runs vary by about 5%.

The token mix explains part of the spread. Character tokens are 74–82% of every
case, and multi-letter control words are 4.4% of `gentle`, 11.9–13.7% of
`kernel` and `packages`, and 19.2–20.7% of `expl3` and `pgf`.

### 3.3 Instructions

Callgrind counts the instructions of three lexes (verification, warm, and
cold), averaged per token:

| Case     | `current` | `avx2` | classify | run write | names | `memchr2` | long hash | fallback |
| -------- | --------: | -----: | -------: | --------: | ----: | --------: | --------: | -------: |
| `kernel` |      79.9 |   58.2 |      6.6 |       5.8 |   9.4 |       4.8 |       0.3 |      0.4 |
| `expl3`  |     142.2 |  128.4 |     14.7 |       2.5 |  18.3 |      10.6 |       2.7 |      0.3 |
| `pgf`    |     121.9 |   95.2 |     16.2 |       4.4 |  15.8 |       9.6 |       1.4 |      0.0 |
| `gentle` |      48.6 |   27.5 |      3.7 |       4.2 |   2.7 |       2.4 |       0.0 |      0.0 |

The component columns break down `avx2`. The remainder, 31, 79, 48, and 15
instructions per token, is the stage-2 state machine, including line handling
and short-name hashing. The current reader's line loading costs 6.7–28.6
instructions per token. That includes the same `memchr2` search; the copy into
its prepared buffer is what the whole-file lexers avoid.

Wall time improves more than instruction counts do: `avx2` runs 27% fewer
instructions than `current` on `kernel` but takes 46% less time. Fewer
per-token calls, returns, and data-dependent branches probably raise the
instructions per cycle; this host's `perf_event_paranoid` setting of 4 blocked
branch counters.

## 4. What limits the gain

Prose gains 2.9× because its runs are long: words, single spaces, and
punctuation write as single vector runs, and few control words need lookups.
Macro code gains 1.3–1.85× because its tokens are mostly events and short runs.
The table and the profiles show where its time goes:

- **The stage-2 state machine.** Each escape, ignored space, comment, and line
  end loads a byte and its category, takes a data-dependent jump, and updates
  the state. `expl3` has about 263,000 spaces, about one per token, and its
  table ignores each one as a separate event.
- **Name lookup.** At 9–18 instructions per token, lookup equals or exceeds the
  stage-1 cost. On `expl3` each lookup averages about 96 instructions,
  including the call, probing, comparison of long spellings, and cold
  insertions. Lookup is unchanged from the current reader, so its share grows
  as the rest shrinks.
- **Line splitting.** `memchr2` setup costs about 68 instructions per line on
  `expl3`, whose lines average 34 bytes.
- **Stage 1.** Four masks cost 2.7–3.3 instructions per byte. The cost per
  token is high where comments, indentation, and long names make bytes per
  token high.

The word hash adds 2–14% over FNV-1a, least on `gentle`, which has few control
words. The SIMD stage adds 4–63% over the matching scalar lexer. Reading lines in
place without a call per token gives 12–75% over `current`.

## 5. Integration with the reader (proposal)

### 5.1 Lookahead beyond the line

The reader in [input.md](input.md) section 3 consumes characters lazily, with
narrow current-line operations and no token lookahead buffer. A speculative
whole-file pass could instead prepare chunks of lines under a _regime_: the
category table and `\endlinechar` in force. Each chunk records its starting byte
offset, its token range per line, and the regime it assumed. A line starts in state N, so line boundaries are
natural rewind points. A rewind inside a line relexes that line from its start
under the old table to recover the position, which costs at most one line.

The lazy reader remains the reference. Speculation would need per-line fallbacks
or markers to preserve its consumption order:

- A translated `^^` line is lexed lazily when reached.
- An invalid character becomes a marker whose error is raised on consumption.
- A missed name becomes a marker for its spelling range and precomputed hash, and
  is interned on consumption.

### 5.2 Names

Name IDs are assigned in insertion order, so insertions must follow consumption
order, as on the committed path. A capacity failure then occurs at the same
token, and names lexed ahead but discarded at a regime change never enter the
table. A lookahead pass on the main thread may look up names that already exist.
Their IDs are stable, so those tokens remain valid after any later regime
change. A miss leaves a marker (section 5.1). The prototype inserts while
lexing; that is equivalent here only because every token is consumed in lex
order.

Lookup costs about as much as stage 1 (section 3.3), but most lookups repeat an
earlier one. The retained report's token mix and name counts give:

| Case       | Multi-letter control words | Distinct names | Occurrences per name |
| ---------- | -------------------------: | -------------: | -------------------: |
| `kernel`   |                     30,535 |          2,953 |                 10.3 |
| `expl3`    |                     45,233 |          7,753 |                  5.8 |
| `packages` |                    100,924 |          7,531 |                 13.4 |
| `pgf`      |                    141,640 |          8,811 |                 16.1 |
| `gentle`   |                      6,640 |            390 |                 17.0 |

Distinct names are the table's size after a cold lex of the whole case, less the
514 fixed identities. They include `\par`, which every session interns first,
and a directory case shares one table across its files. A table local to the
lexer can resolve every repeat without the global table, which lets a worker
thread keep most lookups off the main thread (section 6). On a single thread,
local IDs would replace each repeated global probe with a local probe and a
translation, which saves nothing unless the local table is much faster.
Main-thread lookahead should instead make the global lookup cheaper: inline the
probe for short names, and let `Names` use the 64-bit hash that the pass already
computed.

### 5.3 Regime IDs and byte-sensitive invalidation

LaTeX switches between a few recurring regimes, such as `\makeatletter` and
`\ExplSyntaxOn`. Interning complete regimes by content gives small IDs, so
retained chunks can be keyed by (resource range, regime ID). A toggle and its
reversal then reuse the same lexed chunks.

A category write for byte `b` changes the tokens of a line not yet reached only
if:

- the line contains `b`;
- the line contains a `^^` pair, since translation can produce `b`;
- `b` is the end-of-line character, which every line contains.

Other lines keep their tokens. A chunk can retain a 256-bit byte-presence set
per line or per block of lines, so that a category write invalidates only the
affected lines. ``\catcode`\@=11`` would keep every lexed line without `@`. Any
`\endlinechar` write invalidates every line not yet loaded. The current line
still follows input.md section 3.

The design needs measurement against real documents. The open question is how
often lookahead survives catcode writes in a typical preamble, and how much
memory chunks and presence sets cost.

## 6. A lexing thread (proposal)

Under a fixed regime the pass is a pure function of bytes. A native host could
run it on a worker thread that fills a single-producer, single-consumer queue
of chunks ahead of the reader:

- **The reader never waits.** If the next chunk is not ready, or its regime
  does not match, the reader lexes lazily itself and the worker skips ahead.
- **The worker numbers names locally.** It cannot probe the global table while
  the main thread grows it, and it must not insert. Instead it interns each
  control-word spelling in a table of its own, which assigns dense _local IDs_
  in first-occurrence order and stores each spelling once with its 64-bit hash.
  A chunk holds ordinary `u32` tokens whose multi-letter names carry local IDs.
  Single-character and active identities are fixed, so they are already global.
  Each chunk also lists the spellings and hashes of the local IDs it introduces.
  The main thread therefore learns of a local ID before any token uses it, and
  the threads share nothing but the queue. If the local table fills, the worker
  stops and the reader continues lazily.
- **The main thread translates on consumption.** It keeps a dense array from
  local to global IDs, and each multi-letter name leaving a chunk costs one
  array load. At the first consumption of a local ID the entry is empty. The
  main thread then looks the spelling up in the global table with the stored
  hash, inserts it if missing, and fills the entry. Insertions therefore happen
  at the same tokens as on the committed path, and names in chunks discarded at
  a regime change never reach the global table. Local IDs need not be
  reproducible, because nothing outside the worker and the array observes them.
- **Lifetime.** The local table can span every file the worker lexes in a run,
  so a name first seen in one file is a repeat in the next. A table per source
  would make chunks self-contained for retention, at the cost of one first
  lookup per name per file. The session interner is append-only and survives
  checkpoint restore ([storage.md](storage.md) and
  [checkpoints.md](checkpoints.md) section 4), so a filled entry never becomes
  stale within a session.
- **Amdahl.** If the main thread looked up every name, lookup (14–17% of `avx2`
  instructions on macro code) would cap the share of lexing instructions that a
  worker could move at about 83–86%. With local IDs the main thread performs one
  global lookup per distinct name instead of one per occurrence, 5.8–17 times
  fewer in section 5.2's cases. Hashing, probing, and spelling comparison for
  repeats move to the worker. The main thread keeps translation, first lookups,
  regime checks, and consumption, which it performs in any design. The new cap
  depends on the translation and queue costs, which are not yet measured.
- **WebAssembly.** Wasm threads need shared memory, which browsers allow only
  in [cross-origin isolated](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/SharedArrayBuffer#security_requirements)
  pages, so the portable path must not depend on a worker. Stage 1 ports to
  [Wasm SIMD](https://github.com/WebAssembly/simd/blob/main/proposals/simd/SIMD.md)
  with `i8x16.swizzle`, which returns zero for indices of 16 or more, and
  `i8x16.bitmask`.

Three alternatives would give the worker IDs without a local table. Each fails
a requirement:

- **Source positions.** An ID built from a source number and the offset of a
  name's first occurrence needs no coordination. But a control sequence's
  identity is its spelling. The same name in two files needs one ID, which
  occurrence comes first depends on consumption order, and a name built by
  `\csname` has no position. Positions would also make the dense meaning array
  sparse. Within a worker, any numbering that gives each spelling one value
  works, and a dense counter keeps the translation array dense.
- **Hashes.** A 64-bit hash of the spelling needs no table. But it does not fit
  the 24-bit ID of a `u32` token, so tokens would double in size. Every meaning
  lookup, including those for names replayed from macro bodies, would become a
  hash-map probe instead of an array index. Two spellings with equal hashes
  would silently become one control sequence. Detecting that requires comparing
  each new spelling with a stored one, which is the table again.
- **A shared table.** A concurrent table would let the worker look names up
  directly. But it must stay readable while the main thread grows it, and the
  worker's misses must still return to the main thread for insertion.

A worker pays off most when its results are retained: lexed chunks keyed by
resource identity and regime could be reused after checkpoint restore and across
incremental runs. A retained chunk keeps its local IDs along with the local
table and translation array they index. Within a session the array stays valid,
so a restored run translates names it has already resolved without any lookup.
That benefit belongs to [checkpoints.md](checkpoints.md) and needs its own
measurements.

## 7. Integration tradeoffs

The lazy reader in [input.md](input.md) section 3 remains the semantic reference
for any integration. In this prototype, reading lines in place without a
per-token call is the largest single gain. Raw-byte line lexing and the word hash
are portable, need no unsafe code beyond the bounded output writes, and give
1.25–1.78× here.

Stage 1 can remain an optional, target-specific accelerator behind the same
interface, with scalar loops as the portable reference. Its gain is large on
prose and moderate on macro code. Cross-line speculation and worker lexing need
end-to-end measurements of lexing time and catcode-write frequency before their
additional state and coordination are justified.

Untested prototype ideas:

- **Newline mask.** A stage-1 mask of terminators would replace one `memchr2`
  call per line with a bit scan.
- **Compacting runs.** Category-9 bytes and space runs could join runs if the
  writer compacts tokens. AVX2 has no compress instruction
  ([Intel intrinsics guide](https://www.intel.com/content/www/us/en/docs/intrinsics-guide/index.html)),
  but a 256-entry table of 8-lane shuffles would serve. This targets `expl3`'s
  ignored spaces.
- **Cheaper stage 1.** Use fewer masks, or classify incrementally in
  cache-sized blocks just ahead of stage 2 rather than over the whole file.
- **Faster lookup.** Inline the short-name probe and reuse the 64-bit hash.
- **Local IDs on one thread.** Lexing with a local table, then translating in a
  separate pass, would measure translation cost and first lookups before any
  thread exists.

## 8. Reproduction

```bash
mkdir -p .tools/lexing-corpus/debs .tools/lexing-corpus/root
cd .tools/lexing-corpus/debs
apt-get download texlive-latex-base texlive-latex-recommended texlive-pictures
for deb in *.deb; do dpkg-deb -x "$deb" ../root; done
cd ../../..
cargo test --release --lib --features lexing-prototype lexing
cargo build --release --example lexing --features lexing-prototype
T=.tools/lexing-corpus/root/usr/share/texlive/texmf-dist/tex
L=$T/latex
taskset -c 5 target/release/examples/lexing --repeats 21 \
  --report target/benchmark/fast-lexing.json \
  kernel=package:$L/base/latex.ltx expl3=expl3:$L/l3kernel/expl3-code.tex \
  packages=package:$L/amsmath,$L/hyperref,$L/listings,$L/tools,$L/graphics,$L/beamer \
  pgf=package:$T/generic/pgf gentle=plain:.tools/corpus/gentle.tex
```

The measured packages were version 2023.20240207-1, with SHA-256:

- `texlive-latex-base`: `abab46588cb0071216b440e2f7b1daaba57d768b1e8974e42c0b5a12f1c7b1d0`
- `texlive-latex-recommended`: `5dfb6d1082e5eba3a6f7fcc6f0e10c9864fceb4491abbb69d3a9480b908ae75c`
- `texlive-pictures`: `d2c81e3e52d4f53bcf00b43660096018c50c6c1eef7368fa1bdaa13d464d3638`

`gentle.tex` matches the SHA-256 in
[gentle.json](../benchmarks/corpus/gentle.json). The `--lexers` option
restricts the comparison; the first lexer listed is the reference.
