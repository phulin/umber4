"""Compare complete detached document output with immutable native DVI."""
import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import struct
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
TEXMF = Path("/home/phulin/umber/third_party/texlive-20260301-texmf/texmf-dist")
UNITS = (25400000, 473628672, 1000)


class Reader:
    def __init__(self, data, position=0):
        self.data, self.position = data, position

    def bytes(self, n):
        end = self.position + n
        if end > len(self.data):
            raise ValueError("truncated DVI")
        result = self.data[self.position:end]
        self.position = end
        return result

    def take(self, n=1, signed=False):
        return int.from_bytes(self.bytes(n), "big", signed=signed)


def font_definition(reader, opcode):
    number = reader.take(opcode - 242)
    checksum, size, design = (reader.take(4) for _ in range(3))
    area, name = reader.take(), reader.take()
    name = reader.bytes(area + name)
    if name.endswith(b".tfm"):
        name = name[:-4]
    return number, (name, size, checksum, design)


def postamble_fonts(data):
    end = len(data)
    while end and data[end - 1] == 223:
        end -= 1
    if len(data) - end < 4 or end < 6 or data[end - 1] != 2 or data[end - 6] != 249:
        raise ValueError("invalid DVI trailer")
    reader = Reader(data, int.from_bytes(data[end - 5:end - 1], "big"))
    if reader.take() != 248:
        raise ValueError("invalid DVI postamble pointer")
    reader.take(4)  # last BOP
    if tuple(reader.take(4) for _ in range(3)) != UNITS:
        raise ValueError("comparison requires standard scaled-point DVI units and mag1000")
    reader.bytes(10)  # maximum height/width and stack depth
    page_count = reader.take(2)
    fonts = {}
    while True:
        opcode = reader.take()
        if opcode == 249:
            return fonts, page_count
        if not 243 <= opcode <= 246:
            raise ValueError(f"invalid postamble opcode {opcode}")
        number, definition = font_definition(reader, opcode)
        if number in fonts and fonts[number] != definition:
            raise ValueError(f"inconsistent DVI font {number}")
        fonts[number] = definition


