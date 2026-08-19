#!/usr/bin/env python3
"""Gate indexed authored-view triangles against the accepted V7 layout."""

from __future__ import annotations

import argparse
import csv
import importlib.util
import json
import math
import re
import statistics
import subprocess
import time
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = (
    ROOT / "reports/fptvox7-ranked50-latest-naadf-300px-32spp-20260815/summary.json"
)
DEFAULT_OUTPUT = ROOT / "reports/fptvox8-indexed-ranked50-gate-20260819"
DEFAULT_MANDEL_ROOT = Path("/Volumes/Ventura/Projects/mandelbulber2/mandelbulber2")
DEFAULT_NAADF = Path(
    "/Users/jordantotty/Desktop/vox/metal-voxel-naadf-pathtracer/"
    "build/MetalVoxel.app/Contents/MacOS/MetalVoxel"
)
FANOUT_ERROR = re.compile(r"requires (\d+) references in one cell; limit is (\d+)")


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


PARITY = load_module("fptvox7_parity", ROOT / "scripts/render_fptvox7_parity_sheets.py")
GATE = load_module("fptvox7_gate", ROOT / "scripts/run_fptvox7_view_splat_gate.py")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--ranks", help="comma-separated ranks; default is the complete manifest")
    parser.add_argument("--limit", type=int)
    parser.add_argument("--voxel-resolution", type=int, default=192)
    parser.add_argument("--sampling-resolution", type=int, default=300)
    parser.add_argument("--benchmark-frames", type=int, default=60)
    parser.add_argument("--benchmark-warmup", type=int, default=10)
    parser.add_argument("--benchmark-pairs", type=int, default=0)
    parser.add_argument("--auto-bounds-margin", type=float, default=0.02)
    parser.add_argument("--fixed-bounds", action="store_true")
    parser.add_argument("--fpt", type=Path, default=ROOT / "target/release/fpt-metal")
    parser.add_argument("--naadf", type=Path, default=DEFAULT_NAADF)
    parser.add_argument("--mandelbulber-root", type=Path, default=DEFAULT_MANDEL_ROOT)
    parser.add_argument("--force", action="store_true")
    args = parser.parse_args()
    if args.limit is not None and args.limit <= 0:
        parser.error("--limit must be positive")
    if args.benchmark_frames <= 0 or args.benchmark_warmup < 0 or args.benchmark_pairs < 0:
        parser.error("benchmark frames must be positive; warmup and pairs must be nonnegative")
    if not 0.001 <= args.auto_bounds_margin <= 1.0:
        parser.error("--auto-bounds-margin must be 0.001..1.0")
    return args


def csv_vec(values: list[float]) -> str:
    return ",".join(f"{value:.9g}" for value in values)


def run_export(
    args: argparse.Namespace,
    row: dict,
    scene: Path,
    label: str,
    indexed: bool,
) -> tuple[dict | None, float, dict | None]:
    report = scene / f"{label}.json"
    volume = scene / f"{label}.fptvox"
    error_report = scene / f"{label}-error.json"
    if report.is_file() and volume.is_file() and not args.force:
        return json.loads(report.read_text()), 0.0, None
    if error_report.is_file() and not args.force:
        return None, 0.0, json.loads(error_report.read_text())

    command = [
        args.fpt.as_posix(),
        "voxel-export",
        row["source"],
        "--out",
        volume.as_posix(),
        "--voxel-resolution",
        str(args.voxel_resolution),
        "--bounds-min",
        csv_vec(row["reference_bounds_min"]),
        "--bounds-max",
        csv_vec(row["reference_bounds_max"]),
        "--surface-view-indexed-triangles" if indexed else "--surface-view-triangles",
        "--surface-view-splats",
        "--surface-view-capture-cache",
        (scene / "capture.bin").as_posix(),
        "--surface-triangle-resolution",
        str(args.sampling_resolution),
        "--mandelbulber-root",
        args.mandelbulber_root.as_posix(),
    ]
    if not args.fixed_bounds:
        command.extend(
            [
                "--surface-triangle-auto-bounds",
                "--surface-view-auto-fit-bounds",
                "--surface-triangle-auto-bounds-margin",
                str(args.auto_bounds_margin),
            ]
        )
    started = time.perf_counter()
    result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, timeout=1800)
    elapsed = time.perf_counter() - started
    (scene / f"{label}.stdout.log").write_text(result.stdout)
    (scene / f"{label}.stderr.log").write_text(result.stderr)
    if result.returncode:
        match = FANOUT_ERROR.search(result.stderr)
        error = {
            "kind": "fanout" if match else "export",
            "message": result.stderr.strip() or result.stdout.strip(),
            "maximum_cell_triangle_references": int(match.group(1)) if match else None,
            "maximum_supported_cell_triangle_references": int(match.group(2)) if match else None,
        }
        error_report.write_text(json.dumps(error, indent=2) + "\n")
        return None, elapsed, error
    payload = json.loads(result.stdout)
    report.write_text(json.dumps(payload, indent=2) + "\n")
    error_report.unlink(missing_ok=True)
    return payload, elapsed, None


