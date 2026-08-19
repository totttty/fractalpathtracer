#!/usr/bin/env python3
"""Derive camera-visible FPTVOX bounds and a memory-bounded resolution policy."""

from __future__ import annotations

import argparse
import csv
import json
import math
import shutil
import subprocess
import sys
from pathlib import Path

import numpy as np


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = (
    ROOT / "reports/fptvox7-ranked50-latest-naadf-300px-32spp-20260815/summary.json"
)
DEFAULT_OUTPUT = ROOT / "reports/fptvox7-view-bounds-gate-20260816"
DEFAULT_BASELINE_PARITY = (
    ROOT / "reports/fptvox7-ranked50-exact-v7-r192-m384-20260816/summary.json"
)
DEFAULT_MANDEL_ROOT = Path("/Volumes/Ventura/Projects/mandelbulber2/mandelbulber2")
DEFAULT_MANDEL_BIN = Path(
    "/Volumes/Ventura/Projects/mandelbulber2/build-opencl/"
    "mandelbulber2.app/Contents/MacOS/mandelbulber2"
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--baseline-parity", type=Path, default=DEFAULT_BASELINE_PARITY)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--ranks", default="9,27,32,38,49,3,13")
    parser.add_argument("--export-ranks", default="")
    parser.add_argument("--render-exported", action="store_true")
    parser.add_argument("--anisotropic", action="store_true")
    parser.add_argument("--image-size", type=int, default=300)
    parser.add_argument("--quantile", type=float, default=0.005)
    parser.add_argument("--margin", type=float, default=0.02)
    parser.add_argument("--min-expansion-ratio", type=float, default=1.05)
    parser.add_argument("--expansion-fraction", type=float, default=1.0)
    parser.add_argument("--min-fpt-iou-gain", type=float, default=0.02)
    parser.add_argument("--min-ply-v7-iou", type=float, default=0.95)
    parser.add_argument("--max-ply-v7-regression", type=float, default=0.005)
    parser.add_argument("--base-resolution", type=int, default=192)
    parser.add_argument("--base-sampling-resolution", type=int, default=384)
    parser.add_argument("--max-output-resolution", type=int, default=192)
    parser.add_argument("--max-sampling-resolution", type=int, default=384)
    parser.add_argument("--max-field-mib", type=float, default=512.0)
    parser.add_argument("--min-free-gib", type=float, default=4.0)
    parser.add_argument("--fpt", type=Path, default=ROOT / "target/release/fpt-metal")
    parser.add_argument("--mandelbulber-root", type=Path, default=DEFAULT_MANDEL_ROOT)
    parser.add_argument("--mandelbulber-bin", type=Path, default=DEFAULT_MANDEL_BIN)
    parser.add_argument("--force", action="store_true")
    args = parser.parse_args()
    if not 0.0 <= args.quantile < 0.5:
        parser.error("--quantile must be in [0, 0.5)")
    if not 0.0 <= args.margin <= 1.0:
        parser.error("--margin must be in [0, 1]")
    if args.max_field_mib <= 0.0:
        parser.error("--max-field-mib must be positive")
    if args.min_expansion_ratio < 1.0:
        parser.error("--min-expansion-ratio must be at least 1")
    if not 0.0 <= args.expansion_fraction <= 1.0:
        parser.error("--expansion-fraction must be in [0, 1]")
    if args.min_fpt_iou_gain < 0.0 or args.max_ply_v7_regression < 0.0:
        parser.error("parity tolerances must be nonnegative")
    return args


def run(command: list[str], stdout: Path, stderr: Path, timeout: int = 1800) -> str:
    result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, timeout=timeout)
    stdout.write_text(result.stdout)
    stderr.write_text(result.stderr)
    if result.returncode:
        raise RuntimeError(result.stderr.strip() or result.stdout.strip())
    return result.stdout


def csv_vec(values: list[float]) -> str:
    return ",".join(f"{value:.9g}" for value in values)


