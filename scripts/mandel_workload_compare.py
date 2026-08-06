#!/usr/bin/env python3
"""Compare path-traced and diffuse-normal Mandel corpus GPU timings."""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
from statistics import fmean, median
from typing import Any


def percentile(values: list[float], fraction: float) -> float:
    ordered = sorted(values)
    position = fraction * (len(ordered) - 1)
    lower = math.floor(position)
    upper = math.ceil(position)
    if lower == upper:
        return ordered[lower]
    weight = position - lower
    return ordered[lower] * (1.0 - weight) + ordered[upper] * weight


def timing_summary(values: list[float]) -> dict[str, float]:
    return {
        "minimum_ms": min(values),
        "median_ms": median(values),
        "mean_ms": fmean(values),
        "p90_ms": percentile(values, 0.90),
        "p95_ms": percentile(values, 0.95),
        "maximum_ms": max(values),
    }


def successful_entries(report: dict[str, Any]) -> dict[int, dict[str, Any]]:
    return {
        int(entry["corpus_index"]): entry
        for entry in report["entries"]
        if entry.get("status") == "ok" and entry.get("gpu_ms")
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("pathtrace_report", type=Path)
    parser.add_argument("diffuse_report", type=Path)
    parser.add_argument("--selected-hybrid-report", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()

    path_report = json.loads(args.pathtrace_report.read_text(encoding="utf-8"))
    diffuse_report = json.loads(args.diffuse_report.read_text(encoding="utf-8"))
    path_entries = successful_entries(path_report)
    if args.selected_hybrid_report:
        selected = json.loads(
            args.selected_hybrid_report.read_text(encoding="utf-8")
        )
        for index, entry in successful_entries(selected).items():
            metadata_path = entry.get("metadata")
            if not metadata_path:
                continue
            metadata = json.loads(Path(metadata_path).read_text(encoding="utf-8"))
            if metadata.get("mandel_formula_dispatch_mode") == "direct-homogeneous":
                path_entries[index] = entry
    diffuse_entries = successful_entries(diffuse_report)

    entries: list[dict[str, Any]] = []
    for index in sorted(path_entries.keys() & diffuse_entries.keys()):
        path_entry = path_entries[index]
        diffuse_entry = diffuse_entries[index]
        path_ms = float(path_entry["gpu_ms"])
        diffuse_ms = float(diffuse_entry["gpu_ms"])
        entries.append(
            {
                "corpus_index": index,
                "path": diffuse_entry["path"],
                "pathtrace_gpu_ms": path_ms,
                "pathtrace_fps": 1000.0 / path_ms,
                "diffuse_gpu_ms": diffuse_ms,
                "diffuse_fps": 1000.0 / diffuse_ms,
                "diffuse_to_pathtrace_fraction": diffuse_ms / path_ms,
            }
        )

    path_values = [entry["pathtrace_gpu_ms"] for entry in entries]
    diffuse_values = [entry["diffuse_gpu_ms"] for entry in entries]
    fractions = [entry["diffuse_to_pathtrace_fraction"] for entry in entries]
    path_logs = [math.log(value) for value in path_values]
    diffuse_logs = [math.log(value) for value in diffuse_values]
    path_mean = fmean(path_logs)
    diffuse_mean = fmean(diffuse_logs)
    covariance = sum(
        (path - path_mean) * (diffuse - diffuse_mean)
        for path, diffuse in zip(path_logs, diffuse_logs)
    )
    variance_product = sum((value - path_mean) ** 2 for value in path_logs) * sum(
        (value - diffuse_mean) ** 2 for value in diffuse_logs
    )

    report = {
        "schema_version": 1,
        "pathtrace_report": str(args.pathtrace_report.resolve()),
        "diffuse_report": str(args.diffuse_report.resolve()),
        "selected_hybrid_report": (
            str(args.selected_hybrid_report.resolve())
            if args.selected_hybrid_report
            else None
        ),
        "matched_scenes": len(entries),
        "pathtrace_successful_scenes": len(path_entries),
        "diffuse_successful_scenes": len(diffuse_entries),
        "diffuse_failed_scenes": diffuse_report["failed_scenes"],
        "pathtrace": timing_summary(path_values),
        "diffuse": timing_summary(diffuse_values),
        "diffuse_to_pathtrace_fraction": {
            "median": median(fractions),
            "mean": fmean(fractions),
            "p90": percentile(fractions, 0.90),
            "p95": percentile(fractions, 0.95),
            "scenes_above_one": sum(value > 1.0 for value in fractions),
        },
        "log_gpu_time_pearson_correlation": covariance
        / math.sqrt(variance_product),
        "diffuse_failed_entries": diffuse_report["failed_entries"],
        "entries": sorted(
            entries,
            key=lambda entry: (entry["diffuse_gpu_ms"], entry["path"]),
            reverse=True,
        ),
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

    lines = [
        "# Mandel path-traced versus diffuse-normal GPU timing",
        "",
        f"Matched scenes: {len(entries)}.",
        "",
        "| Metric | Path traced | Diffuse normal |",
        "| :--- | ---: | ---: |",
    ]
    for label, key in (
        ("Median", "median_ms"),
        ("Mean", "mean_ms"),
        ("P90", "p90_ms"),
        ("P95", "p95_ms"),
        ("Maximum", "maximum_ms"),
    ):
        lines.append(
            f'| {label} GPU ms | {report["pathtrace"][key]:.3f} | '
            f'{report["diffuse"][key]:.3f} |'
        )
    lines.extend(
        [
            "",
            (
                "Median diffuse/path-traced fraction: "
                f'{report["diffuse_to_pathtrace_fraction"]["median"]:.3f}. '
                "Log-time Pearson correlation: "
                f'{report["log_gpu_time_pearson_correlation"]:.3f}.'
            ),
            "",
            "## Slowest diffuse scenes",
            "",
            "| Rank | Corpus | Scene | Diffuse ms | FPS | Path ms | Fraction |",
            "| ---: | ---: | :--- | ---: | ---: | ---: | ---: |",
        ]
    )
    for rank, entry in enumerate(report["entries"][:20], start=1):
        lines.append(
            f'| {rank} | {entry["corpus_index"]:04d} | '
            f'{Path(entry["path"]).name.replace("|", "\\|")} | '
            f'{entry["diffuse_gpu_ms"]:.3f} | {entry["diffuse_fps"]:.2f} | '
            f'{entry["pathtrace_gpu_ms"]:.3f} | '
            f'{entry["diffuse_to_pathtrace_fraction"]:.3f} |'
        )
    fastest = list(reversed(report["entries"][-20:]))
    lines.extend(
        [
            "",
            "## Fastest diffuse scenes",
            "",
            "| Rank | Corpus | Scene | Diffuse ms | FPS | Path ms | Fraction |",
            "| ---: | ---: | :--- | ---: | ---: | ---: | ---: |",
        ]
    )
    for rank, entry in enumerate(fastest, start=1):
        lines.append(
            f'| {rank} | {entry["corpus_index"]:04d} | '
            f'{Path(entry["path"]).name.replace("|", "\\|")} | '
            f'{entry["diffuse_gpu_ms"]:.3f} | {entry["diffuse_fps"]:.2f} | '
            f'{entry["pathtrace_gpu_ms"]:.3f} | '
            f'{entry["diffuse_to_pathtrace_fraction"]:.3f} |'
        )
    args.out.with_suffix(".md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