def render_mask(
    args: argparse.Namespace,
    row: dict,
    scene: Path,
    label: str,
    volume: Path,
    image_size: int,
) -> np.ndarray:
    render_args = argparse.Namespace(naadf=args.naadf, image_size=image_size, force=args.force)
    base = PARITY.render_visibility(render_args, row, volume, "exact", scene / f"render-{label}")
    forward = PARITY.camera_basis(row)[0].astype(np.float32)
    images = PARITY.visibility_images(
        base,
        PARITY.volume_arrays(volume),
        forward,
        row["fov"],
        float(row.get("roll", 0.0)),
        bool(row.get("legacy_coordinate_system", False)),
    )
    return images["hit"]


def benchmark_once(
    args: argparse.Namespace,
    row: dict,
    scene: Path,
    label: str,
    pair: int,
) -> float | None:
    volume = scene / f"{label}.fptvox"
    report = scene / (
        f"benchmark-{label}-f{args.benchmark_frames}-w{args.benchmark_warmup}-pair-{pair:02d}"
    )
    csv_path = Path(f"{report}.csv")
    if args.force or not csv_path.is_file():
        header = PARITY.volume_header(volume)
        bounds_min = header["bounds_min"]
        bounds_max = header["bounds_max"]
        camera = [
            (row["world_camera"][axis] - bounds_min[axis])
            / (bounds_max[axis] - bounds_min[axis])
            * header["resolution"][axis]
            for axis in range(3)
        ] + [row["yaw"], row["pitch"]]
        command = [
            args.naadf.as_posix(),
            "--mode", "greedy2d",
            "--fptvox", volume.as_posix(),
            "--size", f"{args.sampling_resolution}x{args.sampling_resolution}",
            "--camera", csv_vec(camera),
            "--camera-fov", str(row["fov"]),
            "--camera-roll", str(row.get("roll", 0.0)),
            "--gpu-renderer", "naadf",
            "--gpu-naadf-mode", "aadf",
            "--gpu-naadf-build", "cpu",
            "--gpu-naadf-primary-layout", "direct16",
            "--gpu-naadf-path-tracing", "fixed",
            "--gpu-naadf-path-samples-per-frame", "1",
            "--gpu-naadf-path-max-samples",
            str(args.benchmark_frames + args.benchmark_warmup + 8),
            "--gpu-naadf-path-bounces", "1",
            "--gpu-naadf-fptvox-surface-mode", "exact",
            "--benchmark", "--offscreen",
            "--frames", str(args.benchmark_frames),
            "--warmup", str(args.benchmark_warmup),
            "--report", report.as_posix(),
        ]
        result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, timeout=1800)
        (scene / f"benchmark-{label}-pair-{pair:02d}.stdout.log").write_text(result.stdout)
        (scene / f"benchmark-{label}-pair-{pair:02d}.stderr.log").write_text(result.stderr)
        if result.returncode:
            raise RuntimeError(result.stderr.strip() or result.stdout.strip())
    with csv_path.open(newline="") as handle:
        value = float(next(csv.DictReader(handle))["gpu_ms"])
    return value if math.isfinite(value) and 0.0 < value < 60_000.0 else None


def benchmark_pair(args: argparse.Namespace, row: dict, scene: Path) -> dict:
    samples = {"v7": [], "v8": []}
    paired = []
    for pair in range(args.benchmark_pairs):
        order = ("v7", "v8") if pair % 2 == 0 else ("v8", "v7")
        pair_values = {}
        for label in order:
            value = benchmark_once(args, row, scene, label, pair)
            if value is not None:
                samples[label].append(value)
                pair_values[label] = value
        if pair_values.keys() >= {"v7", "v8"}:
            paired.append((pair_values["v8"] / pair_values["v7"] - 1.0) * 100.0)
    if not samples["v7"] or not samples["v8"]:
        raise RuntimeError("benchmark produced no valid GPU timestamp for one layout")
    return {
        "v7_samples_gpu_ms": samples["v7"],
        "v8_samples_gpu_ms": samples["v8"],
        "v7_median_gpu_ms": statistics.median(samples["v7"]),
        "v8_median_gpu_ms": statistics.median(samples["v8"]),
        "paired_delta_pct_samples": paired,
        "paired_delta_pct_median": statistics.median(paired) if paired else None,
    }


