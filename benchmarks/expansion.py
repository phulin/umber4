"""Validated macro workloads from Gentle, plus token lists and name working sets.

These are expansion kernels, not a compilation of the whole manual. Every
record is expanded into a definition and compared with independently generated
expected tokens. This validation is part of the measured work.
"""

import hashlib
import json
from pathlib import Path

from mvp import BOOT, ROOT, run_suite

CORPUS = Path(__file__).resolve().parent / "corpus"
KERNELS = (CORPUS / "gentle-kernels.tex").read_text()
CASES = ("gentle-text", "gentle-calendar", "gentle-contents", "gentle-records",
         "stored-records", "token-pipeline", "name-table", "optional-arguments",
         "numeric-references")
SUPPLEMENTAL_CASES = ("register-meanings",)
MONTHS = ("January February March April May June July August September October "
          "November December").split()
SPELLINGS = (("analogue", "analog"), ("cancelled", "canceled"),
             ("centimetre", "centimeter"), ("centre", "center"),
             ("centred", "centered"), ("our", "or"),
             ("postalcode", "zipcode"), ("province", "state"),
             ("theatre", "theater"))
ALIASES = ("analog", "canceled", "centimeter", "center", "centered",
           "our", "postcode", "province", "theater")
NAMES = ("John", "Jane", "Michael", "Alice", "She", "We", "The author")
LENGTHS = (8, 32, 128, 256, 1024)
CHECK = r"\ifx\actual\wanted\advance\count0by1\relax" \
        r"\else\message{UMBER:MISMATCH:END}\end\fi" + "\n"


