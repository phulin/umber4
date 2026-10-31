"""One bounded prose/layout and unchanged-source checkpoint measurement pass."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import statistics
import struct
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from reference import ROOT, reference

DOCS = ["architecture", "storage", "checkpoints", "fonts-text", "paragraphs", "pages"]
FONTS = ROOT / ".tools/lexing-corpus/root/usr/share/texlive/texmf-dist/fonts/tfm/public"
CASES = [
    ("book", "ae/aer10", 360, 540, 12, 0),
    ("narrow", "latex-fonts/lcmss8", 180, 360, 12, 3),
    ("dense", "latex-fonts/icmtt8", 420, 600, 10, 0),
]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def prose():
    """Keep actual prose paragraphs, remove Markdown notation/TeX metacharacters."""
    paragraphs = []
    for name in DOCS:
        text = (ROOT / f"docs/{name}.md").read_text()
        text = re.sub(r"```.*?```", "", text, flags=re.S)
        for paragraph in re.split(r"\n\s*\n", text):
            if not re.match(r"^[A-Za-z]", paragraph) or "|" in paragraph:
                continue
            paragraph = re.sub(r"\[([^]]+)\]\([^)]*\)", r"\1", paragraph)
            paragraph = re.sub(r"[^A-Za-z0-9.,;:!?()' /+\-=\-]", " ", paragraph)
            paragraph = " ".join(paragraph.split())
            if len(paragraph.split()) >= 30:
                paragraphs.append(paragraph)
    assert len(paragraphs) >= 100
    return paragraphs


def source(paragraphs, font, width, height, leading, gap, count):
    boot = (r"\catcode123=1\catcode125=2\catcode35=6"
            + f"\\font\\f={font} at10pt\\f\\hsize={width}pt\\vsize={height}pt"
            + f"\\baselineskip={leading}pt\\parskip={gap}pt"
            + r"\topskip=10pt\maxdepth=3pt\lineskip=1pt\parindent=12pt"
            + r"\hfuzz=16383pt\vfuzz=16383pt"
            + r"\parfillskip=0pt plus1fil\pretolerance=100\tolerance=10000"
            + r"\def\finish{\par\advance\count0by1\relax}" + "\n")
    body = "\n".join(paragraphs[i % len(paragraphs)] + r"\finish" for i in range(count))
    return (boot + body + "\n" + r"\message{UMBER:\the\count0:END}\end" + "\n").encode()


def dvi_pages(data, tfm):
    """Read native glyph coordinates; widths come independently from raw TFM."""
    _, lh, bc, ec, *_ = struct.unpack(">12H", tfm[:24])
    char_start = 24 + lh * 4
    width_start = char_start + (ec - bc + 1) * 4
    def width(c):
        index = tfm[char_start + (c - bc) * 4]
        a, b, c, d = tfm[width_start + index * 4:width_start + index * 4 + 4]
        # TeX's store_scaled, for this pass's explicit 10pt size (< 2^23).
        z = 10 * 65536
        return (((d * z // 256 + c * z) // 256) + b * z) // 16 - (16 * z if a else 0)
    p, pos, stack, pages = 0, [0] * 6, [], []
    def take(n, signed=False):
        nonlocal p
        v = int.from_bytes(data[p:p+n], "big", signed=signed)
        p += n
        return v
    while p < len(data):
        op = take(1)
        if op <= 131 or 133 <= op <= 136:
            c = op if op <= 127 else take(op - (127 if op <= 131 else 132))
            pages[-1].append([c, pos[0], pos[1]])
            if op <= 131:
                pos[0] += width(c)
        elif op in (132, 137):
            raise AssertionError("unexpected rule")
        elif op == 138:
            pass
        elif op == 139:
            p += 44
            pos, stack = [0] * 6, []
            pages.append([])
        elif op == 140:
            assert not stack
        elif op == 141:
            stack.append(pos.copy())
        elif op == 142:
            pos = stack.pop()
        elif 143 <= op <= 146:
            pos[0] += take(op - 142, True)
        elif 147 <= op <= 156:
            slot, base = (2, 147) if op <= 151 else (3, 152)
            if op != base:
                pos[slot] = take(op - base, True)
            pos[0] += pos[slot]
        elif 157 <= op <= 160:
            pos[1] += take(op - 156, True)
        elif 161 <= op <= 170:
            slot, base = (4, 161) if op <= 165 else (5, 166)
            if op != base:
                pos[slot] = take(op - base, True)
            pos[1] += pos[slot]
        elif 171 <= op <= 234:
            pass
        elif 235 <= op <= 238:
            take(op - 234)
        elif 239 <= op <= 242:
            n = take(op - 238)
            p += n
        elif 243 <= op <= 246:
            take(op - 242)
            p += 12
            area, name = take(1), take(1)
            p += area + name
        elif op == 247:
            assert take(1) == 2
            p += 12
            n = take(1)
            p += n
        elif op in (248, 249):
            break
        else:
            raise AssertionError(f"DVI opcode {op}")
    return pages


def child(command, env=None):
    with tempfile.TemporaryFile() as stdout:
        start = time.perf_counter_ns()
        process = subprocess.Popen(command, cwd=ROOT, env=env, stdout=stdout,
                                   stderr=subprocess.STDOUT)
        _, status, usage = os.wait4(process.pid, 0)
        elapsed = (time.perf_counter_ns() - start) / 1e9
        process.returncode = os.waitstatus_to_exitcode(status)
        stdout.seek(0)
        output = stdout.read()
    assert process.returncode == 0, output.decode(errors="replace")
    return output, {"wall_seconds": elapsed,
                    "cpu_seconds": usage.ru_utime + usage.ru_stime}


def rss(command, env=None):
    # The small time process forks after exec, excluding the inspecting Python
    # parent's inherited high-water RSS from the engine's own resource usage.
    with tempfile.NamedTemporaryFile() as memory:
        subprocess.check_output(["/usr/bin/time", "-f", "%M", "-o", memory.name, *command],
                                cwd=ROOT, env=env, stderr=subprocess.STDOUT)
        return int(Path(memory.name).read_text())


def quiet_native(native, path):
    # Umber has no diagnostic threshold primitives. Only the native adapter sets
    # these; both engines consume exactly the same typesetting file.
    return native + [f"-jobname={path.stem}",
                     r"\hbadness=10000\vbadness=10000\input " + str(path)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--paragraphs", type=int, default=600)
    parser.add_argument("--repeats", type=int, default=5)
    parser.add_argument("--pdftex")
    parser.add_argument("--report", type=Path, default=ROOT / "target/benchmark/layout.json")
    args = parser.parse_args()
    assert args.paragraphs >= 100 and args.paragraphs % 100 == 0
    assert args.repeats > 0
    out = ROOT / "target/benchmark/layout"
    out.mkdir(parents=True, exist_ok=True)
    cpu = min(os.sched_getaffinity(0))
    os.sched_setaffinity(0, {cpu})
    native, env = reference(args.pdftex, out / "pdftex")
    env["TFMFONTS"] = str(out)
    cli = ROOT / "target/release/umber"
    adapter = ROOT / "target/release/examples/layout"
    allocator = ROOT / "target/release/examples/layout_allocations"
    paragraphs = prose()
    report = {
        "date": datetime.now(timezone.utc).isoformat(),
        "measured_engine_commit": subprocess.check_output(["git", "rev-parse", "HEAD"]).decode().strip(),
        "build": "cargo build --release --bin umber --example layout --example layout_allocations; default features; fat LTO; codegen-units=1",
        "rustc": subprocess.check_output(["rustc", "--version"]).decode().strip(),
        "pdftex": subprocess.check_output([native[0], "--version"], env=env).decode().splitlines()[0],
        "machine": platform.platform(), "cpu": cpu,
        "cpu_model": next(l.split(":", 1)[1].strip() for l in Path("/proc/cpuinfo").read_text().splitlines() if l.startswith("model name")),
        "rustflags": os.environ.get("RUSTFLAGS", ""),
        "native_only_setup": r"\hbadness=10000\vbadness=10000; diagnostics only, identical typesetting input",
        "hashes": {str(p.relative_to(ROOT)): sha(p) for p in [cli, adapter, allocator, Path(__file__), ROOT / "crates/umber/examples/layout_support/mod.rs"]},
        "prose_sources": {f"docs/{n}.md": sha(ROOT / f"docs/{n}.md") for n in DOCS},
        "distinct_prose_paragraphs": len(paragraphs),
        "repeats": args.repeats, "warmups": 1, "cases": [], "complete": False,
    }
    for case, font, width, height, leading, gap in CASES:
        font_path = FONTS / (font + ".tfm")
        font_name = font.split("/")[-1]
        (out / (font_name + ".tfm")).write_bytes(font_path.read_bytes())
        path = out / f"{case}.tex"
        path.write_bytes(source(paragraphs, font_name, width, height, leading, gap, args.paragraphs))
        inspect = json.loads(subprocess.check_output([adapter, "inspect", path, font_name, font_path]))
        commands = {"umber": ([str(cli), str(path), "--tfm", f"{font_name}={font_path}"], None),
                    "pdftex": (quiet_native(native, path), env)}
        # Independent native output check before timings, without trace diagnostics.
        native_output, _ = child(*commands["pdftex"])
        actual = dvi_pages((out / "pdftex" / f"{case}.dvi").read_bytes(), font_path.read_bytes())
        expected = [[p[:3] for p in page["placements"]] for page in inspect["pages"]]
        assert expected == actual, f"{case}: native placement mismatch"
        assert inspect["paragraphs"] == args.paragraphs
        samples = {"umber": [], "pdftex": []}
        for repeat in range(args.repeats + 1):
            for engine in (["umber", "pdftex"] if repeat % 2 else ["pdftex", "umber"]):
                output, sample = child(*commands[engine])
                assert re.findall(rb"UMBER:(\d+):END", output) == [str(args.paragraphs).encode()]
                if repeat:
                    samples[engine].append(sample)
        summary = {engine: {key: statistics.median(s[key] for s in rows) for key in rows[0]}
                   for engine, rows in samples.items()}
        glyphs = sum(len(p["placements"]) for p in inspect["pages"])
        row = {"case": case, "paragraphs": args.paragraphs, "source_bytes": path.stat().st_size,
               "source_sha256": sha(path), "font": font, "font_sha256": sha(font_path),
               "width_pt": width, "height_pt": height, "baseline_pt": leading, "paragraph_gap_pt": gap,
               "pages": len(expected), "glyphs": glyphs,
               "lines": sum(len({g[2] for g in p["placements"]}) for p in inspect["pages"]),
               "native_underfull_diagnostics": native_output.count(b"Underfull"),
               "validation": "all pages and glyph characters/x/y match native DVI, independently scaled raw TFM widths",
               "samples": samples, "medians": summary,
               "max_rss_kib": {engine: rss(*command) for engine, command in commands.items()},
               "cpu_ratio": summary["umber"]["cpu_seconds"] / summary["pdftex"]["cpu_seconds"],
               "umber_glyphs_per_cpu_second": glyphs / summary["umber"]["cpu_seconds"],
               "allocations": json.loads(subprocess.check_output([allocator, "alloc-full", path, font_name, font_path]))}
        report["cases"].append(row)
        print(f"{case}: {len(expected)} pages, {glyphs} glyphs, CPU ratio {row['cpu_ratio']:.3f}", flush=True)
    # One representative editing session, separate from full CLI timings.
    path, font_path, font_name = out / "book.tex", FONTS / "ae/aer10.tfm", "aer10"
    report["checkpoint"] = json.loads(subprocess.check_output([adapter, "time", path, font_name, font_path]))
    report["checkpoint_allocations"] = json.loads(subprocess.check_output([allocator, "alloc", path, font_name, font_path]))
    report["complete"] = True
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