def save_mask(mask: np.ndarray, path: Path) -> None:
    Image.fromarray(np.where(mask[:, :, None], 255, 0).astype(np.uint8).repeat(3, axis=2)).save(path)


def make_sheets(rows: list[dict], output: Path, page_size: int = 10) -> list[str]:
    paths = []
    tile, header, footer = 300, 45, 50
    columns = ("continuous capture", "V7 clipped", "V8 indexed", "V7 xor V8")
    for page, start in enumerate(range(0, len(rows), page_size), start=1):
        page_rows = rows[start : start + page_size]
        canvas = Image.new("RGB", (tile * 4, header + len(page_rows) * (tile + footer)), (28, 30, 33))
        draw = ImageDraw.Draw(canvas)
        for column, title in enumerate(columns):
            draw.text((column * tile + 10, 15), title, fill="white")
        y = header
        for row in page_rows:
            keys = ("capture_mask", "v7_mask", "v8_mask", "diff_mask")
            for column, key in enumerate(keys):
                if row.get(key):
                    image = Image.open(row[key]).convert("RGB").resize((tile, tile))
                else:
                    image = Image.new("RGB", (tile, tile), (50, 22, 22))
                    ImageDraw.Draw(image).text((12, 12), "V8 ineligible", fill="white")
                canvas.paste(image, (column * tile, y))
            if row["status"] == "ok":
                footer_text = (
                    f"{row['rank']:02d} {row['name']} | capture IoU V7 {row['capture_v7']['visible_iou']:.3f} "
                    f"V8 {row['capture_v8']['visible_iou']:.3f} | V7/V8 {row['v7_v8']['visible_iou']:.3f} "
                    f"| refs {row['cell_triangle_references']:,} max {row['maximum_cell_triangle_references']}"
                )
            else:
                footer_text = (
                    f"{row['rank']:02d} {row['name']} | V8 {row['status']} | "
                    f"max refs {row.get('maximum_cell_triangle_references', 'unknown')}"
                )
            draw.text((10, y + tile + 8), footer_text, fill="white")
            y += tile + footer
        path = output / f"contact-sheet-{page:02d}.png"
        canvas.save(path)
        paths.append(path.as_posix())
    return paths


def load_manifest(path: Path) -> list[dict]:
    payload = json.loads(path.read_text())
    return payload if isinstance(payload, list) else payload["rows"]


