"""Compare marked messages from the exact same source in both engines."""

import argparse
import json
from pathlib import Path
import re
import subprocess

from reference import ROOT, execute, reference


def wrapped(text):
    # Comments join physical lines without adding tokens, keeping the generated
    # literals within the native INITEX reader's default line-buffer capacity.
    return "%\n".join(text[index:index + 1000] for index in range(0, len(text), 1000))


def delimited_windows(folder):
    sources = []
    for length in (1, 4093, 4094, 4095, 4096, 4097, 8191, 8192, 8193):
        text = wrapped("X" * length)
        padding = wrapped("P" * (4092 + length % 4))
        lines = [r"\catcode123=1 \catcode125=2 \catcode35=6\relax",
                 r"\def\padding{" + padding + "}",
                 r"\def\grab#1aaba{\edef\held{#1/#1}}"]
        cases = [
            (r"\def\stored{\grab " + text + r"aaaba}\stored", text + "a"),
            (r"\def\forward#1{\grab#1}\forward{" + text + "aaaba}", text + "a"),
            (r"\def\across#1#2{\grab#1#2}\across{" + text + "aa}{aba}", text + "a"),
            (r"\def\stored{\grab{" + text + r"}aaba}\stored", text),
        ]
        for call, captured in cases:
            lines.extend([call, r"\def\wanted{" + captured + "/" + captured + "}",
                          r"\message{UMBER:\ifx\held\wanted Y\else N\fi:END}"])
        lines.append(r"\end")
        path = folder / f"delimited-window-{length}.tex"
        path.write_text("\n".join(lines) + "\n")
        sources.append(path)
    return sources


def single_group_windows(folder):
    sources = []
    for padding in range(4084, 4093):
        text = wrapped("P" * padding)
        source = (r"\catcode123=1 \catcode125=2 \catcode35=6\relax" + "\n"
                  + r"\def\padding{" + text + r"}\def\id#1{#1}\def\stored{\id{X}}"
                  + "\n" + r"\def\emit#1{\id{#1}}\def\forward#1{#1}" + "\n"
                  + r"\message{UMBER:\stored/\stored/\emit{Y}/\forward{\id{Z}}:END}\end"
                  + "\n")
        path = folder / f"single-group-window-{padding}.tex"
        path.write_text(source)
        sources.append(path)
    return sources


def group_run_boundaries(folder):
    for padding in range(4):
        for length in (*range(13), 255, 256, 257):
            payload = "A" * length
            source = (r"\catcode123=1 \catcode125=2 \catcode35=6\relax" + "\n"
                      + r"\def\padding{" + "P" * padding + "}"
                      + r"\def\id#1{#1}\def\ignore#1{I}" + "\n"
                      + r"\def\forward#1{\id{#1}/\ignore{#1}}" + "\n"
                      + r"\def\invoke{\id{" + payload + r"}B/\ignore{" + payload
                      + r"}D/\forward{" + payload + "}E}" + "\n"
                      + r"\message{UMBER:\invoke/\id{" + payload + r"}F/\ignore{"
                      + payload + r"}G:END}\end" + "\n")
            expected = f"{payload}B/ID/{payload}/IE/{payload}F/IG".encode()
            path = folder / f"group-run-{padding}-{length}.tex"
            path.write_text(source)
            yield path, [expected]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--umber", type=Path, default=ROOT / "target/release/umber")
    parser.add_argument("--pdftex")
    parser.add_argument("sources", nargs="*", type=Path)
    args = parser.parse_args()
    command, env = reference(args.pdftex, ROOT / "target/oracle")
    sources = args.sources or sorted((ROOT / "fixtures/mvp").glob("*.tex"))
    generated_expected = {}
    if not args.sources:
        lines = json.loads((ROOT / "fixtures/mvp/physical-lines.json").read_text())
        for name, source in lines.items():
            path = ROOT / "target/oracle" / (name + ".tex")
            path.write_bytes(source.encode())
            sources.append(path)
        for path in delimited_windows(ROOT / "target/oracle"):
            sources.append(path)
            generated_expected[path] = [b"Y"] * 4
        for path in single_group_windows(ROOT / "target/oracle"):
            sources.append(path)
            generated_expected[path] = [b"X/X/Y/Z"]
        for path, expected in group_run_boundaries(ROOT / "target/oracle"):
            sources.append(path)
            generated_expected[path] = expected
    for source in sources:
        native = execute(command, source, env)
        umber = execute([str(args.umber.resolve())], source)
        expected = re.findall(rb"UMBER:(.*?):END", native, re.DOTALL)
        actual = re.findall(rb"UMBER:(.*?):END", umber, re.DOTALL)
        if source in generated_expected and expected != generated_expected[source]:
            raise SystemExit(f"FAIL generated expectation {source.name}: pdftex={expected!r}")
        if not expected or actual != expected:
            raise SystemExit(f"FAIL {source.name}: pdftex={expected!r}, umber={actual!r}")
        print(f"PASS {source.name}: {len(actual)} messages")
    if not args.sources:
        for source in sorted((ROOT / "fixtures/mvp/failures").glob("*.tex")):
            for cmd, engine_env in ((command, env), ([str(args.umber.resolve())], None)):
                result = subprocess.run(cmd + [str(source)], cwd=ROOT, env=engine_env,
                                        stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
                if result.returncode == 0:
                    raise SystemExit(f"FAIL {cmd[0]} unexpectedly accepted {source.name}")
            print(f"PASS {source.name}: both reject")


if __name__ == "__main__":
    main()
