# Whole-file lexing under fixed category codes

This document explores a fast path for file input that holds while category codes and
`\endlinechar` stay unchanged. It explores alternatives to the lazy reader in
[input.md](input.md) section 3. The engine does not use the prototype.
Sections 1–2 describe its current contract; sections 3–5 are experiments and open
questions, not required runtime architecture. Historical measurements and corpus
setup are retained in the [lexer report](../benchmarks/results/fast-lexing.md).
Line-local batching and portable hashing come first. Cross-line caches and a lexing
thread remain deferred until full-engine profiles justify their complexity.

## 1. What a fixed table allows

Each prepared line starts in state N. Comments, `^^` translation, and control
words end within their line. A control word can take a letter end-of-line
character, but that character is part of the prepared line. Given the category
table and `\endlinechar`, a line's tokens therefore depend only on its bytes.
Name identities are the exception: they depend on the order in which names
enter the table (section 3.2). Lines are independent lexing units, so a fixed
table lets a reader lex a whole file in one pass, in any order, or on another
thread.

A fixed-table pass can produce exactly the tokens that the lazy reader would
return. Two rare cases need the prepared line rather than the raw bytes: a `^^`
pair that the reader would translate, since translation rewrites the line, and a
control word that continues into a letter end-of-line character, which exists
only in the prepared line. Two side effects must also keep their places in
consumption order: the error for an invalid character and the insertion of a new
name (section 3.2).

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
compare, and a mask extraction per 32 bytes. It rebuilds its tables for each
category table. The prototype keeps four masks:

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
written from that table by a scalar loop. After a category-10 space, the stage
skips all following category-9 and category-10 bytes in one mask count, because
they cannot change the state or produce a token. The scalar lexers take the same
steps with byte loops. Escapes,
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

### 2.6 Output bound

Every token consumes at least one source byte, except the one end-of-line
character per line. Every line but the last consumes a terminator byte, which
yields no token. A file of `n` bytes therefore yields at most `n + 1` tokens. The
pass reserves that bound plus eight cells once, then writes without capacity
checks; a vector write may store up to seven cells past a run's end. Debug
builds assert the bound at every write.

### 2.7 Compatibility checks

`lexing/tests.rs` compares every available lexer with the current reader on:

- every MVP fixture under each of four tables (INITEX, plain, package with
  `@` as a letter, and `\ExplSyntaxOn`);
- 34 line-edge texts under 4 tables × 11 end-of-line values;
- 4,000 random texts with random category mutations and end-of-line values;
- long runs that cross mask words.

A comparison requires equal token vectors and equal name counts, both cold and
on a warm rerun. The benchmark runner repeats the comparison on every corpus
file before timing.

## 3. Integration with the reader (proposal)

### 3.1 Lookahead beyond the line

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

### 3.2 Names

Name IDs are assigned in insertion order, so insertions must follow consumption
order, as on the committed path. A capacity failure then occurs at the same
token, and names lexed ahead but discarded at a regime change never enter the
table. A lookahead pass on the main thread may look up names that already exist.
Their IDs are stable, so those tokens remain valid after any later regime
change. A miss leaves a marker (section 3.1). The prototype inserts while
lexing; that is equivalent here only because every token is consumed in lex
order.

A table local to the lexer can resolve repeated names without the global table,
which lets a worker thread keep those lookups off the main thread (section 4).
On a single thread, local IDs would replace each repeated global probe with a
local probe and a translation, which saves nothing unless the local table is
much faster.
Main-thread lookahead should instead make the global lookup cheaper: inline the
probe for short names, and let `Names` use the 64-bit hash that the pass already
computed.

### 3.3 Regime IDs and byte-sensitive invalidation

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

## 4. A lexing thread (proposal)

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
- **Worker costs.** Local IDs move hashing, probing, and spelling comparison for
  repeated names to the worker. The main thread keeps translation, first global
  lookups, regime checks, and consumption. Translation and queue costs need
  measurement before this design can justify a worker.
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

## 5. Integration tradeoffs

The lazy reader in [input.md](input.md) section 3 remains the semantic reference
for any integration. Raw-byte line lexing and the word hash are portable and
need no unsafe code beyond the bounded output writes.

Stage 1 can remain an optional, target-specific accelerator behind the same
interface, with scalar loops as the portable reference. Cross-line speculation
and worker lexing need end-to-end measurements of lexing time and catcode-write
frequency before their
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

## 6. Measurements and reproduction

The [retained lexer report](../benchmarks/results/fast-lexing.md) contains the
historical corpus, timings, instruction counts, interpretation, and reproduction
commands. These fixed-table measurements do not establish an end-to-end gain
for the current interpreter.
