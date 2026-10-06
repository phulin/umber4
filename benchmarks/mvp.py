"""Same-source, full-process expansion/register benchmarks against INITEX.

The CLI holds the source bytes in memory before execution. The historical
file-parameters case tokenizes arguments from those bytes; parameters reads
arguments from stored macro tokens. Neither performs per-argument file I/O.
"""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
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
    "file-parameters": (r"\def\take#1{\advance\count0by#1\relax}"
                        r"\def\twice#1{\take{#1}\take{#1}}", r"\twice{3}", 6),
    "long-parameters": (r"\def\take#1{\advance\count0by1\relax}"
                        r"\def\twice#1{\take{#1}\take{#1}}",
                        r"\twice{" + "abcdefgh" * 32 + "}", 2),
    "delimiters": (r"\def\grab#1aba{\advance\count0by1\relax}", r"\grab aaba", 1),
    "groups": ("", r"{\count1=2\advance\count1by3\relax"
               r"\global\advance\count0by\count1\relax}", 5),
}


def program(case, size):
    setup, body, factor = CASES[case]
    if case in ("raw", "file-parameters"):
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


def measure(engines, path, expected, repeats):
    timings = {name: [] for name in engines}
    for repeat in range(repeats + 1):
        order = list(engines)
        if "baseline" in engines:
            offset = repeat % len(order)
            order = order[offset:] + order[:offset]
        if repeat % 2 == 0:
            order.reverse()
        for name in order:
            cmd, engine_env = engines[name]
            start = time.perf_counter_ns()
            result = execute(cmd, path, engine_env)
            elapsed = (time.perf_counter_ns() - start) / 1e9
            checksum = re.findall(rb"UMBER:(.*?):END", result)
            if checksum != [expected]:
                raise SystemExit(f"{path.name} {name}: {checksum!r}, expected {expected!r}")
            if repeat:
                timings[name].append(elapsed)
    medians = {name: statistics.median(values) for name, values in timings.items()}
    result = {"seconds": timings, "median": medians,
              "ratio": medians["umber"] / medians["pdftex"]}
    if "baseline" in engines:
        result["ratio_to_baseline"] = medians["umber"] / medians["baseline"]
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--umber", type=Path, default=ROOT / "target/release/umber")
    parser.add_argument("--pdftex")
    parser.add_argument("--baseline", type=Path,
                        help="previous Umber binary to time in the same run")
    parser.add_argument("--cases", nargs="+", choices=CASES, default=list(CASES))
    parser.add_argument("--sizes", nargs="+", type=int, default=[10000, 100000, 1000000])
    parser.add_argument("--repeats", type=int, default=7)
    parser.add_argument("--cpu", type=int,
                        help="Linux CPU to pin; defaults to the first allowed CPU")
    parser.add_argument("--report", type=Path, default=ROOT / "target/benchmark/results.json")
    args = parser.parse_args()
    if args.repeats < 1 or any(size < 1 or size * 6 > 2147483647 for size in args.sizes):
        parser.error("use positive repeats and sizes whose checksums fit a TeX integer")
    cpu = args.cpu
    if hasattr(os, "sched_setaffinity"):
        cpu = cpu if cpu is not None else min(os.sched_getaffinity(0))
        os.sched_setaffinity(0, {cpu})
    output = ROOT / "target/benchmark"
    command, env = reference(args.pdftex, output / "pdftex")
    engines = {"umber": ([str(args.umber.resolve())], None), "pdftex": (command, env)}
    if args.baseline:
        engines["baseline"] = ([str(args.baseline.resolve())], None)
    version = subprocess.check_output([command[0], "--version"], env=env).decode().splitlines()[0]
    cpu_model = platform.processor()
    if Path("/proc/cpuinfo").is_file():
        cpu_model = next(line.split(":", 1)[1].strip()
                         for line in Path("/proc/cpuinfo").read_text().splitlines()
                         if line.startswith("model name"))
    report = {"date": datetime.now(timezone.utc).isoformat(), "pdftex": version,
              "rustc": subprocess.check_output(["rustc", "--version"]).decode().strip(),
              "rustflags": os.environ.get("RUSTFLAGS", ""),
              "machine": platform.platform(), "cpu": cpu, "cpu_model": cpu_model,
              "umber_binary_sha256": hashlib.sha256(args.umber.read_bytes()).hexdigest(),
              "generator_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "repeats": args.repeats, "complete": False, "samples": []}
    if args.baseline:
        report["baseline_binary_sha256"] = hashlib.sha256(args.baseline.read_bytes()).hexdigest()
    startup = output / "startup.tex"
    startup.write_text(BOOT + r"\message{UMBER:0,0:END}\end" + "\n")
    report["startup"] = measure(engines, startup, b"0,0", args.repeats)
    for case in args.cases:
        for size in args.sizes:
            source, expected = program(case, size)
            path = output / f"{case}-{size}.tex"
            path.write_bytes(source)
            row = {"case": case, "size": size, "bytes": len(source),
                   "sha256": hashlib.sha256(source).hexdigest(),
                   **measure(engines, path, expected, args.repeats)}
            medians = row["median"]
            report["samples"].append(row)
            label = "source args" if case == "file-parameters" else case
            print(f"{label:12} {size:9}  umber {medians['umber']:.6f}s  "
                  f"pdftex {medians['pdftex']:.6f}s  ratio {row['ratio']:.3f}"
                  + (f"  previous {medians['baseline']:.6f}s"
                     f"  ratio {row['ratio_to_baseline']:.3f}" if args.baseline else ""),
                  flush=True)
            args.report.parent.mkdir(parents=True, exist_ok=True)
            args.report.write_text(json.dumps(report, indent=2) + "\n")
    report["complete"] = True
    args.report.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