def round_up(value: float, step: int = 16) -> int:
    return int(math.ceil(value / step) * step)


def round_down(value: int, step: int = 16) -> int:
    return max(step, value // step * step)


def max_sampling_for_field_mib(mebibytes: float) -> int:
    # The exact extractor retains topology and color as two f32 lattices.
    bytes_available = mebibytes * 1024.0 * 1024.0
    lattice = int((bytes_available / 8.0) ** (1.0 / 3.0))
    return max(1, lattice - 1)


def aspect_grid(span: np.ndarray, maximum: int) -> list[int]:
    maximum_span = float(span.max())
    return [
        max(2, min(maximum, int(math.floor(value / maximum_span * maximum + 0.5))))
        for value in span
    ]


def field_mib(shape: list[int]) -> float:
    return 2.0 * 4.0 * math.prod(resolution + 1 for resolution in shape) / (1024.0**2)


def anisotropic_sampling_limit(args: argparse.Namespace, span: np.ndarray) -> int:
    for maximum in range(args.max_sampling_resolution, 1, -1):
        if field_mib(aspect_grid(span, maximum)) <= args.max_field_mib:
            return maximum
    return 2


def derive_bounds(
    positions: np.ndarray,
    world_scale: float,
    quantile: float,
    margin: float,
) -> tuple[np.ndarray, np.ndarray]:
    scene_positions = positions / world_scale
    lower = np.quantile(scene_positions, quantile, axis=0)
    upper = np.quantile(scene_positions, 1.0 - quantile, axis=0)
    span = np.maximum(upper - lower, 1.0e-6)
    return lower - span * margin, upper + span * margin


def structural_probe(args: argparse.Namespace, row: dict, scene: Path) -> dict:
    probe = scene / "continuous-probe"
    structural = probe / "structural.bin"
    manifest = Path(f"{structural}.json")
    if args.force or not (structural.is_file() and manifest.is_file()):
        probe.mkdir(parents=True, exist_ok=True)
        run(
            [
                args.fpt.as_posix(), "diagnostic", row["source"],
                "--out", probe.as_posix(),
                "--mode", "diffuse-normal",
                "--structural-dump", structural.as_posix(),
                "--mandelbulber-root", args.mandelbulber_root.as_posix(),
                "--width", str(args.image_size), "--height", str(args.image_size),
            ],
            scene / "diagnostic.stdout.log",
            scene / "diagnostic.stderr.log",
        )
    metadata = json.loads(manifest.read_text())
    values = np.fromfile(structural, dtype="<f4")
    expected = args.image_size * args.image_size * 16
    if values.size != expected:
        raise RuntimeError(f"structural dump has {values.size} floats; expected {expected}")
    records = values.reshape(-1, 16)
    hit = records[:, 7] > 0.5
    if not hit.any():
        raise RuntimeError("continuous diagnostic has no visible surface hits")
    world_scale = float(metadata.get("world_scale", 1000.0))
    lower, upper = derive_bounds(
        records[hit, :3].astype(np.float64),
        world_scale,
        args.quantile,
        args.margin,
    )
    return {
        "hit_count": int(hit.sum()),
        "world_scale": world_scale,
        "bounds_min": lower.tolist(),
        "bounds_max": upper.tolist(),
    }


def resolution_policy(args: argparse.Namespace, row: dict, probe: dict) -> dict:
    old_min = np.asarray(row["reference_bounds_min"], dtype=np.float64)
    old_max = np.asarray(row["reference_bounds_max"], dtype=np.float64)
    derived_min = np.asarray(probe["bounds_min"], dtype=np.float64)
    derived_max = np.asarray(probe["bounds_max"], dtype=np.float64)
    target_min = np.minimum(old_min, derived_min)
    target_max = np.maximum(old_max, derived_max)
    new_min = old_min + (target_min - old_min) * args.expansion_fraction
    new_max = old_max + (target_max - old_max) * args.expansion_fraction
    old_span = old_max - old_min
    new_span = new_max - new_min
    if float(new_span.max() / old_span.max()) < args.min_expansion_ratio:
        new_min = old_min
        new_max = old_max
        new_span = old_span
    old_cell = float(old_span.max() / args.base_resolution)
    required_output = round_up(float(new_span.max() / old_cell))
    if args.anisotropic:
        sampling_limit = anisotropic_sampling_limit(args, new_span)
    else:
        sampling_limit = round_down(
            min(args.max_sampling_resolution, max_sampling_for_field_mib(args.max_field_mib))
        )
    sampling_ratio = args.base_sampling_resolution / args.base_resolution
    quality_output_limit = round_down(int(sampling_limit / sampling_ratio))
    selected_output = max(
        args.base_resolution,
        min(args.max_output_resolution, quality_output_limit, required_output),
    )
    desired_sampling = round_up(
        selected_output * sampling_ratio
    )
    selected_sampling = max(
        args.base_sampling_resolution,
        min(sampling_limit, desired_sampling),
    )
    selected_cell = float(new_span.max() / selected_output)
    output_grid = (
        aspect_grid(new_span, selected_output)
        if args.anisotropic
        else [selected_output] * 3
    )
    sampling_grid = (
        aspect_grid(new_span, selected_sampling)
        if args.anisotropic
        else [selected_sampling] * 3
    )
    return {
        "old_span": old_span.tolist(),
        "derived_span": (derived_max - derived_min).tolist(),
        "target_span": (target_max - target_min).tolist(),
        "expansion_fraction": args.expansion_fraction,
        "new_span": new_span.tolist(),
        "selected_bounds_min": new_min.tolist(),
        "selected_bounds_max": new_max.tolist(),
        "old_world_cell_size": old_cell,
        "selected_world_cell_size": selected_cell,
        "world_cell_size_scale": selected_cell / old_cell,
        "required_output_resolution": required_output,
        "quality_output_limit": quality_output_limit,
        "selected_output_resolution": selected_output,
        "selected_output_grid": output_grid,
        "desired_sampling_resolution": desired_sampling,
        "selected_sampling_resolution": selected_sampling,
        "selected_sampling_grid": sampling_grid,
        "estimated_field_mib": field_mib(sampling_grid),
        "output_resolution_capped": selected_output < required_output,
        "sampling_resolution_capped": selected_sampling < desired_sampling,
    }


def export_scene(args: argparse.Namespace, row: dict, scene: Path, result: dict) -> dict:
    free_gib = shutil.disk_usage(scene).free / 1024.0**3
    if free_gib < args.min_free_gib:
        raise RuntimeError(
            f"only {free_gib:.2f} GiB free; export requires --min-free-gib {args.min_free_gib}"
        )
    bounds_min = csv_vec(result["resolution"]["selected_bounds_min"])
    bounds_max = csv_vec(result["resolution"]["selected_bounds_max"])
    output_resolution = result["resolution"]["selected_output_resolution"]
    sampling_resolution = result["resolution"]["selected_sampling_resolution"]
    direct = scene / "direct-v7.fptvox"
    reference = scene / "mandel-ply-v7.fptvox"
    raw_ply = scene / "mandel.ply"
    common = [
        args.fpt.as_posix(), "voxel-export", row["source"],
        "--voxel-resolution", str(output_resolution),
        "--bounds-min", bounds_min,
        "--bounds-max", bounds_max,
        "--mandelbulber-root", args.mandelbulber_root.as_posix(),
    ]
    anisotropic = ["--surface-triangle-anisotropic"] if args.anisotropic else []
    direct_report = json.loads(run(
        common[:3] + ["--out", direct.as_posix()] + common[3:] + [
            "--surface-triangles",
            "--surface-triangle-resolution", str(sampling_resolution),
        ] + anisotropic,
        scene / "direct.stdout.log",
        scene / "direct.stderr.log",
    ))
    reference_report = json.loads(run(
        common[:3] + ["--out", reference.as_posix()] + common[3:] + [
            "--surface-source", "mandelbulber-mesh",
            "--mandel-mesh-resolution", str(sampling_resolution),
            "--mandelbulber-bin", args.mandelbulber_bin.as_posix(),
            "--mandel-mesh-ply-out", raw_ply.as_posix(),
            "--surface-triangles",
        ] + anisotropic,
        scene / "ply.stdout.log",
        scene / "ply.stderr.log",
    ))
    return {
        "direct": direct_report,
        "reference": reference_report,
        "direct_volume": direct.resolve().as_posix(),
        "reference_volume": reference.resolve().as_posix(),
    }


def render_exports(args: argparse.Namespace, alignment: list[dict]) -> list[dict]:
    alignment_path = args.output / "alignment-summary.json"
    render_output = args.output / "parity"
    alignment_path.write_text(json.dumps(alignment, indent=2) + "\n")
    command = [
        sys.executable,
        (ROOT / "scripts/render_fptvox7_parity_sheets.py").as_posix(),
        "--alignment-summary", alignment_path.as_posix(),
        "--output", render_output.as_posix(),
        "--image-size", str(args.image_size),
    ]
    if args.force:
        command.append("--force")
    run(
        command,
        args.output / "parity.stdout.log",
        args.output / "parity.stderr.log",
    )
    return json.loads((render_output / "summary.json").read_text())


def apply_acceptance(args: argparse.Namespace, rows: list[dict]) -> None:
    if not args.baseline_parity.is_file():
        return
    baseline = {
        int(row["rank"]): row for row in json.loads(args.baseline_parity.read_text())
    }
    for row in rows:
        candidate = row.get("parity")
        previous = baseline.get(int(row["rank"]))
        if candidate is None or previous is None:
            continue
        previous_fpt = float(previous["fpt_vs_v7_mask"]["visible_iou"])
        candidate_fpt = float(candidate["fpt_vs_v7_mask"]["visible_iou"])
        previous_ply = float(previous["geometry"]["visible_iou"])
        candidate_ply = float(candidate["geometry"]["visible_iou"])
        fpt_gain = candidate_fpt - previous_fpt
        ply_delta = candidate_ply - previous_ply
        accepted = (
            fpt_gain >= args.min_fpt_iou_gain
            and candidate_ply >= args.min_ply_v7_iou
            and ply_delta >= -args.max_ply_v7_regression
        )
        row["acceptance"] = {
            "accepted": accepted,
            "fpt_v7_visible_iou_gain": fpt_gain,
            "ply_v7_visible_iou_delta": ply_delta,
            "baseline_fpt_v7_visible_iou": previous_fpt,
            "baseline_ply_v7_visible_iou": previous_ply,
        }


def write_reports(output: Path, rows: list[dict]) -> None:
    (output / "summary.json").write_text(json.dumps(rows, indent=2) + "\n")
    fields = [
        "rank", "name", "hit_count", "old_max_span", "new_max_span",
        "expansion_fraction", "old_world_cell_size", "world_cell_size_scale",
        "required_output_resolution",
        "quality_output_limit", "selected_output_resolution", "selected_output_grid",
        "selected_sampling_resolution", "selected_sampling_grid", "estimated_field_mib",
        "output_resolution_capped",
        "sampling_resolution_capped", "exported", "fpt_v7_visible_iou",
        "ply_v7_visible_iou", "fpt_v7_visible_iou_gain", "ply_v7_visible_iou_delta",
        "accepted", "error",
    ]
    with (output / "summary.csv").open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fields)
        writer.writeheader()
        for row in rows:
            probe = row.get("probe", {})
            policy = row.get("resolution", {})
            writer.writerow({
                "rank": row["rank"],
                "name": row["name"],
                "hit_count": probe.get("hit_count", ""),
                "old_max_span": max(policy.get("old_span", [0.0])),
                "new_max_span": max(policy.get("new_span", [0.0])),
                "old_world_cell_size": policy.get("old_world_cell_size", ""),
                "expansion_fraction": policy.get("expansion_fraction", ""),
                "world_cell_size_scale": policy.get("world_cell_size_scale", ""),
                "required_output_resolution": policy.get("required_output_resolution", ""),
                "quality_output_limit": policy.get("quality_output_limit", ""),
                "selected_output_resolution": policy.get("selected_output_resolution", ""),
                "selected_output_grid": "x".join(
                    str(value) for value in policy.get("selected_output_grid", [])
                ),
                "selected_sampling_resolution": policy.get("selected_sampling_resolution", ""),
                "selected_sampling_grid": "x".join(
                    str(value) for value in policy.get("selected_sampling_grid", [])
                ),
                "estimated_field_mib": policy.get("estimated_field_mib", ""),
                "output_resolution_capped": policy.get("output_resolution_capped", ""),
                "sampling_resolution_capped": policy.get("sampling_resolution_capped", ""),
                "exported": "export" in row,
                "fpt_v7_visible_iou": row.get("parity", {}).get("fpt_vs_v7_mask", {}).get(
                    "visible_iou", ""
                ),
                "ply_v7_visible_iou": row.get("parity", {}).get("geometry", {}).get(
                    "visible_iou", ""
                ),
                "fpt_v7_visible_iou_gain": row.get("acceptance", {}).get(
                    "fpt_v7_visible_iou_gain", ""
                ),
                "ply_v7_visible_iou_delta": row.get("acceptance", {}).get(
                    "ply_v7_visible_iou_delta", ""
                ),
                "accepted": row.get("acceptance", {}).get("accepted", ""),
                "error": row.get("error", ""),
            })