def scaled_widths(data, size, checksum):
    """TeX82 store_scaled, independently applied to the raw TFM width table."""
    if len(data) < 32:
        raise ValueError("truncated TFM")
    lf, lh, bc, ec, nw, *_ = struct.unpack(">12H", data[:24])
    logical_end = lf * 4
    if len(data) < logical_end or lh < 2 or ec > 255 or bc > ec + 1 or nw == 0:
        raise ValueError("invalid TFM header")
    actual_checksum = int.from_bytes(data[24:28], "big")
    if checksum and actual_checksum and checksum != actual_checksum:
        raise ValueError("DVI/TFM checksum mismatch")
    chars = 24 + lh * 4
    widths = chars + (ec - bc + 1) * 4
    if widths + nw * 4 > logical_end or not 0 < size < (1 << 27):
        raise ValueError("invalid TFM widths or DVI font size")
    z, alpha = size, 16
    while z >= (1 << 23):
        z //= 2
        alpha *= 2
    beta, alpha = 256 // alpha, alpha * z
    result = {}
    for character in range(bc, ec + 1):
        index = data[chars + (character - bc) * 4]
        if index == 0:
            continue
        if index >= nw:
            raise ValueError("invalid TFM width index")
        a, b, c, d = data[widths + index * 4:widths + index * 4 + 4]
        if a not in (0, 255):
            raise ValueError("invalid TFM fix_word")
        value = (((d * z // 256 + c * z) // 256) + b * z) // beta
        result[character] = value - (alpha if a == 255 else 0)
    return result


def dvi_pages(data, tfms, nonvisual_specials=()):
    """Read ordered glyph baselines/rule top-lefts; never use Umber metrics."""
    fonts, page_count = postamble_fonts(data)
    widths = {}
    for number, (name, size, checksum, _) in fonts.items():
        if name not in tfms:
            raise ValueError(f"missing TFM bundle entry {name!r}")
        widths[number] = scaled_widths(tfms[name], size, checksum)
    reader = Reader(data)
    if reader.take() != 247 or reader.take() != 2:
        raise ValueError("expected DVI2 preamble")
    if tuple(reader.take(4) for _ in range(3)) != UNITS:
        raise ValueError("comparison requires standard scaled-point DVI units and mag1000")
    reader.bytes(reader.take())
    pages, stack, pos, font, page = [], [], [0] * 6, None, None
    while True:
        opcode = reader.take()
        if opcode <= 137:
            if page is None:
                raise ValueError("painting outside DVI page")
            if opcode in (132, 137):
                height, width = reader.take(4, True), reader.take(4, True)
                if height > 0 and width > 0:
                    page["events"].append(["r", pos[0], pos[1] - height, width, height])
                if opcode == 132:
                    pos[0] += width
            else:
                character = opcode if opcode <= 127 else reader.take(
                    opcode - (127 if opcode <= 131 else 132))
                if font not in fonts or character not in widths[font]:
                    raise ValueError(f"undefined DVI glyph {font}:{character}")
                name, size, _, _ = fonts[font]
                page["events"].append(["g", list(name), size, character, pos[0], pos[1]])
                if opcode <= 131:
                    pos[0] += widths[font][character]
        elif opcode == 138:
            pass
        elif opcode == 139:
            if page is not None:
                raise ValueError("nested DVI BOP")
            counts = [reader.take(4, True) for _ in range(10)]
            reader.take(4, True)
            pos, stack = [0] * 6, []
            page = {"counts": counts, "events": []}
            pages.append(page)
        elif opcode == 140:
            if page is None or stack:
                raise ValueError("invalid DVI EOP/stack")
            page = None
        elif opcode == 141:
            stack.append(pos.copy())
        elif opcode == 142:
            if not stack:
                raise ValueError("DVI stack underflow")
            pos = stack.pop()
        elif 143 <= opcode <= 146:
            pos[0] += reader.take(opcode - 142, True)
        elif 147 <= opcode <= 156:
            slot, base = (2, 147) if opcode <= 151 else (3, 152)
            if opcode != base:
                pos[slot] = reader.take(opcode - base, True)
            pos[0] += pos[slot]
        elif 157 <= opcode <= 160:
            pos[1] += reader.take(opcode - 156, True)
        elif 161 <= opcode <= 170:
            slot, base = (4, 161) if opcode <= 165 else (5, 166)
            if opcode != base:
                pos[slot] = reader.take(opcode - base, True)
            pos[1] += pos[slot]
        elif 171 <= opcode <= 238:
            font = opcode - 171 if opcode <= 234 else reader.take(opcode - 234)
            if font not in fonts:
                raise ValueError(f"undefined DVI font {font}")
        elif 239 <= opcode <= 242:
            special = reader.bytes(reader.take(opcode - 238))
            if special not in nonvisual_specials:
                raise ValueError(f"unsupported DVI special {special!r}")
        elif 243 <= opcode <= 246:
            number, definition = font_definition(reader, opcode)
            if fonts.get(number) != definition:
                raise ValueError(f"inconsistent inline DVI font {number}")
        elif opcode == 248:
            if page is not None or len(pages) != page_count:
                raise ValueError("DVI page count/end mismatch")
            return pages
        else:
            raise ValueError(f"unsupported DVI opcode {opcode}")


def compare(inspection, native, counts=False):
    pages = inspection["pages"]
    if len(pages) != len(native):
        raise ValueError(f"page count: Umber {len(pages)}, native {len(native)}")
    for number, (page, reference) in enumerate(zip(pages, native), 1):
        if counts and page["counts"][:10] != reference["counts"]:
            raise ValueError(f"page {number} BOP counts: {page['counts'][:10]} != {reference['counts']}")
        expected = reference["events"]
        for index in range(max(len(page["events"]), len(expected))):
            actual = page["events"][index] if index < len(page["events"]) else None
            if actual is not None and actual[0] == "g":
                resource = page["fonts"][actual[1]]
                actual = ["g", resource["name"], resource["size"], *actual[2:]]
            target = expected[index] if index < len(expected) else None
            if actual != target:
                raise ValueError(f"page {number} event {index}: Umber {actual!r}, native {target!r}")


def mapping(value):
    name, separator, path = value.partition("=")
    if not separator or not name or not path:
        raise ValueError(f"expected NAME=PATH: {value!r}")
    return name, Path(path)


def recorded_tfms(recorders):
    result = {}
    for recorder in recorders:
        for line in recorder.read_text().splitlines():
            if line.startswith("INPUT ") and line.endswith(".tfm"):
                path = Path(line[6:])
                if path.stem in result and result[path.stem] != path:
                    raise ValueError(f"ambiguous TFM resource {path.stem}")
                result[path.stem] = path
    return sorted(result.items())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--adapter", type=Path, default=ROOT / "target/release/examples/layout")
    parser.add_argument("--assets-root", type=Path, default=ROOT)
    parser.add_argument("--texmf", type=Path, default=TEXMF)
    parser.add_argument("--wrapper", type=Path)
    parser.add_argument("--dvi", type=Path)
    parser.add_argument("--input", action="append", default=[], type=mapping)
    parser.add_argument("--tfm", action="append", default=[], type=mapping)
    parser.add_argument("--fls", action="append", type=Path)
    parser.add_argument("--source-date-epoch", type=int, default=1772323200)
    parser.add_argument("--compare-counts", action="store_true")
    parser.add_argument("--nonvisual-special", action="append", default=[], type=bytes.fromhex)
    parser.add_argument("--replay", action="store_true")
    parser.add_argument("--allocation-adapter", type=Path)
    parser.add_argument("--ready-message")
    parser.add_argument("--repeats", type=int, default=5)
    parser.add_argument("--iterations", type=int, default=200)
    args = parser.parse_args()
    if args.repeats <= 0 or args.iterations <= 0:
        parser.error("repeats and iterations must be positive")
    if args.ready_message is not None and args.wrapper is None:
        parser.error("--ready-message requires an explicit --wrapper")
    reference = args.assets_root / ".tools/corpus/gentle-native-pdftex14025-tl2026"
    inputs = args.input or [
        ("plain", args.texmf / "tex/plain/base/plain.tex"),
        ("hyphen", args.texmf / "tex/generic/hyphen/hyphen.tex"),
        ("gentle", args.assets_root / ".tools/corpus/gentle.tex"),
    ]
    recorders = args.fls or [reference / "format/gentle-plain.fls", reference / "run/gentle.fls"]
    tfms = args.tfm or recorded_tfms(recorders)
    if not args.tfm and not args.fls and len(tfms) != 47:
        raise ValueError(f"expected retained 47-TFM union, found {len(tfms)}")
    native = dvi_pages((args.dvi or reference / "run/gentle.dvi").read_bytes(),
                       {name.removesuffix(".tfm").encode(): path.read_bytes() for name, path in tfms},
                       args.nonvisual_special)
    calendar = datetime.fromtimestamp(args.source_date_epoch, timezone.utc)
    report = {"pages": len(native), "glyphs": sum(
        event[0] == "g" for page in native for event in page["events"]), "rules": sum(
        event[0] == "r" for page in native for event in page["events"]),
        "comparison": "all ordered events match", "bop_counts": args.compare_counts}
    with tempfile.TemporaryDirectory(prefix="umber-gentle-inspect-") as temporary:
        wrapper = args.wrapper
        ready = args.ready_message
        if wrapper is None:
            wrapper = Path(temporary) / "gentle.tex"
            if args.replay or args.allocation_adapter:
                ready = "UMBER:DOCUMENT:READY:END"
                wrapper.write_text(r"\input plain \message{" + ready + r"} \input gentle \end" + "\n")
            else:
                wrapper.write_bytes(b"\\input plain \\input gentle \\end\n")
        command = [str(args.adapter), "document-inspect", str(wrapper), "--calendar",
                   f"{calendar.hour * 60 + calendar.minute},{calendar.day},{calendar.month},{calendar.year}"]
        for option, bundle in [("--input", inputs), ("--tfm", tfms)]:
            for name, path in bundle:
                command.extend([option, f"{name}={path}"])
        inspection = json.loads(subprocess.check_output(command))
        compare(inspection, native, args.compare_counts)
        prepared_args = [*command[2:], "--repeats", str(args.repeats),
                         "--iterations", str(args.iterations)]
        if ready is not None:
            prepared_args.extend(["--ready-message", ready])
        if args.replay:
            report["prepared_replay"] = json.loads(subprocess.check_output(
                [str(args.adapter), "document-replay", *prepared_args]))
        if args.allocation_adapter:
            report["prepared_allocations"] = json.loads(subprocess.check_output(
                [str(args.allocation_adapter), "document-alloc", *prepared_args]))
    print(json.dumps(report))


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        raise SystemExit(f"document adapter failed (exit {error.returncode}); comparison/measurements incomplete") from error
    except (ValueError, OSError) as error:
        raise SystemExit(str(error)) from error
