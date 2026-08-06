#!/usr/bin/env python3
"""Freeze a reproducible Mandel performance and visual-parity cohort."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("report", type=Path)
    parser.add_argument("audit", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--slow", type=int, default=30)
    parser.add_argument("--controls", type=int, default=10)
    args = parser.parse_args()

    report = json.loads(args.report.read_text(encoding="utf-8"))
    audit = json.loads(args.audit.read_text(encoding="utf-8"))
    audit_by_index = {
        index: entry for index, entry in enumerate(audit["entries"], start=1)
    }
    ranked = report["ranked_entries"]
    successful_by_index = {entry["corpus_index"]: entry for entry in ranked}

    selected: list[tuple[str, dict[str, Any]]] = []
    selected.extend(("slow", entry) for entry in ranked[: args.slow])
    selected.extend(("failure", entry) for entry in report["failed_entries"])
    if ranked and args.controls > 0:
        middle = len(ranked) // 2
        half = args.controls // 2
        median_controls = ranked[max(middle - half, 0) :][: args.controls]
        selected.extend(("median-control", entry) for entry in median_controls)
        selected.extend(("fast-control", entry) for entry in ranked[-args.controls :])

    records: list[dict[str, Any]] = []
    seen: set[int] = set()
    for cohort, benchmark in selected:
        corpus_index = int(benchmark["corpus_index"])
        if corpus_index in seen:
            continue
        seen.add(corpus_index)
        scene_audit = audit_by_index[corpus_index]
        successful = successful_by_index.get(corpus_index)
        records.append(
            {
                "cohort": cohort,
                "corpus_index": corpus_index,
                "path": benchmark["path"],
                "formula_ids": scene_audit.get("formula_ids", []),
                "hybrid": scene_audit.get("hybrid"),
                "baseline_status": benchmark["status"],
                "baseline_gpu_ms": successful.get("gpu_ms") if successful else None,
                "baseline_fps": successful.get("fps") if successful else None,
                "baseline_image": successful.get("output") if successful else None,
                "baseline_metadata": successful.get("metadata") if successful else None,
                "baseline_error": benchmark.get("error"),
            }
        )

    manifest = {
        "schema_version": 1,
        "source_report": str(args.report.resolve()),
        "source_audit": str(args.audit.resolve()),
        "settings": report["settings"],
        "acceptance_gates": {
            "silhouette_iou": 1.0,
            "new_hit_miss_differences": 0,
            "normal_cosine_minimum": 0.9999,
            "material_channel_max_error_8bit": 1,
            "path_ssim_minimum": 0.9999,
            "path_mae_maximum_8bit": 0.25,
            "targeted_gpu_improvement_minimum": 0.10,
            "control_gpu_regression_maximum": 0.02,
        },
        "scenes": records,
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    counts: dict[str, int] = {}
    for record in records:
        counts[record["cohort"]] = counts.get(record["cohort"], 0) + 1
    print(f"wrote {len(records)} scenes to {args.out}: {counts}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
