#!/usr/bin/env python3
"""Run the experimental three-term arithmetic on real Metal, without Mandel source."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import random
import struct
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def split3(value):
    if not math.isfinite(value):
        raise ValueError("precision inputs must be finite")
    parts = []
    for _ in range(3):
        try:
            part = struct.unpack("<f", struct.pack("<f", value))[0]
        except OverflowError as error:
            raise ValueError("input exceeds float expansion exponent range") from error
        if not math.isfinite(part):
            raise ValueError("input exceeds float expansion exponent range")
        parts.append(part)
        value -= part
    return parts


def samples():
    values = [(0.0, 1.0), (1.0, -1.0), (1.0 + 2**-40, -1.0),
              (-19.6377709180615, 19.6377709180615 + 2**-40)]
    rng = random.Random(48)
    for _ in range(1024):
        a = math.ldexp(rng.uniform(.5, 1), rng.randrange(-16, 17))
        b = math.ldexp(rng.uniform(.5, 1), rng.randrange(-16, 17))
        values.append((a * rng.choice([-1, 1]), b * rng.choice([-1, 1])))
    return values


def compare(values, raw):
    if len(raw) != len(values) * 32 or not values:
        raise ValueError("incorrect or empty result length")
    errors = [[], [], [], []]
    for (a, b), row in zip(values, struct.iter_unpack("<8f", raw)):
        expected = [a + b, a * b, a / b, math.sqrt(abs(a))]
        for i, reference in enumerate(expected):
            actual = float(row[2 * i]) + float(row[2 * i + 1])
            if not math.isfinite(actual):
                raise ValueError("nonfinite GPU result")
            error = abs(actual - reference) / abs(reference) if reference else abs(actual)
            errors[i].append(error)
    maxima = dict(zip(["sum", "product", "quotient", "root"], map(max, errors)))
    return {"passed": all(e < 2e-13 for e in maxima.values()), "max_errors": maxima}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True, help="New report directory")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    support = ROOT / "examples/precision"
    source = (support / "Expansion.metal").read_text() + """
using namespace fpt_precision;
kernel void probe(device const float *p [[buffer(0)]], device float *out [[buffer(1)]],
                  uint i [[thread_position_in_grid]]) {
    Scalar a(p[i*9],p[i*9+1],p[i*9+2]), b(p[i*9+3],p[i*9+4],p[i*9+5]);
    Scalar results[4]={a+b,a*b,a/b,root(absolute(a))};
    for(int j=0;j<4;++j) {
        out[i*8+j*2]=results[j].hi;
        out[i*8+j*2+1]=results[j].mid+results[j].lo;
    }
}
"""
    (args.out / "probe.metal").write_text(source)
    values = samples()
    (args.out / "input.bin").write_bytes(b"".join(
        struct.pack("<9f", *split3(a), *split3(b), 0, 0, 0) for a, b in values))
    commands = [["clang++", "-std=c++17", "-O2", "-fobjc-arc", str(support / "probe.mm"),
                 "-framework", "Foundation", "-framework", "Metal", "-o", str(args.out / "probe")]]
    subprocess.run(commands[0], check=True)
    metadata = []
    for suffix in ["first", "repeat"]:
        command = [str(args.out / "probe"), str(args.out / "probe.metal"),
                   str(args.out / "input.bin"), str(args.out / f"{suffix}.bin")]
        commands.append(command)
        result = subprocess.run(command, check=True, capture_output=True, text=True)
        metadata.append(json.loads(result.stdout))
    raw = (args.out / "first.bin").read_bytes()
    repeat = raw == (args.out / "repeat.bin").read_bytes()
    report = compare(values, raw)
    report.update(samples=len(values), operations=len(values) * 4, repeat_byte_exact=repeat,
                  scope="Experimental finite-range arithmetic only; not a renderer or full IEEE implementation",
                  pipeline=metadata, commands=commands,
                  source_sha256=hashlib.sha256(source.encode()).hexdigest(),
                  driver_sha256=hashlib.sha256((support / "probe.mm").read_bytes()).hexdigest())
    report["passed"] = report["passed"] and repeat
    (args.out / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    raise SystemExit(0 if report["passed"] else 1)


if __name__ == "__main__":
    main()
