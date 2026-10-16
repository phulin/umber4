# Whole-file lexing under fixed category codes

This document explores a fast path for file input that holds while category codes and
`\endlinechar` stay unchanged. It extends the line lookahead in
[input.md](input.md) section 3 and reports a measured prototype. The engine
does not use the prototype. Sections 1–4 describe and measure what is built.
Sections 5–7 are proposals and open questions.

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

Section 3 of [input.md](input.md) lexes ahead within the prepared line and rewinds
on any category write. The whole-file pass could replace the per-line buffer with
chunks of lines under a _regime_: the category table and `\endlinechar` in
force. Each chunk records its starting byte offset, its token range per line,
and the regime it assumed. A line starts in state N, so line boundaries are
natural rewind points. A rewind inside a line relexes that line from its start
under the old table to recover the position, which costs at most one line.

The committed path remains the reference. The stops in input.md section 3
become per-line fallbacks or markers:

- A translated `^^` line is lexed lazily when reached.
- An invalid character becomes a marker whose error is raised on consumption.
- A missed name becomes a marker for its spelling range and precomputed hash, and
  is interned on consumption.

### 5.2 Names

A lookahead pass must not insert names, so that IDs follow consumption order and
a capacity failure occurs at the same token. It may look up names that already
exist. Their IDs are stable, so those tokens remain valid after any later
regime change. The prototype inserts while lexing; that is equivalent here only
because every token is consumed in lex order. Lookups cost as much as stage 1
(section 3.3), so a faster lookup path pays off in both designs. Options are an
inlined probe for short names and the `Names` table using the 64-bit hash that
the pass already computed.

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
- **The worker never interns.** It emits spelling markers with hashes, and the
  main thread performs lookups. Concurrent reads of a table that the main thread
  is growing would need a concurrent name table, which is out of scope.
- **Amdahl.** The main thread keeps name lookup (14–17% of `avx2` instructions
  on macro code), regime checks, and consumption, which it performs in any
  design. A worker could therefore move at most about 83–86% of the lexing
  instructions off the main thread, after a single-threaded pass has already
  made them 1.3–2.9× cheaper.
- **WebAssembly.** Wasm threads need shared memory, which browsers allow only
  in [cross-origin isolated](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/SharedArrayBuffer#security_requirements)
  pages, so the portable path must not depend on a worker. Stage 1 ports to
  [Wasm SIMD](https://github.com/WebAssembly/simd/blob/main/proposals/simd/SIMD.md)
  with `i8x16.swizzle`, which returns zero for indices of 16 or more, and
  `i8x16.bitmask`.

A worker pays off most when its results are retained: lexed chunks keyed by
resource identity and regime could be reused after checkpoint restore and across
incremental runs, provided that their name markers are resolved after the restored
name table. That benefit belongs to [checkpoints.md](checkpoints.md) and
needs its own measurements.

## 7. Recommendations and next steps

1. Land input.md section 3's line lookahead first. In this prototype, reading
   lines in place without a per-token call is the largest single gain.
2. Adopt raw-byte line lexing and the word hash on the lookahead path. Both are
   portable, need no unsafe code beyond the bounded output writes, and give
   1.25–1.78× here.
3. Add stage 1 as an optional, target-specific accelerator behind the same
   interface, with the scalar loops as the portable reference. Its gain is large
   on prose and moderate on macro code.
4. Before taking cross-line speculation or a thread further, measure lexing's
   share of end-to-end engine time on real documents and catcode-write
   frequency.

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
