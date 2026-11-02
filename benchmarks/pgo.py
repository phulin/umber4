"""Build optional default-feature PGO from one explicit Umber CLI training job.

Run from any directory; output and training paths are relative to the checkout.
Profiles and generate/use builds stay in a fresh directory under checkout target.
"""

import argparse
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
EXE = ".exe" if os.name == "nt" else ""


def build(output, stage, flag, layout=False):
    env = os.environ.copy()
    env.pop("LLVM_PROFILE_FILE", None)
    env["CARGO_TARGET_DIR"] = str(output / stage)
    env["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(["-C", flag])
    command = ["cargo", "build", "--offline", "--release", "-p", "umber",
               "--bin", "umber"]
    if layout:
        command.extend(["--example", "layout"])
    subprocess.run(command, cwd=ROOT, env=env, check=True)
    return output / stage / "release" / f"umber{EXE}"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path,
                        help="new directory below checkout target (checkout-relative)")
    parser.add_argument("training", nargs=argparse.REMAINDER,
                        help="one Umber CLI argument list after --")
    args = parser.parse_args()
    if len(args.training) < 2 or args.training[0] != "--":
        parser.error("supply one Umber CLI training argument list after --")
    conflicts = [key for key, value in os.environ.items() if value and (
        key in {"RUSTC", "CARGO_BUILD_RUSTC", "RUSTFLAGS",
                "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_RUSTFLAGS",
                "CARGO_TARGET_DIR", "CARGO_BUILD_TARGET", "CARGO_BUILD_TARGET_DIR"}
        or key.startswith("CARGO_PROFILE_RELEASE_")
        or (key.startswith("CARGO_TARGET_") and key.endswith("_RUSTFLAGS")))]
    if conflicts:
        parser.error("unset conflicting build overrides: " + ", ".join(sorted(conflicts)))
    output = (ROOT / args.output).resolve()
    if output == ROOT / "target" or not output.is_relative_to(ROOT / "target"):
        parser.error("--output must be a new directory below checkout target")
    if output.exists():
        parser.error("--output already exists; choose a fresh directory")
    compiler = subprocess.check_output(["rustc", "-vV"], cwd=ROOT, text=True)
    host = next(line.removeprefix("host: ") for line in compiler.splitlines()
                if line.startswith("host: "))
    sysroot = Path(subprocess.check_output(
        ["rustc", "--print", "sysroot"], cwd=ROOT, text=True).strip())
    profdata = sysroot / "lib/rustlib" / host / "bin" / f"llvm-profdata{EXE}"
    if not profdata.is_file():
        parser.error("matching llvm-profdata is not installed in the rustc sysroot")
    output.mkdir(parents=True)
    raw = output / "raw"
    raw.mkdir()
    generated = build(output, "generate", f"profile-generate={raw}")
    env = os.environ.copy()
    env["LLVM_PROFILE_FILE"] = str(raw / "%m-%p.profraw")
    subprocess.run([str(generated), *args.training[1:]], cwd=ROOT, env=env, check=True)
    profiles = sorted(raw.glob("*.profraw"))
    if not profiles or any(path.stat().st_size == 0 for path in profiles):
        raise RuntimeError("training did not produce nonempty raw profiles")
    profile = output / "umber.profdata"
    subprocess.run([str(profdata), "merge", "-o", str(profile),
                    *map(str, profiles)], cwd=ROOT, check=True)
    binary = build(output, "use", f"profile-use={profile}", layout=True)
    print(f"PGO CLI: {binary}")
    print(f"PGO layout adapter: {binary.parent / 'examples' / f'layout{EXE}'}")


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"PGO build failed: {error}", file=sys.stderr)
        sys.exit(1)
