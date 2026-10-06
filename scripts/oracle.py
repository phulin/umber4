"""Compare marked messages from the exact same source in both engines."""

import argparse
import json
from pathlib import Path
import re
import subprocess

from reference import ROOT, execute, reference


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--umber", type=Path, default=ROOT / "target/release/umber")
    parser.add_argument("--pdftex")
    parser.add_argument("sources", nargs="*", type=Path)
    args = parser.parse_args()
    command, env = reference(args.pdftex, ROOT / "target/oracle")
    sources = args.sources or sorted((ROOT / "fixtures/mvp").glob("*.tex"))
    if not args.sources:
        lines = json.loads((ROOT / "fixtures/mvp/physical-lines.json").read_text())
        for name, source in lines.items():
            path = ROOT / "target/oracle" / (name + ".tex")
            path.write_bytes(source.encode())
            sources.append(path)
    for source in sources:
        native = execute(command, source, env)
        umber = execute([str(args.umber.resolve())], source)
        expected = re.findall(rb"UMBER:(.*?):END", native, re.DOTALL)
        actual = re.findall(rb"UMBER:(.*?):END", umber, re.DOTALL)
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
