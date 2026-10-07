"""Compare marked messages from the exact same source in both engines."""

import argparse
import json
from pathlib import Path
import re
import subprocess

from reference import ROOT, execute, reference


def delimited_windows(folder):
    # Comments join physical lines without adding tokens, keeping the generated
    # literals within the native INITEX reader's default line-buffer capacity.
    def wrapped(text):
        return "%\n".join(text[index:index + 1000] for index in range(0, len(text), 1000))

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
