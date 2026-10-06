"""The native INITEX oracle, without a format or font installation."""

import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def reference(requested, output):
    local = ROOT / ".tools/pdftex/root/usr/bin/pdftex"
    binary = Path(requested or shutil.which("pdftex") or local).resolve()
    if not binary.is_file():
        raise SystemExit("pdftex not found; install it or pass --pdftex /path/to/pdftex")
    output.mkdir(parents=True, exist_ok=True)
    config = output / "config"
    config.mkdir(exist_ok=True)
    (config / "texmf.cnf").write_text(
        "TEXINPUTS = .\nopenin_any = a\nopenout_any = a\n"
        "shell_escape = f\nmax_print_line = 10000\n"
    )
    env = os.environ.copy()
    env["TEXMFCNF"] = str(config) + ":"
    env["TEXINPUTS"] = "."
    if binary == local:
        lib = ROOT / ".tools/pdftex/root/usr/lib/x86_64-linux-gnu"
        env["LD_LIBRARY_PATH"] = str(lib) + ":" + env.get("LD_LIBRARY_PATH", "")
    command = [str(binary), "-ini", "-etex", "-no-shell-escape",
               "-interaction=nonstopmode", "-halt-on-error",
               "-output-directory=" + str(output)]
    return command, env


def execute(command, source, env=None):
    result = subprocess.run(command + [str(source.resolve())], cwd=ROOT,
                            env=env, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT)
    if result.returncode:
        raise RuntimeError(f"{command[0]} failed on {source}:\n"
                           + result.stdout.decode(errors="replace"))
    return result.stdout
