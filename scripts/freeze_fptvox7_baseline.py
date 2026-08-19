#!/usr/bin/env python3
"""Freeze and classify the current FPTVOX7 structural-parity baseline."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_BASELINE = (
    ROOT / "reports/fptvox7-ranked50-exact-v7-r192-m384-20260816/summary.json"
)
DEFAULT_SELECTED = (
    ROOT / "reports/fptvox7-selected-structural-policy-20260816/summary.json"
)
DEFAULT_OUTPUT = ROOT / "reports/fptvox7-frozen-baseline-20260817"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline", type=Path, default=DEFAULT_BASELINE)
    parser.add_argument("--selected", type=Path, default=DEFAULT_SELECTED)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--structure-iou", type=float, default=0.95)
    parser.add_argument("--continuous-iou", type=float, default=0.95)
    return parser.parse_args()


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while block := handle.read(1024 * 1024):
            digest.update(block)
    return digest.hexdigest()


def git(command: str) -> str:
    return subprocess.check_output(
        ["git", *command.split()], cwd=ROOT, text=True
    ).strip()


def classify(row: dict, structure_iou: float, continuous_iou: float) -> str:
    ply_v7 = float(row["ply_v7_visible_iou"])
    cell_iou = row.get("cell_iou")
    if ply_v7 == 0.0 and cell_iou is not None and float(cell_iou) >= 0.99:
        return "degenerate_visibility_metric"
    if ply_v7 < structure_iou:
        return "direct_v7_structure"
    if float(row["fpt_ply_visible_iou"]) < continuous_iou:
        return "continuous_fpt"
    return "pass"


def selected_source_metrics(selected: dict | None) -> dict | None:
    if not selected or selected.get("selection_kind") == "baseline":
        return None
    image = Path(selected["images"]["fpt_geometry"])
    if not image.is_absolute():
        image = ROOT / image
    for parent in image.parents:
        summary = parent / "summary.json"
        if not summary.is_file():
            continue
        contents = json.loads(summary.read_text())
        if not isinstance(contents, list):
            continue
        for row in contents:
            if int(row.get("rank", -1)) == int(selected["rank"]):
                return row
    return None


def main() -> None:
    args = parse_args()
    baseline_rows = json.loads(args.baseline.read_text())
    invalid_reference_rows = [
        int(row["rank"])
        for row in baseline_rows
        if row.get("reference_backend") != "mandelbulber-cpu-double"
    ]
    if invalid_reference_rows:
        raise RuntimeError(
            "baseline is not provenance-safe: expected Mandelbulber CPU/double "
            "reference rows, but the backend is missing or different for ranks "
            + ",".join(map(str, invalid_reference_rows))
        )
    selected_rows = {
        int(row["rank"]): row for row in json.loads(args.selected.read_text())
    }
    rows = []
    for baseline in baseline_rows:
        rank = int(baseline["rank"])
        selected = selected_rows.get(rank)
        source_metrics = selected_source_metrics(selected) or baseline
        if source_metrics.get("reference_backend") != "mandelbulber-cpu-double":
            raise RuntimeError(
                f"selected structural metrics for rank {rank} are not backed by "
                "Mandelbulber CPU/double output"
            )
        row = {
            "rank": rank,
            "name": baseline["name"],
            "selection": selected["selection"] if selected else "baseline",
            "selection_kind": selected["selection_kind"] if selected else "baseline",
            "cell_iou": (
                selected.get("selected_cell_iou")
                if selected
                else baseline.get("cell", {}).get("cell_iou")
            ),
            "ply_v7_visible_iou": (
                selected["selected_ply_v7_visible_iou"]
                if selected
                else baseline["geometry"]["visible_iou"]
            ),
            "ply_v7_visible_miss_pct": (
                selected["selected_visible_miss_pct"]
                if selected
                else baseline["geometry"]["visible_miss_pct"]
            ),
            "ply_v7_visible_extra_pct": (
                selected["selected_visible_extra_pct"]
                if selected
                else baseline["geometry"]["visible_extra_pct"]
            ),
            "fpt_ply_visible_iou": (
                selected["fpt_vs_ply_visible_iou"]
                if selected
                else baseline["fpt_vs_ply_mask"]["visible_iou"]
            ),
            "fpt_v7_visible_iou": (
                selected["fpt_vs_v7_visible_iou"]
                if selected
                else baseline["fpt_vs_v7_mask"]["visible_iou"]
            ),
            "normal_mean_degrees": (
                selected["selected_normal_mean_degrees"]
                if selected
                else baseline["normal_angle"].get("mean_degrees")
            ),
            "material_cell_mae_255": (
                selected["selected_material_cell_mae_255"]
                if selected
                else baseline["material_cells"]["rgb_mae_255"]
            ),
            "ply_v7_depth_relative_mae_pct": source_metrics.get(
                "ply_vs_v7_depth", {}
            ).get("relative_mae_pct"),
            "ply_v7_depth_relative_p95_pct": source_metrics.get(
                "ply_vs_v7_depth", {}
            ).get("relative_p95_pct"),
            "fpt_ply_depth_relative_mae_pct": source_metrics[
                "fpt_vs_ply_depth"
            ].get("relative_mae_pct"),
            "fpt_v7_depth_relative_mae_pct": source_metrics[
                "fpt_vs_v7_depth"
            ].get("relative_mae_pct"),
        }
        row["classification"] = classify(
            row, args.structure_iou, args.continuous_iou
        )
        rows.append(row)

    args.output.mkdir(parents=True, exist_ok=True)
    manifest = {
        "git": {
            "branch": git("branch --show-current"),
            "commit": git("rev-parse HEAD"),
        },
        "inputs": {
            "baseline": str(args.baseline.resolve()),
            "baseline_sha256": sha256(args.baseline),
            "selected": str(args.selected.resolve()),
            "selected_sha256": sha256(args.selected),
        },
        "thresholds": {
            "structure_visible_iou": args.structure_iou,
            "continuous_visible_iou": args.continuous_iou,
        },
        "limitations": [
            "Mandelbulber authored-render versus PLY geometry is not yet measured.",
            "Historical source reports may have null direct PLY/V7 first-hit depth fields until regenerated by the current parity harness.",
        ],
        "rows": rows,
    }
    (args.output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    fields = list(rows[0])
    with (args.output / "summary.csv").open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fields)
        writer.writeheader()
        writer.writerows(rows)
    counts: dict[str, int] = {}
    for row in rows:
        classification = row["classification"]
        counts[classification] = counts.get(classification, 0) + 1
    print(json.dumps({"output": str(args.output), "classifications": counts}))


if __name__ == "__main__":
    main()
