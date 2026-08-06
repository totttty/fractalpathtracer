#!/usr/bin/env python3
"""Compare a Mandel optimization cohort against its frozen PNG baselines."""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
from statistics import median
from typing import Any

from PIL import Image, ImageChops


def image_metrics(baseline: Path, candidate: Path) -> dict[str, float | int]:
    with Image.open(baseline) as baseline_image, Image.open(candidate) as candidate_image:
        first = baseline_image.convert("RGB")
        second = candidate_image.convert("RGB")
        if first.size != second.size:
            raise ValueError(f"image dimensions differ: {first.size} != {second.size}")
        difference = ImageChops.difference(first, second)
        extrema = difference.getextrema()
        maximum = max(channel[1] for channel in extrema)
        histogram = difference.histogram()
        channel_values = first.width * first.height * 3
        absolute_sum = sum((index % 256) * count for index, count in enumerate(histogram))
        squared_sum = sum(
            ((index % 256) ** 2) * count for index, count in enumerate(histogram)
        )
        changed = sum(
            1
            for pixel in difference.getdata()
            if pixel[0] != 0 or pixel[1] != 0 or pixel[2] != 0
        )
        return {
            "width": first.width,
            "height": first.height,
            "mean_absolute_error": absolute_sum / channel_values,
            "root_mean_square_error": math.sqrt(squared_sum / channel_values),
            "max_absolute_error": maximum,
            "changed_pixels": changed,
            "changed_pixel_percent": 100.0 * changed / (first.width * first.height),
        }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("baseline", type=Path)
    parser.add_argument("candidate_report", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()

    baseline_report = json.loads(args.baseline.read_text(encoding="utf-8"))
    candidate = json.loads(args.candidate_report.read_text(encoding="utf-8"))
    if "scenes" in baseline_report:
        frozen = {
            int(scene["corpus_index"]): scene for scene in baseline_report["scenes"]
        }
    else:
        frozen = {
            int(entry["corpus_index"]): {
                "corpus_index": entry["corpus_index"],
                "baseline_status": entry["status"],
                "baseline_gpu_ms": entry.get("gpu_ms"),
                "baseline_image": entry.get("output"),
            }
            for entry in baseline_report["entries"]
        }
    entries: list[dict[str, Any]] = []
    for current in candidate["entries"]:
        index = int(current["corpus_index"])
        baseline = frozen[index]
        record: dict[str, Any] = {
            "corpus_index": index,
            "path": current["path"],
            "baseline_status": baseline["baseline_status"],
            "candidate_status": current["status"],
            "baseline_gpu_ms": baseline.get("baseline_gpu_ms"),
            "candidate_gpu_ms": current.get("gpu_ms"),
            "dispatch_mode": current.get("dispatch_mode"),
        }
        if baseline.get("baseline_gpu_ms") and current.get("gpu_ms"):
            record["speedup"] = float(baseline["baseline_gpu_ms"]) / float(
                current["gpu_ms"]
            )
        if (
            baseline["baseline_status"] == "ok"
            and current["status"] == "ok"
            and baseline.get("baseline_image")
            and current.get("output")
        ):
            record["image"] = image_metrics(
                Path(baseline["baseline_image"]), Path(current["output"])
            )
        entries.append(record)

    comparable = [entry for entry in entries if "image" in entry]
    exact = [
        entry
        for entry in comparable
        if entry["image"]["changed_pixels"] == 0
    ]
    speedups = [float(entry["speedup"]) for entry in entries if "speedup" in entry]
    result = {
        "schema_version": 1,
        "baseline": str(args.baseline.resolve()),
        "candidate_report": str(args.candidate_report.resolve()),
        "scenes": len(entries),
        "comparable_images": len(comparable),
        "pixel_exact_images": len(exact),
        "image_failures": len(comparable) - len(exact),
        "median_speedup": median(speedups) if speedups else None,
        "minimum_speedup": min(speedups) if speedups else None,
        "maximum_speedup": max(speedups) if speedups else None,
        "entries": entries,
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(
        f'{len(exact)}/{len(comparable)} comparable images pixel-exact; '
        f'median speedup {result["median_speedup"]:.3f}x'
    )
    return 0 if len(exact) == len(comparable) else 1


if __name__ == "__main__":
    raise SystemExit(main())
