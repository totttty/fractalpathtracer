#!/usr/bin/env python3
"""Gate authored-view connected triangles against bounded isolated-hit splats."""

from __future__ import annotations

import argparse
import csv
import importlib.util
import json
import math
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
DEFAULT_OUTPUT = ROOT / "reports/fptvox7-view-splat-gate-20260818"
DEFAULT_MANDEL_ROOT = Path("/Volumes/Ventura/Projects/mandelbulber2/mandelbulber2")
DEFAULT_NAADF = Path(
    "/Users/jordantotty/Desktop/vox/metal-voxel-naadf-pathtracer/"
    "build/MetalVoxel.app/Contents/MacOS/MetalVoxel"
)


def load_parity_module():
    path = ROOT / "scripts/render_fptvox7_parity_sheets.py"
    spec = importlib.util.spec_from_file_location("fptvox7_parity", path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


PARITY = load_parity_module()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument(
        "--capture-cache-root",
        type=Path,
        help="reuse <root>/<rank>/capture.bin instead of storing captures in --output",
    )
    parser.add_argument("--ranks", help="comma-separated ranks; default is all rows")
    parser.add_argument("--limit", type=int)
    parser.add_argument("--voxel-resolution", type=int, default=192)
    parser.add_argument("--sampling-resolution", type=int, default=300)
    parser.add_argument("--benchmark-frames", type=int, default=60)
    parser.add_argument("--benchmark-warmup", type=int, default=10)
    parser.add_argument(
        "--benchmark-pairs",
        type=int,
        default=0,
        help="alternating timing pairs; 0 runs the structural gate without timing",
    )
    parser.add_argument("--auto-bounds-margin", type=float, default=0.02)
    parser.add_argument(
        "--fixed-bounds",
        action="store_true",
        help="keep manifest bounds instead of applying continuous-view bounds policies",
    )
    parser.add_argument("--splat-scale", type=float, default=0.85)
    parser.add_argument("--splat-cell-cap", type=float, default=0.45)
    parser.add_argument(
        "--external-reference-root",
        type=Path,
        help=(
            "optional ranked output containing <rank>/mandel-ply-v7.fptvox; "
            "uses Mandelbulber CPU/double PLY geometry as the headline mask reference"
        ),
    )
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
    if not 0.25 <= args.splat_scale <= 1.5:
        parser.error("--splat-scale must be 0.25..1.5")
    if not 0.1 <= args.splat_cell_cap <= 0.49:
        parser.error("--splat-cell-cap must be 0.1..0.49")
    return args


def csv_vec(values: list[float]) -> str:
    return ",".join(f"{value:.9g}" for value in values)


def run_json(command: list[str], stdout: Path, stderr: Path) -> tuple[dict, float]:
    started = time.perf_counter()
    result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, timeout=1800)
    elapsed = time.perf_counter() - started
    stdout.write_text(result.stdout)
    stderr.write_text(result.stderr)
    if result.returncode:
        raise RuntimeError(result.stderr.strip() or result.stdout.strip())
    return json.loads(result.stdout), elapsed


def export_variant(
    args: argparse.Namespace,
    row: dict,
    scene: Path,
    label: str,
    splats: bool,
    cache: Path,
) -> tuple[dict, float]:
    report = scene / f"{label}.json"
    volume = scene / f"{label}.fptvox"
    if report.is_file() and volume.is_file() and not args.force:
        return json.loads(report.read_text()), 0.0
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
        "--surface-view-triangles",
        "--surface-view-capture-cache",
        cache.as_posix(),
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
    if splats:
        command.extend(
            [
                "--surface-view-splats",
                "--surface-view-splat-scale",
                str(args.splat_scale),
                "--surface-view-splat-cell-cap",
                str(args.splat_cell_cap),
            ]
        )
    payload, elapsed = run_json(command, scene / f"{label}.stdout.log", scene / f"{label}.stderr.log")
    report.write_text(json.dumps(payload, indent=2) + "\n")
    return payload, elapsed


def reference_hit(path: Path, resolution: int) -> np.ndarray:
    values = np.fromfile(path, dtype="<f4")
    expected = resolution * resolution * 16
    if values.size != expected:
        raise RuntimeError(f"{path} has {values.size} floats; expected {expected}")
    return values.reshape(resolution, resolution, 16)[:, :, 7] > 0.5


