# Fixed-table lexer prototype measurements

These historical measurements describe the optional whole-file lexer prototype,
not the current interpreter's end-to-end performance. The engine uses the lazy
reader. The [prototype contract](../../docs/fast-lexing.md#2-prototype) describes
the five compared lexers; the [raw report](fast-lexing.json) retains samples,
corpus identities, and token mixes. Cross-line integration and worker lexing
remain proposals.

## Setup

- Processor: AMD EPYC 7642 (Zen 2), with the runner pinned to CPU 5 by `taskset`.
- Toolchain and build: rustc 1.98.1 with the workspace release profile (fat
  LTO, one codegen unit).
- Corpus: Ubuntu noble's `texlive-latex-base`, `texlive-latex-recommended`, and
  `texlive-pictures` 2023.20240207-1, plus Doob's
  [gentle.tex](https://ctan.net/info/gentle/gentle.tex).

[fast-lexing.json](fast-lexing.json) retains every
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

## Time

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

## Instructions

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

## Interpretation

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

## Repeated names and worker estimates

Lookup costs about as much as stage 1 (see Instructions above), but most lookups
repeat an earlier one. The retained report's token mix and name counts give:

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
lexer can resolve every repeat without the global table, as proposed in the
[worker design](../../docs/fast-lexing.md#4-a-lexing-thread-proposal).

If the main thread looked up every name, lookup (14–17% of `avx2` instructions
on macro code) would cap the share of lexing instructions that a worker could
move at about 83–86%. With local IDs the main thread performs one global lookup
per distinct name instead of one per occurrence, 5.8–17 times fewer in the cases
above. Hashing, probing, and spelling comparison for repeats move to the worker.
The main thread keeps translation, first lookups, regime checks, and consumption,
which it performs in any design. The new cap depends on translation and queue
costs, which were not measured.

Reading lines in place and the portable word hash gave 1.25–1.78× speedups in
these cases; they were the largest single gain in the prototype.
Short runs under `\ExplSyntaxOn`, including most braces, used scalar writes
because vector writing cost more than it saved.

## Compatibility checks

The retained comparison matched every lexer with `FileInput::read` on all 384
corpus files, comparing both token vectors and name counts before timing.
Fallbacks covered 0–0.8% of corpus lines. Disabling the special-byte patch failed
four of the five prototype tests.

## Reproduction

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
[gentle.json](../corpus/gentle.json). The `--lexers` option
restricts the comparison; the first lexer listed is the reference.
