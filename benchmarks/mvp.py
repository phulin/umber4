"""Same-source, full-process expansion/register benchmarks against INITEX."""

import argparse
import hashlib
import json
from pathlib import Path
import platform
import re
import statistics
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from reference import ROOT, execute, reference

BOOT = r"\catcode123=1 \catcode125=2 \catcode35=6 \catcode94=7\relax" + "\n"
CASES = {
    "raw": ("", r"\advance\count0by1\relax", 1),
    "bodies": (r"\def\leaf{\advance\count0by1\relax}",
               r"\relax\relax\relax\relax\leaf", 1),
    "integers": ("", r"\count1=1234567\relax\advance\count0by1\relax", 1),
    "parameters": (r"\def\take#1{\advance\count0by#1\relax}"
                   r"\def\twice#1{\take{#1}\take{#1}}", r"\twice{3}", 6),
    "delimiters": (r"\def\grab#1aba{\advance\count0by1\relax}", r"\grab aaba", 1),
    "groups": ("", r"{\count1=2\advance\count1by3\relax"
               r"\global\advance\count0by\count1\relax}", 5),
}


def program(case, size):
    setup, body, factor = CASES[case]
    if case == "raw":
        work = (body + "\n") * size
    else:
        work = r"\def\work{" + body + "}\n"
        levels = [r"\work"]
        for index in range(len(str(size)) - 1):
            name = r"\work" + chr(ord("a") + index)
            work += r"\def" + name + "{" + levels[-1] * 10 + "}\n"
            levels.append(name)
        work += "".join(levels[index] * int(digit)
                        for index, digit in enumerate(reversed(str(size)))) + "\n"
    source = BOOT + setup + "\n" + work
    source += r"\message{UMBER:\the\count0,\the\count1:END}\end" + "\n"
    expected = f"{factor * size},{1234567 if case == 'integers' else 0}".encode()
    return source.encode(), expected


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--umber", type=Path, default=ROOT / "target/release/umber")
    parser.add_argument("--pdftex")
    parser.add_argument("--cases", nargs="+", choices=CASES, default=list(CASES))
    parser.add_argument("--sizes", nargs="+", type=int, default=[10000, 100000, 1000000])
    parser.add_argument("--repeats", type=int, default=7)
    parser.add_argument("--report", type=Path, default=ROOT / "target/benchmark/results.json")
    args = parser.parse_args()
    output = ROOT / "target/benchmark"
    command, env = reference(args.pdftex, output / "pdftex")
    engines = {"umber": ([str(args.umber.resolve())], None), "pdftex": (command, env)}
    version = subprocess.check_output([command[0], "--version"], env=env).decode().splitlines()[0]
    report = {"pdftex": version, "rustc": subprocess.check_output(["rustc", "--version"]).decode().strip(),
              "machine": platform.platform(), "repeats": args.repeats, "samples": []}
    for case in args.cases:
        for size in args.sizes:
            source, expected = program(case, size)
            path = output / f"{case}-{size}.tex"
            path.write_bytes(source)
            timings = {name: [] for name in engines}
            for repeat in range(args.repeats + 1):
                order = ["umber", "pdftex"] if repeat % 2 else ["pdftex", "umber"]
                for name in order:
                    cmd, engine_env = engines[name]
                    start = time.perf_counter_ns()
                    result = execute(cmd, path, engine_env)
                    elapsed = (time.perf_counter_ns() - start) / 1e9
                    checksum = re.findall(rb"UMBER:(.*?):END", result)
                    if checksum != [expected]:
                        raise SystemExit(f"{case}/{size} {name}: {checksum!r}, expected {expected!r}")
                    if repeat:
                        timings[name].append(elapsed)
            medians = {name: statistics.median(values) for name, values in timings.items()}
            row = {"case": case, "size": size, "bytes": len(source),
                   "sha256": hashlib.sha256(source).hexdigest(), "seconds": timings,
                   "median": medians, "ratio": medians["umber"] / medians["pdftex"]}
            report["samples"].append(row)
            print(f"{case:12} {size:9}  umber {medians['umber']:.6f}s  "
                  f"pdftex {medians['pdftex']:.6f}s  ratio {row['ratio']:.3f}", flush=True)
            args.report.parent.mkdir(parents=True, exist_ok=True)
            args.report.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
