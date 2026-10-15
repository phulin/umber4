"""Supported producer/pull comparison kernels; every engine receives the same source."""
from pathlib import Path
from mvp import BOOT, ROOT, run_suite
CASES = ("handoff", "tail-loop", "nested-bodies", "conversions", "definitions", "messages")


def program(case, size):
    setup = ""
    factor, other = 1, 0
    if case == "handoff":
        other = 123
        setup = r"\def\value{123\relax\advance\count0by1\relax}"
        work = (r"\count1=\value" + "\n") * size
    elif case == "nested-bodies":
        setup = r"\def\leaf{\advance\count0by1\relax}"
        for name, child in zip("jihgfedcba", ["leaf", *"jihgfedcb"]):
            setup += "\\def\\" + name + "{\\" + child + "\\relax}"
        work = (r"\a" + "\n") * size
    elif case == "tail-loop":
        setup = (r"\def\step{\advance\count0by1\relax\ifnum\count0<"
                 + str(size) + r" \let\next\step\else\let\next\relax\fi\next}")
        work = r"\step"
    elif case == "conversions":
        setup = r"\count1=7\relax\def\digits{\number\number\count1}"
        work = (r"\advance\count0by\digits\relax" + "\n") * size
        factor, other = 7, 7
    elif case == "definitions":
        body = r"\def\value{12" + r"\relax" * 32 + r"}\count1=\value\advance\count0by\count1\relax"
        work = (body + "\n") * size
        factor, other = 12, 12
    elif case == "messages":
        setup = r"\def\text{Alpha {Beta} \number\count0 Gamma}"
        work = (r"\message{\text}\advance\count0by1\relax" + "\n") * size
    else:
        raise ValueError(case)
    source = (BOOT + setup + "\n" + work
              + r"\message{UMBER:\the\count0,\the\count1:END}\end" + "\n")
    return source.encode(), f"{factor * size},{other}".encode()


def main():
    folder = ROOT / "target/benchmark/producer"
    run_suite(program, cases=CASES, sizes=[100000], output=folder,
              report=folder / "results.json", startup="0,0", generator=Path(__file__),
              max_size=2147483647 // 12)


if __name__ == "__main__":
    main()