def main() -> int:
    args = parse_args()
    selected = None if not args.ranks else {int(value) for value in args.ranks.split(",")}
    rows = [row for row in load_manifest(args.manifest) if selected is None or row["rank"] in selected]
    if args.limit is not None:
        rows = rows[: args.limit]
    args.output.mkdir(parents=True, exist_ok=True)
    summary = []
    for row in rows:
        rank = int(row["rank"])
        scene = args.output / f"{rank:02d}"
        scene.mkdir(parents=True, exist_ok=True)
        try:
            v7_export, v7_seconds, v7_error = run_export(args, row, scene, "v7", False)
            if v7_error is not None or v7_export is None:
                raise RuntimeError(v7_error["message"] if v7_error else "V7 export failed")
            v8_export, v8_seconds, v8_error = run_export(args, row, scene, "v8", True)
            capture_resolution = int(
                v7_export.get("surface", {}).get("maximum_capture_resolution", args.sampling_resolution)
            )
            capture = GATE.reference_hit(scene / "capture.bin", capture_resolution)
            v7_hit = render_mask(args, row, scene, "v7", scene / "v7.fptvox", capture_resolution)
            capture_path = scene / "capture-mask.png"
            v7_path = scene / "v7-mask.png"
            save_mask(capture, capture_path)
            save_mask(v7_hit, v7_path)
            if v8_error is not None or v8_export is None:
                result = {
                    "rank": rank,
                    "name": row["name"],
                    "source": row["source"],
                    "status": "ineligible" if v8_error and v8_error["kind"] == "fanout" else "error",
                    "error": v8_error["message"] if v8_error else "V8 export failed",
                    "maximum_cell_triangle_references": (
                        v8_error.get("maximum_cell_triangle_references") if v8_error else None
                    ),
                    "maximum_supported_cell_triangle_references": (
                        v8_error.get("maximum_supported_cell_triangle_references") if v8_error else None
                    ),
                    "capture_v7": PARITY.mask_metrics(capture, v7_hit),
                    "v7_export_seconds": v7_seconds,
                    "v8_export_seconds": v8_seconds,
                    "v7_bytes": int(v7_export["summary"]["bytes"]),
                    "capture_mask": capture_path.as_posix(),
                    "v7_mask": v7_path.as_posix(),
                    "v8_mask": None,
                    "diff_mask": None,
                }
            else:
                v8_hit = render_mask(args, row, scene, "v8", scene / "v8.fptvox", capture_resolution)
                diff = np.logical_xor(v7_hit, v8_hit)
                v8_path = scene / "v8-mask.png"
                diff_path = scene / "v7-v8-diff-mask.png"
                save_mask(v8_hit, v8_path)
                save_mask(diff, diff_path)
                timing = benchmark_pair(args, row, scene) if args.benchmark_pairs else None
                result = {
                    "rank": rank,
                    "name": row["name"],
                    "source": row["source"],
                    "status": "ok",
                    "capture_v7": PARITY.mask_metrics(capture, v7_hit),
                    "capture_v8": PARITY.mask_metrics(capture, v8_hit),
                    "v7_v8": PARITY.mask_metrics(v7_hit, v8_hit),
                    "source_triangles": int(v8_export["source_triangles"]),
                    "cell_triangle_references": int(v8_export["cell_triangle_references"]),
                    "maximum_cell_triangle_references": int(
                        v8_export["maximum_cell_triangle_references"]
                    ),
                    "p99_cell_triangle_references": int(v8_export["p99_cell_triangle_references"]),
                    "v7_clipped_triangles": int(v8_export["v7_clipped_triangles"]),
                    "intersection_reduction_pct": float(v8_export["intersection_reduction_pct"]),
                    "v7_export_seconds": v7_seconds,
                    "v8_export_seconds": v8_seconds,
                    "v7_bytes": int(v7_export["summary"]["bytes"]),
                    "v8_bytes": int(v8_export["summary"]["bytes"]),
                    "v7_cells": int(v7_export["summary"]["voxel_count"]),
                    "v8_cells": int(v8_export["summary"]["voxel_count"]),
                    "timing": timing,
                    "capture_mask": capture_path.as_posix(),
                    "v7_mask": v7_path.as_posix(),
                    "v8_mask": v8_path.as_posix(),
                    "diff_mask": diff_path.as_posix(),
                }
        except Exception as error:
            result = {
                "rank": rank,
                "name": row["name"],
                "source": row["source"],
                "status": "error",
                "error": str(error),
            }
        summary.append(result)
        print(
            json.dumps(
                {
                    key: result[key]
                    for key in (
                        "rank", "name", "status", "maximum_cell_triangle_references",
                        "intersection_reduction_pct", "error",
                    )
                    if key in result
                }
            ),
            flush=True,
        )
        (args.output / "summary.partial.json").write_text(json.dumps({"rows": summary}, indent=2) + "\n")

    eligible = [row for row in summary if row["status"] == "ok"]
    ineligible = [row for row in summary if row["status"] == "ineligible"]
    failed = [row for row in summary if row["status"] == "error"]
    sheets = make_sheets(summary, args.output)
    aggregate = {
        "scenes_requested": len(rows),
        "scenes_eligible": len(eligible),
        "scenes_ineligible_fanout": len(ineligible),
        "scenes_failed": len(failed),
        "eligibility_pct": 100.0 * len(eligible) / len(rows) if rows else 0.0,
        "median_capture_v7_iou": (
            statistics.median(row["capture_v7"]["visible_iou"] for row in summary if "capture_v7" in row)
            if any("capture_v7" in row for row in summary) else None
        ),
        "median_capture_v8_iou": (
            statistics.median(row["capture_v8"]["visible_iou"] for row in eligible)
            if eligible else None
        ),
        "median_v7_v8_iou": (
            statistics.median(row["v7_v8"]["visible_iou"] for row in eligible)
            if eligible else None
        ),
        "median_intersection_reduction_pct": (
            statistics.median(row["intersection_reduction_pct"] for row in eligible)
            if eligible else None
        ),
        "median_payload_delta_pct": (
            statistics.median((row["v8_bytes"] / row["v7_bytes"] - 1.0) * 100.0 for row in eligible)
            if eligible else None
        ),
        "eligible_ranks": [row["rank"] for row in eligible],
        "ineligible_ranks": [row["rank"] for row in ineligible],
        "failed_ranks": [row["rank"] for row in failed],
        "contact_sheets": sheets,
    }
    (args.output / "summary.json").write_text(
        json.dumps({"aggregate": aggregate, "rows": summary}, indent=2) + "\n"
    )
    print(json.dumps(aggregate, indent=2))
    return 0 if not failed else 1


if __name__ == "__main__":
    raise SystemExit(main())
