"""Generate the bounded layout excerpts from the complete local Gentle source."""
import hashlib
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
provenance = json.loads((HERE / "gentle.json").read_text())
original = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / ".tools/corpus/gentle.tex"
destination = Path(sys.argv[2]) if len(sys.argv) > 2 else ROOT / "target/benchmark/gentle-layout.tex"
raw = original.read_bytes()
assert hashlib.sha256(raw).hexdigest() == provenance["sha256"], "Gentle source identity differs"
lines = raw.decode().splitlines(keepends=True)
excerpts = {entry["name"]: "".join(lines[entry["lines"][0] - 1:entry["lines"][1]]).rstrip()
            for entry in provenance["layout"]["excerpts"]}
bootstrap = r"""\catcode123=1\catcode125=2\catcode35=6
\font\rm=aer10 at10pt\font\it=aeti10 at10pt\font\tt=aett10 at10pt\rm
\hsize=360pt\vsize=540pt\baselineskip=12pt\parskip=0pt
\topskip=10pt\maxdepth=3pt\lineskip=1pt\parindent=12pt
\hfuzz=16383pt\vfuzz=16383pt\parfillskip=0pt plus1fil
\pretolerance=100\tolerance=10000
\def\smallskip{\vskip3pt plus1pt minus1pt}
\def\%{\char37}
"""
body = "\n\\par\n".join(excerpts[name] for name in [
    "font-name explanation", "grouped italic sentence", "ligature example",
    "talks definition and invocations", "letter invocation",
]) + "\n\\par\n\\advance\\count0by1\\relax\n"
destination.parent.mkdir(parents=True, exist_ok=True)
destination.write_text(bootstrap + excerpts["letter definition"] + "\n"
                       + body * provenance["layout"]["repetitions"]
                       + "\n" + r"\message{UMBER:\the\count0:END}\end" + "\n")
