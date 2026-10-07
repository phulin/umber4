# Native expansion benchmarks

`mvp.py` retains the original eight core workloads. `expansion.py` exercises
seven larger macro workloads, including excerpts from Michael Doob's
[gentle.tex](https://ctan.net/info/gentle/gentle.tex). Both runners execute the
same complete input in Umber and native `pdftex -ini -etex`, validate every run,
alternate engine order, pin a CPU on Linux, and retain raw timings and identities.

```bash
cargo build --release
python3 benchmarks/mvp.py --report target/benchmark/core.json
python3 benchmarks/expansion.py --report target/benchmark/expansion.json
```

Use `--cases`, `--sizes`, and `--repeats` for focused runs. Both accept
`--pdftex /path/to/pdftex` and `--baseline /path/to/previous/umber`; the expanded
suite's baseline must support its primitive set. Default reports live under
`target/`. Reviewed measurements live in `results/`.

| Expanded workload    | Measured behavior                                                                                                                                                |
| -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gentle-text`        | Original spelling switches/aliases and `\talks` template, varied names and both branches of every spelling definition.                                           |
| `gentle-calendar`    | Original `\today`, all twelve month branches, varied days and years.                                                                                             |
| `gentle-contents`    | Original contents write payload evaluated in `\edef`; positive and negative page numbers, varied expanded titles, string/roman conversion and detokenization.    |
| `gentle-records`     | Original bracketed-title and paragraph-delimited signature; adapted consumer retains and expands both full payloads, including nested groups.                    |
| `token-pipeline`     | Shared token-register values, group restoration, a deferred assignment hook, opaque list insertion into `\edef`, and later expansion after a field redefinition. |
| `name-table`         | 64–4,096 dynamically defined names, defined/missing tests, lookups, and periodic definition replacement.                                                         |
| `optional-arguments` | `\futurelet`/`\unless` dispatch between optional and default arguments, delimiter matching, integer expressions, and source/stored argument routes.              |

These kernels consume text in expanded definitions. Fonts, boxes, mathematics,
paragraph building, and output are outside the measured slice. The full manual
requires those subsystems. There are no placeholder typesetting primitives.

Every expanded record is compared with independently generated expected tokens
using `\ifx`. Contents serialization compares detokenized text; dedicated native
fixtures separately check conversion categories. A record increments the final
checksum only after equality succeeds. Expected-definition construction and
comparison are included in timing. Payloads vary from 8 to 1,024 characters;
`expanded_text_bytes` counts final validated text, not intermediate storage or
total interpreter work. Umber's replaced definitions and token lists currently
accumulate in its session arena, so allocation diagnostics matter too.

The selected definitions are retained in
[gentle-kernels.tex](corpus/gentle-kernels.tex). [gentle.json](corpus/gentle.json)
records the original URL, SHA-256, exact line ranges, and each adaptation.
The generators work offline. To inspect the original source separately:

```bash
mkdir -p .tools/corpus
curl -fL https://ctan.net/info/gentle/gentle.tex -o .tools/corpus/gentle.tex
sha256sum .tools/corpus/gentle.tex
```

Expected source hash:
`b3e1acf4a6feb6a13b0ace279a5be7f1546ac2198452188f6fb22b4eb590ba3f`.
Each report also records the selected-kernel, generator, measurement-runner,
generated-input, and executable hashes. Exact input files remain under
`target/benchmark/expansion/` after running the generator.

Measure allocations with the separate adapter; its counters stay out of timing:

```bash
cargo build --release --example allocations
target/release/examples/allocations target/benchmark/expansion/gentle-records-50000.tex
```

This counts requests during engine construction and execution. Host input bytes,
Rust stack, allocator overhead, and native pdfTeX memory are outside that counter.
