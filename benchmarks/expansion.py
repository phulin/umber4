"""Validated macro workloads from Gentle, plus token lists and name working sets.

These are expansion kernels, not a compilation of the whole manual. Every
record is expanded into a definition and compared with independently generated
expected tokens. This validation is part of the measured work.
"""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess

from mvp import BOOT, ROOT, measure, reference

CORPUS = Path(__file__).resolve().parent / "corpus"
KERNELS = (CORPUS / "gentle-kernels.tex").read_text()
CASES = ("gentle-text", "gentle-calendar", "gentle-contents", "gentle-records",
         "stored-records", "token-pipeline", "name-table", "optional-arguments")
MONTHS = ("January February March April May June July August September October "
          "November December").split()
SPELLINGS = (("analogue", "analog"), ("cancelled", "canceled"),
             ("centimetre", "centimeter"), ("centre", "center"),
             ("centred", "centered"), ("our", "or"),
             ("postalcode", "zipcode"), ("province", "state"),
             ("theatre", "theater"))
ALIASES = ("analog", "canceled", "centimeter", "center", "centered",
           "our", "postcode", "province", "theater")
NAMES = ("John", "Jane", "Michael", "Alice", "She", "We", "The author")
LENGTHS = (8, 32, 128, 256, 1024)
CHECK = r"\ifx\actual\wanted\advance\count0by1\relax" \
        r"\else\message{UMBER:MISMATCH:END}\end\fi" + "\n"