def render_volume(
    args: argparse.Namespace,
    row: dict,
    scene: Path,
    label: str,
    volume: Path,
) -> tuple[np.ndarray, dict]:
    render = scene / f"render-{label}"
    render_args = argparse.Namespace(
        naadf=args.naadf,
        image_size=args.sampling_resolution,
        force=args.force,
    )
    base = PARITY.render_visibility(render_args, row, volume, "exact", render)
    forward = PARITY.camera_basis(row)[0].astype(np.float32)
    images = PARITY.visibility_images(
        base,
        PARITY.volume_arrays(volume),
        forward,
        row["fov"],
        float(row.get("roll", 0.0)),
        bool(row.get("legacy_coordinate_system", False)),
    )
    return images["hit"], images


def render_variant(
    args: argparse.Namespace,
    row: dict,
    scene: Path,
    label: str,
) -> tuple[np.ndarray, dict]:
    return render_volume(args, row, scene, label, scene / f"{label}.fptvox")


def benchmark_variant(
    args: argparse.Namespace, row: dict, scene: Path, label: str, pair: int
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
        grid_camera = [
            (row["world_camera"][axis] - bounds_min[axis])
            / (bounds_max[axis] - bounds_min[axis])
            * header["resolution"][axis]
            for axis in range(3)
        ]
        camera = grid_camera + [row["yaw"], row["pitch"]]
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
        (scene / f"benchmark-{label}.stdout.log").write_text(result.stdout)
        (scene / f"benchmark-{label}.stderr.log").write_text(result.stderr)
        if result.returncode:
            raise RuntimeError(result.stderr.strip() or result.stdout.strip())
    with csv_path.open(newline="") as handle:
        timing = next(csv.DictReader(handle))
    gpu_ms = float(timing["gpu_ms"])
    return gpu_ms if math.isfinite(gpu_ms) and 0.0 < gpu_ms < 60_000.0 else None


def benchmark_variants(args: argparse.Namespace, row: dict, scene: Path) -> dict:
    samples = {"connected": [], "splats": []}
    rejected = {"connected": 0, "splats": 0}
    paired_deltas = []
    for pair in range(args.benchmark_pairs):
        order = ("connected", "splats") if pair % 2 == 0 else ("splats", "connected")
        pair_samples = {}
        for label in order:
            value = benchmark_variant(args, row, scene, label, pair)
            if value is None:
                rejected[label] += 1
            else:
                samples[label].append(value)
                pair_samples[label] = value
        if pair_samples.keys() >= {"connected", "splats"}:
            paired_deltas.append(
                (pair_samples["splats"] / pair_samples["connected"] - 1.0) * 100.0
            )
    if any(not values for values in samples.values()):
        raise RuntimeError("benchmark produced no valid GPU timestamps for one variant")
    variants = {
        label: {
            "median_gpu_ms": statistics.median(values),
            "stdev_gpu_ms": statistics.stdev(values) if len(values) > 1 else 0.0,
            "samples_gpu_ms": values,
            "rejected_gpu_samples": rejected[label],
        }
        for label, values in samples.items()
    }
    variants["paired_delta_pct_samples"] = paired_deltas
    variants["paired_delta_pct_median"] = (
        statistics.median(paired_deltas) if paired_deltas else None
    )
    return variants


def save_mask(mask: np.ndarray, path: Path) -> None:
    Image.fromarray(np.where(mask[:, :, None], 255, 0).astype(np.uint8).repeat(3, axis=2)).save(path)


def make_sheets(
    rows: list[dict],
    output: Path,
    external_reference: bool,
    page_size: int = 10,
) -> list[str]:
    paths = []
    tile = 300
    footer = 42
    header = 46
    columns = (
        (
            ("Mandel CPU/double PLY", "reference_mask"),
            ("continuous source capture", "capture_mask"),
            ("connected triangles", "connected_mask"),
            ("bounded splats", "splat_mask"),
        )
        if external_reference
        else (
            ("continuous hit mask", "reference_mask"),
            ("connected triangles", "connected_mask"),
            ("bounded splats", "splat_mask"),
        )
    )
    for page, start in enumerate(range(0, len(rows), page_size), start=1):
        page_rows = rows[start : start + page_size]
        canvas = Image.new(
            "RGB",
            (tile * len(columns), header + len(page_rows) * (tile + footer)),
            (28, 30, 33),
        )
        draw = ImageDraw.Draw(canvas)
        for column, (title, _) in enumerate(columns):
            draw.text((column * tile + 10, 15), title, fill="white")
        y = header
        for row in page_rows:
            for column, (_, key) in enumerate(columns):
                image = Image.open(row[key]).convert("RGB").resize((tile, tile))
                canvas.paste(image, (column * tile, y))
            connected = row["connected"]
            splat = row["splats"]
            footer_text = (
                f"{row['rank']:02d} {row['name']} | IoU {connected['visible_iou']:.3f} -> "
                f"{splat['visible_iou']:.3f} | miss {connected['visible_miss_pct']:.1f}% -> "
                f"{splat['visible_miss_pct']:.1f}% | extra {splat['visible_extra_pct']:.2f}%"
                + (
                    f" | source IoU {row['capture_splats']['visible_iou']:.3f}"
                    if external_reference
                    else ""
                )
            )
            draw.text((10, y + tile + 8), footer_text, fill="white")
            y += tile + footer
        path = output / f"contact-sheet-{page:02d}.png"
        canvas.save(path)
        paths.append(path.as_posix())
    return paths


def main() -> int:
    args = parse_args()
    rows = json.loads(args.manifest.read_text())
    selected = None if not args.ranks else {int(value) for value in args.ranks.split(",")}
    rows = [row for row in rows if selected is None or int(row["rank"]) in selected]
    if args.limit is not None:
        rows = rows[: args.limit]
    args.output.mkdir(parents=True, exist_ok=True)
    summary = []
    for row in rows:
        rank = int(row["rank"])
        scene = args.output / f"{rank:02d}"
        scene.mkdir(parents=True, exist_ok=True)
        cache = (
            args.capture_cache_root / f"{rank:02d}" / "capture.bin"
            if args.capture_cache_root
            else scene / "capture.bin"
        )
        try:
            connected_export, connected_seconds = export_variant(
                args, row, scene, "connected", False, cache
            )
            splat_export, splat_seconds = export_variant(
                args, row, scene, "splats", True, cache
            )
            capture_resolution = int(
                splat_export.get("surface", {}).get(
                    "maximum_capture_resolution", args.sampling_resolution
                )
            )
            scene_args = argparse.Namespace(**vars(args))
            scene_args.sampling_resolution = capture_resolution
            capture_reference = reference_hit(cache, capture_resolution)
            external_volume = (
                args.external_reference_root / f"{rank:02d}" / "mandel-ply-v7.fptvox"
                if args.external_reference_root
                else None
            )
            if external_volume is not None and not external_volume.is_file():
                raise RuntimeError(f"missing external reference volume: {external_volume}")
            reference = (
                render_volume(
                    scene_args, row, scene, "external-reference", external_volume
                )[0]
                if external_volume is not None
                else capture_reference
            )
            connected_hit, _ = render_variant(scene_args, row, scene, "connected")
            splat_hit, _ = render_variant(scene_args, row, scene, "splats")
            timing = (
                benchmark_variants(args, row, scene) if args.benchmark_pairs > 0 else None
            )
            connected_gpu_ms = (
                timing["connected"]["median_gpu_ms"] if timing is not None else None
            )
            splat_gpu_ms = timing["splats"]["median_gpu_ms"] if timing is not None else None
            reference_path = scene / "reference-mask.png"
            capture_path = scene / "capture-mask.png"
            connected_path = scene / "connected-mask.png"
            splat_path = scene / "splat-mask.png"
            save_mask(reference, reference_path)
            save_mask(capture_reference, capture_path)
            save_mask(connected_hit, connected_path)
            save_mask(splat_hit, splat_path)
            connected_metrics = PARITY.mask_metrics(reference, connected_hit)
            splat_metrics = PARITY.mask_metrics(reference, splat_hit)
            capture_connected_metrics = PARITY.mask_metrics(capture_reference, connected_hit)
            capture_splat_metrics = PARITY.mask_metrics(capture_reference, splat_hit)
            result = {
                "rank": rank,
                "name": row["name"],
                "source": row["source"],
                "status": "ok",
                "requested_sampling_resolution": args.sampling_resolution,
                "effective_sampling_resolution": capture_resolution,
                "reference_hit_pixels": int(reference.sum()),
                "reference_backend": (
                    "mandelbulber-cpu-double-ply"
                    if external_volume is not None
                    else "continuous-source-capture"
                ),
                "connected": connected_metrics,
                "splats": splat_metrics,
                "capture_connected": capture_connected_metrics,
                "capture_splats": capture_splat_metrics,
                "iou_delta": splat_metrics["visible_iou"] - connected_metrics["visible_iou"],
                "extra_delta_pct": splat_metrics["visible_extra_pct"] - connected_metrics["visible_extra_pct"],
                "connected_gpu_ms": connected_gpu_ms,
                "splat_gpu_ms": splat_gpu_ms,
                "gpu_delta_pct": (
                    timing["paired_delta_pct_median"]
                    if timing is not None
                    else None
                ),
                "timing": timing,
                "connected_export_seconds": connected_seconds,
                "splat_export_seconds": splat_seconds,
                "connected_bytes": int(connected_export["summary"]["bytes"]),
                "splat_bytes": int(splat_export["summary"]["bytes"]),
                "connected_cells": int(connected_export["summary"]["voxel_count"]),
                "splat_cells": int(splat_export["summary"]["voxel_count"]),
                "connected_surface": connected_export["surface"],
                "splat_surface": splat_export["surface"],
                "reference_mask": reference_path.as_posix(),
                "capture_mask": capture_path.as_posix(),
                "connected_mask": connected_path.as_posix(),
                "splat_mask": splat_path.as_posix(),
            }
        except Exception as error:  # Continue so one unsupported scene does not hide the cohort.
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
                    for key in ("rank", "name", "status", "iou_delta", "gpu_delta_pct", "error")
                    if key in result
                }
            )
        )

    successful = [row for row in summary if row["status"] == "ok"]
    contact_sheets = make_sheets(
        successful, args.output, args.external_reference_root is not None
    )
    aggregate = {
        "reference_backend": (
            "mandelbulber-cpu-double-ply"
            if args.external_reference_root is not None
            else "continuous-source-capture"
        ),
        "bounds_mode": "fixed-manifest" if args.fixed_bounds else "auto-visible",
        "scenes_requested": len(rows),
        "scenes_succeeded": len(successful),
        "scenes_failed": len(rows) - len(successful),
        "median_connected_iou": statistics.median(row["connected"]["visible_iou"] for row in successful) if successful else None,
        "median_splat_iou": statistics.median(row["splats"]["visible_iou"] for row in successful) if successful else None,
        "median_iou_delta": statistics.median(row["iou_delta"] for row in successful) if successful else None,
        "median_extra_pct": statistics.median(row["splats"]["visible_extra_pct"] for row in successful) if successful else None,
        "median_capture_splat_iou": statistics.median(row["capture_splats"]["visible_iou"] for row in successful) if successful else None,
        "median_gpu_delta_pct": (
            statistics.median(
                row["gpu_delta_pct"]
                for row in successful
                if row["gpu_delta_pct"] is not None
            )
            if any(row["gpu_delta_pct"] is not None for row in successful)
            else None
        ),
        "iou_regressions": [row["rank"] for row in successful if row["iou_delta"] < -0.001],
        "extra_over_two_pct": [row["rank"] for row in successful if row["splats"]["visible_extra_pct"] > 2.0],
        "contact_sheets": contact_sheets,
    }
    (args.output / "summary.json").write_text(json.dumps({"aggregate": aggregate, "rows": summary}, indent=2) + "\n")
    if successful:
        fields = [
            "rank", "name", "reference_hit_pixels", "iou_delta", "extra_delta_pct",
            "connected_gpu_ms", "splat_gpu_ms", "gpu_delta_pct", "connected_bytes",
            "splat_bytes", "connected_cells", "splat_cells",
        ]
        with (args.output / "summary.csv").open("w", newline="") as handle:
            writer = csv.DictWriter(handle, fieldnames=fields)
            writer.writeheader()
            writer.writerows({key: row[key] for key in fields} for row in successful)
    print(json.dumps(aggregate, indent=2))
    return 0 if len(successful) == len(rows) else 1


if __name__ == "__main__":
    raise SystemExit(main())
