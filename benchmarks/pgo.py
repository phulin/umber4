"""Build an optional native interpreter profile from the validated macro suites.

All build artifacts stay in target/pgo. Training uses sizes outside the ordinary
timing sweep; the final executable has no profiling instrumentation.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

import expansion
import mvp

ROOT = mvp.ROOT
OUTPUT = ROOT / "target/pgo"
OPTIONS = ["-C", "llvm-args=-pgo-warn-missing-function",
           "-C", "llvm-args=--inline-enable-cost-benefit-analysis=false",
           "-C", "llvm-args=--hot-callsite-threshold=1000"]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def build(stage, flags, example=False):
    env = os.environ.copy()
    env.pop("RUSTFLAGS", None)
    env.pop("LLVM_PROFILE_FILE", None)
    env["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(flags)
    env["CARGO_TARGET_DIR"] = str(OUTPUT / stage)
    target = ["--example", "allocations"] if example else ["--bin", "umber"]
    subprocess.run(["cargo", "build", "--release", "-p", "umber", *target],
                   cwd=ROOT, env=env, check=True)
    return OUTPUT / stage / "release" / ("examples/allocations" if example else "umber")


def oracle(binary, env, name, pdftex):
    command = [sys.executable, str(ROOT / "scripts/oracle.py"), "--umber", str(binary)]
    if pdftex:
        command.extend(["--pdftex", pdftex])
    with (OUTPUT / f"{name}-oracle.log").open("wb") as log:
        subprocess.run(command, cwd=ROOT, env=env, stdout=log, check=True)
    print(f"PASS native oracle: {name}", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pdftex", help="native oracle binary")
    parser.add_argument("--cpu", type=int, help="training CPU; first allowed CPU by default")
    args = parser.parse_args()
    if os.environ.get("RUSTFLAGS") or os.environ.get("CARGO_ENCODED_RUSTFLAGS"):
        parser.error("unset existing Rust flags so the recorded profile options are complete")
    compiler = subprocess.check_output(["rustc", "-Vv"]).decode()
    host = next(line.removeprefix("host: ") for line in compiler.splitlines()
                if line.startswith("host: "))
    sysroot = Path(subprocess.check_output(["rustc", "--print", "sysroot"]).decode().strip())
    profdata = sysroot / "lib/rustlib" / host / "bin/llvm-profdata"
    if not profdata.is_file():
        parser.error("install the matching tools with: rustup component add llvm-tools")
    if hasattr(os, "sched_setaffinity"):
        cpu = args.cpu if args.cpu is not None else min(os.sched_getaffinity(0))
        os.sched_setaffinity(0, {cpu})
    OUTPUT.mkdir(parents=True, exist_ok=True)
    (OUTPUT / "build.json").unlink(missing_ok=True)
    raw = OUTPUT / "raw"
    if raw.exists():
        shutil.rmtree(raw)
    raw.mkdir()
    training = OUTPUT / "training"
    training.mkdir(exist_ok=True)
    generated = build("generate", ["-C", f"profile-generate={raw}"])
    env = os.environ.copy()
    env["LLVM_PROFILE_FILE"] = str(raw / "%m-%p.profraw")
    manifest = {
        "compiler": compiler,
        "profdata_version": subprocess.check_output([str(profdata), "--version"]).decode(),
        "builder_sha256": digest(Path(__file__)),
        "source_files": {str(path.relative_to(ROOT)): digest(path) for path in sorted([
            ROOT / "Cargo.toml", ROOT / "Cargo.lock", ROOT / "crates/umber/Cargo.toml",
            *(ROOT / "crates/umber").rglob("*.rs"),
        ])},
        "training_binary_sha256": digest(generated),
        "generators": {name: digest(ROOT / f"benchmarks/{name}.py")
                       for name in ("mvp", "expansion")},
        "kernels_sha256": digest(expansion.CORPUS / "gentle-kernels.tex"),
        "sources": [],
    }
    for generator, size in ((mvp, 75031), (expansion, 7003)):
        for case in generator.CASES:
            source, expected, *_ = generator.program(case, size)
            path = training / f"{case}-{size}.tex"
            path.write_bytes(source)
            result = subprocess.run([str(generated), str(path)], env=env, check=True,
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            actual = re.findall(rb"UMBER:(.*?):END", result.stdout)
            if actual != [expected]:
                raise SystemExit(f"{case}: {actual!r}, expected {expected!r}")
            manifest["sources"].append({"case": case, "size": size, "bytes": len(source),
                                        "sha256": digest(path)})
            print(f"TRAIN {case}: {size}", flush=True)
    oracle(generated, env, "training", args.pdftex)
    profile = OUTPUT / "umber.profdata"
    subprocess.run([str(profdata), "merge", "-o", str(profile),
                    *map(str, sorted(raw.glob("*.profraw")))], check=True)
    # The profile identity in RUSTFLAGS also invalidates Cargo's build cache.
    retained_profile = OUTPUT / f"{digest(profile)}.profdata"
    profile.replace(retained_profile)
    profile = retained_profile
    flags = ["-C", f"profile-use={profile}", *OPTIONS]
    binary = build("use", flags)
    diagnostic = build("use", flags, example=True)
    env.pop("LLVM_PROFILE_FILE")
    oracle(binary, env, "final", args.pdftex)
    manifest.update(profile_path=str(profile.relative_to(ROOT)),
                    profile_sha256=digest(profile), rustflags=" ".join(flags),
                    rustflags_args=flags,
                    umber_binary_sha256=digest(binary),
                    allocation_binary_sha256=digest(diagnostic))
    (OUTPUT / "build.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Native profile build: {binary}")


if __name__ == "__main__":
    main()