def payload(index):
    # Distinct deterministic text, balanced groups, spaces and a nested macro.
    alphabet = "abcdefghijklmnopqrstuvwxyz"
    word = "".join(alphabet[(index * 7 + k * 11) % 26] for k in range(19))
    text = (word + " ") * (LENGTHS[index % len(LENGTHS)] // 20 + 1)
    return text[:LENGTHS[index % len(LENGTHS)]].rstrip()


def roman(number):
    text = ""
    for value, digits in ((1000, "m"), (900, "cm"), (500, "d"), (400, "cd"),
                          (100, "c"), (90, "xc"), (50, "l"), (40, "xl"),
                          (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")):
        copies, number = divmod(number, value)
        text += digits * copies
    return text


def wanted(text):
    return r"\def\wanted{" + text + "}\n"


def actual(text):
    return r"\edef\actual{" + text + "}\n"


def program(case, size):
    lines = [BOOT, r"\def\space{ }\let\ifcanspell=\iftrue\countdef\pageno=1",
             "\n", KERNELS, "\n"]
    expanded_bytes = 0
    if case == "gentle-contents":
        # Read literal expected backslashes as characters. The macro itself is
        # already tokenized; it restores the source category after its argument.
        lines.append(r"\def\expected#1{\edef\wanted{\detokenize{#1}}"
                     r"\catcode92=0\relax}"
                     r"\def\literal{\catcode92=12\relax\expected}" + "\n")
    if case == "token-pipeline":
        lines.append(r"\toksdef\left=0\toksdef\right=1\toksdef\joined=2"
                     r"\def\wrap#1{(#1)}\def\setflag{\count9=1\relax}" + "\n")
    if case == "stored-records":
        lines.append(r"\long\def\forward#1{\maketable#1}" + "\n")
    if case == "optional-arguments":
        lines.append(r"\let\open=[\def\parse{\futurelet\next\dispatch}"
                     r"\def\dispatch{\unless\ifx\next\open\let\handler=\without"
                     r"\else\let\handler=\with\fi\handler}"
                     r"\def\with[#1]#2{\finish{#1}{#2}}"
                     r"\def\without#1{\finish{normal}{#1}}"
                     r"\def\finish#1#2{\edef\actual{#1:#2/"
                     r"\number\numexpr(\count0+3)*7/2\relax}}" + "\n")
    width = min(4096, max(64, size // 8))
    table = {}
    if case == "name-table":
        for key in range(width):
            text = payload(key)[:64]
            table[key] = text
            lines.append(r"\expandafter\def\csname field" + str(key)
                         + r"\endcsname{" + text + "}\n")
    for index in range(size):
        canadian = (index // len(SPELLINGS)) % 2 == 0
        spelling_index = (index * 7) % len(SPELLINGS)
        spelling = SPELLINGS[spelling_index][0 if canadian else 1]
        alias = "\\" + ALIASES[spelling_index]
        if case != "name-table":
            lines.append(r"\let\ifcanspell=" + (r"\iftrue" if canadian else r"\iffalse")
                         + "\n")
        if case == "gentle-text":
            a, b = NAMES[index % len(NAMES)], NAMES[(index * 3 + 2) % len(NAMES)]
            lines.append(r"\def\person{" + a + "}\n")
            text = f"{a} talks to {b}. {spelling}/{spelling}."
            lines.extend([wanted(text), actual(r"\talks{\person}{" + b + "} "
                          + alias + "/" + alias + ".")])
        elif case == "gentle-calendar":
            month, day, year = index % 12 + 1, index % 28 + 1, 1993 + index % 35
            lines.append(f"\\month={month} \\day={day} \\year={year}\\relax\n")
            text = f"{MONTHS[month - 1]} {day}, {year}"
            lines.extend([wanted(text), actual(r"\today")])
        elif case == "gentle-contents":
            page = -(index % 39 + 1) if index % 4 == 0 else index % 900 + 1
            number = roman(-page) if page < 0 else str(page)
            title = f"{spelling} chapter {index % 53} " + payload(index)[:128]
            text = f"\\line{{{title} \\dotfill{{}} {number}}}"
            lines.append(f"\\pageno={page}\\relax\n")
            lines.append(r"\literal{" + text + "}\n")
            lines.append(actual(r"\contentsentry{" + alias + r"\space" + f" chapter {index % 53} "
                                + payload(index)[:128] + "}"))
            lines.append(actual(r"\expandafter\detokenize\expandafter{\actual}"))
        elif case in ("gentle-records", "stored-records"):
            title = f"{spelling} table {index % 71}"
            body = payload(index) + " {" + payload(index + 1)[:32] + "} " + spelling
            text = f"[{title}]|[{body}]"
            lines.append(wanted(text))
            arguments = "[" + alias + r"\space" + f" table {index % 71}]" \
                + payload(index) + " {" + payload(index + 1)[:32] \
                + "} " + alias + r"\par"
            if case == "gentle-records":
                lines.append(r"\maketable" + arguments + "\n")
            elif index % 2:
                lines.append(r"\def\call{\maketable" + arguments + r"}\call" + "\n")
            else:
                lines.append(r"\forward{" + arguments + "}\n")
        elif case == "token-pipeline":
            field = payload(index)
            later_field = payload(index + 5)
            text = f"({later_field})|{spelling}|({later_field})"
            lines.append(r"\def\field{" + field + "}\n")
            # Register copies share storage; the saved values outlive overwrites.
            lines.append(r"\count9=0\relax\afterassignment\setflag\left={\wrap{\field}}"
                         r"\right=\left\relax"
                         r"{\left={discarded}\global\joined=\right\relax}"
                         r"\edef\pipeline{\the\right|" + alias
                         + r"|\the\joined}" + "\n")
            lines.append(r"\def\field{" + later_field + "}\n")
            text += "/1"
            lines.extend([wanted(text), actual(r"\pipeline/\the\count9")])
        elif case == "name-table":
            key = (index * 4051 + 17) % width
            if index % 17 == 0:
                table[key] = payload(index + 9)[:64]
                lines.append(r"\expandafter\def\csname field" + str(key)
                             + r"\endcsname{" + table[key] + "}\n")
            present = index % 13 != 0
            name = "field" + str(key) if present else "missing" + str(index % 64)
            text = table[key] if present else "fallback"
            lines.extend([wanted(text), actual(r"\ifcsname " + name + r"\endcsname"
                          r"\csname " + name + r"\endcsname\else fallback\fi")])
        elif case == "optional-arguments":
            option = spelling if index % 4 else "normal"
            body = payload(index) + " {" + payload(index + 1)[:32] + "}"
            number = (7 * (index + 3) + 1) // 2
            text = f"{option}:{body}/{number}"
            lines.append(wanted(text))
            invocation = r"\parse" + ("[" + alias + "]" if index % 4 else "")
            invocation += "{" + body + "}"
            if index % 2:
                lines.append(r"\def\call{" + invocation + r"}\call" + "\n")
            else:
                lines.append(invocation + "\n")
        else:
            raise ValueError(case)
        expanded_bytes += len(text)
        lines.append(CHECK)
    lines.append(r"\message{UMBER:\the\count0:END}\end" + "\n")
    return "".join(lines).encode(), str(size).encode(), {
        "expanded_text_bytes": expanded_bytes,
        "name_table_width": width if case == "name-table" else None,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--umber", type=Path, default=ROOT / "target/release/umber")
    parser.add_argument("--pdftex")
    parser.add_argument("--baseline", type=Path,
                        help="previous expanded-MVP binary to time in the same run")
    parser.add_argument("--cases", nargs="+", choices=CASES, default=list(CASES))
    parser.add_argument("--sizes", nargs="+", type=int, default=[1000, 10000, 50000])
    parser.add_argument("--repeats", type=int, default=7)
    parser.add_argument("--cpu", type=int)
    parser.add_argument("--report", type=Path, default=ROOT / "target/benchmark/expansion.json")
    args = parser.parse_args()
    if args.repeats < 1 or any(size < 1 or size > 1000000 for size in args.sizes):
        parser.error("use positive repeats and sizes up to 1000000")
    cpu = args.cpu
    if hasattr(os, "sched_setaffinity"):
        cpu = cpu if cpu is not None else min(os.sched_getaffinity(0))
        os.sched_setaffinity(0, {cpu})
    output = ROOT / "target/benchmark/expansion"
    command, env = reference(args.pdftex, output / "pdftex")
    engines = {"umber": ([str(args.umber.resolve())], None), "pdftex": (command, env)}
    if args.baseline:
        engines["baseline"] = ([str(args.baseline.resolve())], None)
    cpu_model = platform.processor()
    if Path("/proc/cpuinfo").is_file():
        cpu_model = next(line.split(":", 1)[1].strip()
                         for line in Path("/proc/cpuinfo").read_text().splitlines()
                         if line.startswith("model name"))
    provenance = json.loads((CORPUS / "gentle.json").read_text())
    report = {
        "date": datetime.now(timezone.utc).isoformat(),
        "pdftex": subprocess.check_output([command[0], "--version"], env=env)
        .decode().splitlines()[0],
        "rustc": subprocess.check_output(["rustc", "--version"]).decode().strip(),
        "rustflags": os.environ.get("RUSTFLAGS", ""),
        "machine": platform.platform(), "cpu": cpu, "cpu_model": cpu_model,
        "umber_binary_sha256": hashlib.sha256(args.umber.read_bytes()).hexdigest(),
        "generator_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "measurement_sha256": hashlib.sha256((Path(__file__).parent / "mvp.py").read_bytes()).hexdigest(),
        "kernels_sha256": hashlib.sha256((CORPUS / "gentle-kernels.tex").read_bytes()).hexdigest(),
        "corpus": provenance,
        "validation": "Every record compares its expanded definition with independently generated expected tokens; comparison and expected definition construction are timed.",
        "storage": "Definition and token-register replacements accumulate immutable arena tokens within the Umber run; total work is bounded by the selected size.",
        "repeats": args.repeats, "complete": False, "samples": [],
    }
    if args.baseline:
        report["baseline_binary_sha256"] = hashlib.sha256(args.baseline.read_bytes()).hexdigest()
    startup = output / "startup.tex"
    startup.write_text(BOOT + r"\message{UMBER:0:END}\end" + "\n")
    report["startup"] = measure(engines, startup, b"0", args.repeats)
    for case in args.cases:
        for size in args.sizes:
            source, expected, work = program(case, size)
            path = output / f"{case}-{size}.tex"
            path.write_bytes(source)
            row = {"case": case, "size": size, "bytes": len(source), **work,
                   "sha256": hashlib.sha256(source).hexdigest(),
                   **measure(engines, path, expected, args.repeats)}
            report["samples"].append(row)
            medians = row["median"]
            print(f"{case:16} {size:7}  umber {medians['umber']:.6f}s  "
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