def payload(index):
    # Distinct deterministic text, balanced groups, spaces and a nested macro.
    alphabet = "abcdefghijklmnopqrstuvwxyz"
    word = "".join(alphabet[(index * 7 + k * 11) % 26] for k in range(19))
    text = (word + " ") * (LENGTHS[index % len(LENGTHS)] // 20 + 1)
    return text[:LENGTHS[index % len(LENGTHS)]].rstrip()


def roman(number):
    text = ""
    for value, digits in ((1000, "m"), (900, "cm"), (500, "d"), (400, "cd"),
                          (100, "c"), (90, "xc"), (50, "l"), (40, "xl"),
                          (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")):
        copies, number = divmod(number, value)
        text += digits * copies
    return text


def wanted(text):
    return r"\def\wanted{" + text + "}\n"


def actual(text):
    return r"\edef\actual{" + text + "}\n"


def program(case, size):
    lines = [BOOT, r"\def\space{ }\let\ifcanspell=\iftrue\countdef\pageno=1",
             "\n", KERNELS, "\n"]
    expanded_bytes = 0
    numeric_keys = ([100 + key for key in range(64)]
                    + [10000 + key for key in range(64)]
                    + [10000000 + key for key in range(64)]
                    + [-2000000000 - key for key in range(64)])
    if case == "numeric-references":
        for key, value in enumerate(numeric_keys):
            lines.append(r"\expandafter\def\csname ref" + str(value)
                         + r"\endcsname{entry" + str(key) + "}\n")
    if case == "gentle-contents":
        # Read literal expected backslashes as characters. The macro itself is
        # already tokenized; it restores the source category after its argument.
        lines.append(r"\def\expected#1{\edef\wanted{\detokenize{#1}}"
                     r"\catcode92=0\relax}"
                     r"\def\literal{\catcode92=12\relax\expected}" + "\n")
    if case == "token-pipeline":
        lines.append(r"\toksdef\left=0\toksdef\right=1\toksdef\joined=2"
                     r"\def\wrap#1{(#1)}\def\setflag{\count9=1\relax}" + "\n")
    if case == "stored-records":
        lines.append(r"\long\def\forward#1{\maketable#1}" + "\n")
    if case == "optional-arguments":
        lines.append(r"\let\open=[\def\parse{\futurelet\next\dispatch}"
                     r"\def\dispatch{\unless\ifx\next\open\let\handler=\without"
                     r"\else\let\handler=\with\fi\handler}"
                     r"\def\with[#1]#2{\finish{#1}{#2}}"
                     r"\def\without#1{\finish{normal}{#1}}"
                     r"\def\finish#1#2{\edef\actual{#1:#2/"
                     r"\number\numexpr(\count0+3)*7/2\relax}}" + "\n")
    width = min(4096, max(64, size // 8))
    table = {}
    if case == "name-table":
        for key in range(width):
            text = payload(key)[:64]
            table[key] = text
            lines.append(r"\expandafter\def\csname field" + str(key)
                         + r"\endcsname{" + text + "}\n")
    for index in range(size):
        canadian = (index // len(SPELLINGS)) % 2 == 0
        spelling_index = (index * 7) % len(SPELLINGS)
        spelling = SPELLINGS[spelling_index][0 if canadian else 1]
        alias = "\\" + ALIASES[spelling_index]
        if case not in ("name-table", "numeric-references", "register-meanings"):
            lines.append(r"\let\ifcanspell=" + (r"\iftrue" if canadian else r"\iffalse")
                         + "\n")
        if case == "gentle-text":
            a, b = NAMES[index % len(NAMES)], NAMES[(index * 3 + 2) % len(NAMES)]
            lines.append(r"\def\person{" + a + "}\n")
            text = f"{a} talks to {b}. {spelling}/{spelling}."
            lines.extend([wanted(text), actual(r"\talks{\person}{" + b + "} "
                          + alias + "/" + alias + ".")])
        elif case == "gentle-calendar":
            month, day, year = index % 12 + 1, index % 28 + 1, 1993 + index % 35
            lines.append(f"\\month={month} \\day={day} \\year={year}\\relax\n")
            text = f"{MONTHS[month - 1]} {day}, {year}"
            lines.extend([wanted(text), actual(r"\today")])
        elif case == "gentle-contents":
            page = -(index % 39 + 1) if index % 4 == 0 else index % 900 + 1
            number = roman(-page) if page < 0 else str(page)
            title = f"{spelling} chapter {index % 53} " + payload(index)[:128]
            text = f"\\line{{{title} \\dotfill{{}} {number}}}"
            lines.append(f"\\pageno={page}\\relax\n")
            lines.append(r"\literal{" + text + "}\n")
            lines.append(actual(r"\contentsentry{" + alias + r"\space" + f" chapter {index % 53} "
                                + payload(index)[:128] + "}"))
            lines.append(actual(r"\expandafter\detokenize\expandafter{\actual}"))
        elif case in ("gentle-records", "stored-records"):
            title = f"{spelling} table {index % 71}"
            body = payload(index) + " {" + payload(index + 1)[:32] + "} " + spelling
            text = f"[{title}]|[{body}]"
            lines.append(wanted(text))
            arguments = "[" + alias + r"\space" + f" table {index % 71}]" \
                + payload(index) + " {" + payload(index + 1)[:32] \
                + "} " + alias + r"\par"
            if case == "gentle-records":
                lines.append(r"\maketable" + arguments + "\n")
            elif index % 2:
                lines.append(r"\def\call{\maketable" + arguments + r"}\call" + "\n")
            else:
                lines.append(r"\forward{" + arguments + "}\n")
        elif case == "token-pipeline":
            field = payload(index)
            later_field = payload(index + 5)
            text = f"({later_field})|{spelling}|({later_field})"
            lines.append(r"\def\field{" + field + "}\n")
            # Register copies share storage; the saved values outlive overwrites.
            lines.append(r"\count9=0\relax\afterassignment\setflag\left={\wrap{\field}}"
                         r"\right=\left\relax"
                         r"{\left={discarded}\global\joined=\right\relax}"
                         r"\edef\pipeline{\the\right|" + alias
                         + r"|\the\joined}" + "\n")
            lines.append(r"\def\field{" + later_field + "}\n")
            text += "/1"
            lines.extend([wanted(text), actual(r"\pipeline/\the\count9")])
        elif case == "name-table":
            key = (index * 4051 + 17) % width
            if index % 17 == 0:
                table[key] = payload(index + 9)[:64]
                lines.append(r"\expandafter\def\csname field" + str(key)
                             + r"\endcsname{" + table[key] + "}\n")
            present = index % 13 != 0
            name = "field" + str(key) if present else "missing" + str(index % 64)
            text = table[key] if present else "fallback"
            lines.extend([wanted(text), actual(r"\ifcsname " + name + r"\endcsname"
                          r"\csname " + name + r"\endcsname\else fallback\fi")])
        elif case == "numeric-references":
            key = (index * 61 + 17) % len(numeric_keys)
            value = numeric_keys[key]
            text = f"entry{key}/{value}"
            lines.append(r"\count1=" + str(value) + r"\relax" + "\n")
            lines.extend([wanted(text), actual(
                r"\ifcsname ref\number\count1\endcsname"
                r"\csname ref\number\count1\endcsname\else missing\fi/"
                r"\number\count1")])
        elif case == "register-meanings":
            count_index, token_index = (index * 61 + 17) % 256, (index * 103 + 29) % 256
            escape = (92, 33, -1)[index % 3]
            prefix = chr(escape) if escape >= 0 else ""
            text = f"{prefix}count{count_index}/{prefix}toks{token_index}"
            lines.append(f"\\escapechar={escape} \\countdef\\reg={count_index} "
                         f"\\toksdef\\treg={token_index}\\relax\n")
            lines.extend([actual(r"\meaning\reg/\meaning\treg"),
                          r"\edef\wanted{\string\count" + str(count_index)
                          + r"/\string\toks" + str(token_index) + "}\n"])
        elif case == "optional-arguments":
            option = spelling if index % 4 else "normal"
            body = payload(index) + " {" + payload(index + 1)[:32] + "}"
            number = (7 * (index + 3) + 1) // 2
            text = f"{option}:{body}/{number}"
            lines.append(wanted(text))
            invocation = r"\parse" + ("[" + alias + "]" if index % 4 else "")
            invocation += "{" + body + "}"
            if index % 2:
                lines.append(r"\def\call{" + invocation + r"}\call" + "\n")
            else:
                lines.append(invocation + "\n")
        else:
            raise ValueError(case)
        expanded_bytes += len(text)
        lines.append(CHECK)
    lines.append(r"\message{UMBER:\the\count0:END}\end" + "\n")
    return "".join(lines).encode(), str(size).encode(), {
        "expanded_text_bytes": expanded_bytes,
        "name_table_width": width if case == "name-table" else None,
        "numeric_reference_width": len(numeric_keys) if case == "numeric-references" else None,
    }


def main():
    output = ROOT / "target/benchmark/expansion"
    run_suite(program, cases=CASES + SUPPLEMENTAL_CASES, default_cases=CASES,
              sizes=[1000, 10000, 50000], max_size=1000000, output=output,
              report=output / "expansion.json", startup="0", generator=Path(__file__),
              metadata={
                  "kernels_sha256": hashlib.sha256((CORPUS / "gentle-kernels.tex").read_bytes()).hexdigest(),
                  "corpus": json.loads((CORPUS / "gentle.json").read_text()),
                  "validation": "Every record compares its expanded definition with independently generated expected tokens; comparison and expected definition construction are timed.",
                  "storage": "Immutable definitions and token lists are reclaimed after their last semantic owner; bump chunks and value IDs are reused. Interned names remain session-owned.",
              })


if __name__ == "__main__":
    main()
