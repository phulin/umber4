"""Compare marked messages from the exact same source in both engines."""

import argparse
from pathlib import Path
import re

from reference import ROOT, execute, reference


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--umber", type=Path, default=ROOT / "target/release/umber")
    parser.add_argument("--pdftex")
    parser.add_argument("sources", nargs="*", type=Path)
    args = parser.parse_args()
    command, env = reference(args.pdftex, ROOT / "target/oracle")
    sources = args.sources or sorted((ROOT / "fixtures/mvp").glob("*.tex"))
    for source in sources:
        native = execute(command, source, env)
        umber = execute([str(args.umber.resolve())], source)
        expected = re.findall(rb"UMBER:(.*?):END", native, re.DOTALL)
        actual = re.findall(rb"UMBER:(.*?):END", umber, re.DOTALL)
        if not expected or actual != expected:
            raise SystemExit(f"FAIL {source.name}: pdftex={expected!r}, umber={actual!r}")
        print(f"PASS {source.name}: {len(actual)} messages")


if __name__ == "__main__":
    main()