def main() -> None:
    args = parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    ranks = {int(value) for value in args.ranks.split(",") if value}
    export_ranks = {int(value) for value in args.export_ranks.split(",") if value}
    manifest = [row for row in json.loads(args.manifest.read_text()) if int(row["rank"]) in ranks]
    rows = []
    alignment = []
    for row in manifest:
        rank = int(row["rank"])
        scene = args.output / f"{rank:02d}"
        scene.mkdir(parents=True, exist_ok=True)
        result = {"rank": rank, "name": row["name"], "source": row["source"]}
        try:
            result["probe"] = structural_probe(args, row, scene)
            result["resolution"] = resolution_policy(args, row, result["probe"])
            if rank in export_ranks:
                result["export"] = export_scene(args, row, scene, result)
                aligned = dict(row)
                aligned["v7_volume"] = result["export"]["direct_volume"]
                aligned["reference_volume"] = result["export"]["reference_volume"]
                aligned["voxel_resolution"] = result["resolution"][
                    "selected_output_resolution"
                ]
                aligned["triangle_resolution"] = result["resolution"][
                    "selected_sampling_resolution"
                ]
                alignment.append(aligned)
            print(
                f"[{rank:02d}] {row['name']}: span "
                f"{max(result['resolution']['old_span']):.3f} -> "
                f"{max(result['resolution']['new_span']):.3f}, output "
                f"{result['resolution']['selected_output_resolution']}, sampling "
                f"{result['resolution']['selected_sampling_resolution']}"
            )
        except Exception as error:
            result["error"] = str(error)
            print(f"[{rank:02d}] {row['name']}: FAILED: {error}")
        rows.append(result)
        write_reports(args.output, rows)

    if args.render_exported and alignment:
        parity = {int(row["rank"]): row for row in render_exports(args, alignment)}
        for row in rows:
            if row["rank"] in parity:
                row["parity"] = parity[row["rank"]]
        apply_acceptance(args, rows)
        write_reports(args.output, rows)


if __name__ == "__main__":
    main()
