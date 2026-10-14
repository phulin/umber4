"""Supported producer/pull comparison kernels; every engine receives the same source."""
from pathlib import Path
from mvp import BOOT, ROOT, run_suite
CASES = ("handoff", "tail-loop", "nested-bodies")


def program(case, size):
    setup = ""
    if case == "handoff":
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
    else:
        raise ValueError(case)
    source = (BOOT + setup + "\n" + work
              + r"\message{UMBER:\the\count0,\the\count1:END}\end" + "\n")
    return source.encode(), f"{size},{123 if case == 'handoff' else 0}".encode()


def main():
    folder = ROOT / "target/benchmark/producer"
    run_suite(program, cases=CASES, sizes=[100000], output=folder,
              report=folder / "results.json", startup="0,0", generator=Path(__file__))


if __name__ == "__main__":
    main()
