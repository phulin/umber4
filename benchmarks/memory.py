"""Same-source ownership workloads with compact input and measured heap peaks.

churn replaces a fixed working set; live-growth retains a new binding each time.
metadata alternates parameter/fallback metadata and multiblock definitions.
live-metadata retains distinct nine-parameter definitions and overlapping delimiters.
All execute their iterations in TeX rather than growing the host input file.
"""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

from mvp import BOOT, ROOT, measure, reference

CASES = ("churn", "live-growth", "metadata", "live-metadata")
PAYLOAD = "abcdefgh " * 8
METADATA_PAYLOAD = (PAYLOAD * 14 + "\n") * 5


def program(case, size):
    if case == "churn":
        work = (r"\edef\actual{\the\count0/" + PAYLOAD + "}"
                r"\let\alias=\actual"
                r"\edef\wanted{\number\count0/" + PAYLOAD + "}"
                r"\ifx\alias\wanted\else\message{UMBER:BAD:END}\end\fi"
                r"\toks0={" + PAYLOAD + "}"
                r"\toks1=\toks0"
                r"{\def\actual{discarded}\toks0={local}\global\toks2=\toks1}"
                r"\edef\actual{\the\toks2}\def\wanted{" + PAYLOAD + "}"
                r"\ifx\actual\wanted\else\message{UMBER:BAD:END}\end\fi")
    elif case == "metadata":
        payload = METADATA_PAYLOAD
        work = (r"\def\actual##1abab##2xyxy##3##4##5##6##7##8##9{##9##1##2}"
                r"\edef\result{\actual AababBxyxyCDEFGHI}\def\small{IAB}"
                r"\ifx\result\small\else\message{UMBER:BAD:END}\end\fi"
                r"\def\actual##1aba{##1}\edef\result{\actual Qaba}\def\small{Q}"
                r"\ifx\result\small\else\message{UMBER:BAD:END}\end\fi"
                r"\def\actual{" + payload + r"}\edef\result{\actual}"
                r"\ifx\result\wanted\else\message{UMBER:BAD:END}\end\fi")
    elif case == "live-metadata":
        work = (r"\expandafter\def\csname field\the\count0\endcsname"
                r"##1abab##2xyxy##3##4##5##6##7##8##9{##9##1##2" + PAYLOAD + "}"
                r"\edef\actual{\csname field\number\count0\endcsname AababBxyxyCDEFGHI}"
                r"\ifx\actual\wanted\else\message{UMBER:BAD:END}\end\fi")
    elif case == "live-growth":
        work = (r"\expandafter\def\csname field\the\count0\endcsname{" + PAYLOAD + "}"
                r"\edef\actual{\csname field\number\count0\endcsname}"
                r"\ifx\actual\wanted\else\message{UMBER:BAD:END}\end\fi")
    else:
        raise ValueError(case)
    wanted = METADATA_PAYLOAD if case == "metadata" else ("IAB" if case == "live-metadata" else "") + PAYLOAD
    source = (BOOT + r"\def\wanted{" + wanted + "}\n"
              + r"\def\step{" + work + r"\advance\count0by1\relax"
              + r"\ifnum\count0<" + str(size) + r" \expandafter\step\fi}"
              + "\n" + r"\step\message{UMBER:\the\count0:END}\end" + "\n")
    return source.encode(), str(size).encode()


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def allocation(binary, path, expected):
    output = subprocess.check_output([str(binary.resolve()), str(path.resolve())], cwd=ROOT)
    if re.findall(rb"UMBER:(.*?):END", output) != [expected]:
        raise RuntimeError("allocation adapter checksum mismatch")
    match = re.search(rb"allocations=(\d+) requested_bytes=(\d+) peak_bytes=(\d+)", output)
    if not match:
        raise RuntimeError("missing allocation diagnostic")
    return dict(zip(("allocations", "requested_bytes", "peak_bytes"), map(int, match.groups())))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--umber", type=Path, default=ROOT / "target/release/umber")
    parser.add_argument("--allocations", type=Path,
                        default=ROOT / "target/release/examples/allocations")
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--baseline-allocations", type=Path)
    parser.add_argument("--pdftex")
    parser.add_argument("--cases", nargs="+", choices=CASES, default=list(CASES))
    parser.add_argument("--sizes", nargs="+", type=int, help="override each workload's default sizes")
    parser.add_argument("--repeats", type=int, default=7)
    parser.add_argument("--report", type=Path, default=ROOT / "target/benchmark/memory.json")
    args = parser.parse_args()
    if args.repeats < 1 or any(size < 1 or size > 2147483647 for size in (args.sizes or [])):
        parser.error("use positive repetitions and sizes within TeX's integer range")
    cpu = min(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else None
    if cpu is not None:
        os.sched_setaffinity(0, {cpu})
    folder = ROOT / "target/benchmark/memory"
    command, env = reference(args.pdftex, folder / "pdftex")
    engines = {"umber": ([str(args.umber.resolve())], None), "pdftex": (command, env)}
    if args.baseline:
        engines["baseline"] = ([str(args.baseline.resolve())], None)
    report = {"date": datetime.now(timezone.utc).isoformat(), "cpu": cpu,
              "pdftex": subprocess.check_output([command[0], "--version"], env=env).decode().splitlines()[0],
              "rustc": subprocess.check_output(["rustc", "--version"]).decode().strip(),
              "generator_sha256": digest(Path(__file__)),
              "runner_sha256": digest(ROOT / "benchmarks/mvp.py"),
              "umber_binary_sha256": digest(args.umber),
              "allocation_binary_sha256": digest(args.allocations),
              "scope": "Full-process CPU/wall medians; allocator requests cover engine construction/execution and exclude the compact host input. Heap peaks are requested bytes, not RSS. pdfTeX memory is not measured by this adapter.",
              "repeats": args.repeats, "complete": False, "samples": []}
    for label, path in (("baseline", args.baseline), ("baseline_allocation", args.baseline_allocations)):
        if path:
            report[label + "_binary_sha256"] = digest(path)
    for case in args.cases:
        sizes = args.sizes or {
            "churn": [1000, 10000, 50000],
            "live-growth": [100, 1000, 2500],
            "metadata": [10, 100, 1000],
            "live-metadata": [100, 1000, 2500],
        }[case]
        for size in sizes:
            source, expected = program(case, size)
            path = folder / f"{case}-{size}.tex"
            path.write_bytes(source)
            row = {"case": case, "size": size, "bytes": len(source),
                   "sha256": hashlib.sha256(source).hexdigest(),
                   **measure(engines, path, expected, args.repeats),
                   "allocation": allocation(args.allocations, path, expected)}
            if args.baseline_allocations:
                row["baseline_allocation"] = allocation(args.baseline_allocations, path, expected)
            report["samples"].append(row)
            args.report.parent.mkdir(parents=True, exist_ok=True)
            args.report.write_text(json.dumps(report, indent=2) + "\n")
            print(case, size, "CPU/pdfTeX", round(row["cpu_ratio"], 3),
                  "peak bytes", row["allocation"]["peak_bytes"], flush=True)
    report["complete"] = True
    args.report.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
