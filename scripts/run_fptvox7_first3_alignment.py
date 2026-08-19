#!/usr/bin/env python3
"""Render the first FPT scenes with one camera/bounds contract across all paths."""

from __future__ import annotations

import argparse
import json
import re
import shutil
import struct
import subprocess
import time
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont


ROOT = Path(__file__).resolve().parents[1]
HISTORICAL = ROOT / "reports/fpt-mandel-authoritative-50-r192-m384-20260812"
HISTORICAL_MANIFEST = HISTORICAL / "naadf-view-summary.json"
DEFAULT_NAADF = Path(
    "/Users/jordantotty/Desktop/vox/metal-voxel-naadf-pathtracer/"
    "build/MetalVoxel.app/Contents/MacOS/MetalVoxel"
)
DEFAULT_MANDEL_ROOT = Path("/Volumes/Ventura/Projects/mandelbulber2/mandelbulber2")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--output",
        type=Path,
        default=ROOT / "reports/fptvox7-first3-aligned",
    )
    parser.add_argument("--ranks", default="1,2,3")
    parser.add_argument("--voxel-resolution", type=int, default=96)
    parser.add_argument("--triangle-resolution", type=int, default=192)
    parser.add_argument("--bounds-mode", choices=("archived", "global"), default="archived")
    parser.add_argument("--bounds-min", default="-4,-4,-4")
    parser.add_argument("--bounds-max", default="4,4,4")
    parser.add_argument("--image-size", type=int, default=512)
    parser.add_argument("--sheet-tile-size", type=int)
    parser.add_argument("--sheet-page-rows", type=int, default=10)
    parser.add_argument("--samples", type=int, default=64)
    parser.add_argument("--bounces", type=int, default=4)
    parser.add_argument("--fpt", type=Path, default=ROOT / "target/release/fpt-metal")
    parser.add_argument("--naadf", type=Path, default=DEFAULT_NAADF)
    parser.add_argument("--mandelbulber-root", type=Path, default=DEFAULT_MANDEL_ROOT)
    parser.add_argument("--reference-volume-root", type=Path)
    parser.add_argument("--v7-volume-root", type=Path)
    parser.add_argument(
        "--scene-config",
        type=Path,
        help=(
            "optional JSON map keyed by rank; each entry may override bounds_min, "
            "bounds_max, voxel_resolution, triangle_resolution, threshold_scale, "
            "reference_volume, and v7_volume"
        ),
    )
    parser.add_argument("--force-export", action="store_true")
    parser.add_argument("--force-render", action="store_true")
    parser.add_argument("--reuse-completed", action="store_true")
    parser.add_argument("--compact-output", action="store_true")
    return parser.parse_args()


def run(command: list[str], cwd: Path, stdout: Path, stderr: Path) -> float:
    started = time.perf_counter()
    completed = subprocess.run(
        command,
        cwd=cwd,
        text=True,
        capture_output=True,
        timeout=1800,
    )
    stdout.write_text(completed.stdout)
    stderr.write_text(completed.stderr)
    if completed.returncode:
        raise RuntimeError(completed.stderr.strip() or completed.stdout.strip())
    return time.perf_counter() - started


def first_scene_directory(rank: int) -> Path:
    matches = sorted((HISTORICAL / "scenes").glob(f"{rank:02d}-*"))
    if len(matches) != 1:
        raise RuntimeError(f"rank {rank}: expected exactly one historical scene directory")
    return matches[0]


def source_for(scene_directory: Path) -> Path:
    command_path = scene_directory / "command.json"
    command = json.loads(command_path.read_text())
    return Path(command[2])


def uses_legacy_image_coordinates(source: Path) -> bool:
    match = re.search(
        r"(?m)^\s*legacy_coordinate_system\s+(true|false)\s*;",
        source.read_text(errors="replace"),
    )
    return match is not None and match.group(1) == "true"


def normalized_image(source: Path, output: Path, flip_y: bool) -> Path:
    image = Image.open(source).convert("RGB")
    if flip_y:
        image = image.transpose(Image.Transpose.FLIP_TOP_BOTTOM)
    image.save(output)
    return output


def csv_vector(values: list[float]) -> str:
    return ",".join(f"{value:.9g}" for value in values)


def parse_vector(value: str) -> list[float]:
    result = [float(component) for component in value.split(",")]
    if len(result) != 3:
        raise ValueError(f"expected a three-component vector: {value}")
    return result


def grid_camera(
    camera: dict,
    resolution: int,
    bounds_min_values: list[float] | None = None,
    bounds_max_values: list[float] | None = None,
) -> list[float]:
    bounds_min = np.asarray(bounds_min_values or camera["bounds_min"], dtype=np.float64)
    bounds_max = np.asarray(bounds_max_values or camera["bounds_max"], dtype=np.float64)
    position = np.asarray(camera["world_position"], dtype=np.float64)
    grid = (position - bounds_min) / (bounds_max - bounds_min) * resolution
    return [float(value) for value in grid]


def volume_header(path: Path) -> dict:
    data = path.read_bytes()
    if not (len(data) >= 16 and data[:6] == b"FPTVOX" and data[7] == 0):
        raise RuntimeError(f"unsupported volume magic: {path}")
    header_size, version = struct.unpack_from("<II", data, 8)
    return {
        "data": data,
        "header_size": header_size,
        "version": version,
        "resolution": struct.unpack_from("<III", data, 16),
        "bounds_min": struct.unpack_from("<fff", data, 32),
        "bounds_max": struct.unpack_from("<fff", data, 44),
    }


def occupied_cells(path: Path) -> tuple[dict, set[tuple[int, int, int]]]:
    header = volume_header(path)
    data = header["data"]
    header_size = header["header_size"]
    version = header["version"]
    record_sizes = {1: 24, 2: 28, 3: 28, 5: 32, 6: 36, 7: 32}
    if version not in record_sizes:
        raise RuntimeError(f"unsupported FPTVOX version {version}: {path}")
    count = struct.unpack_from("<Q", data, 56)[0]
    record_size = record_sizes[version]
    cells = {
        struct.unpack_from("<III", data, header_size + index * record_size)
        for index in range(count)
    }
    return header, cells


def cell_metrics(reference_path: Path, candidate_path: Path) -> dict:
    reference_header, reference = occupied_cells(reference_path)
    candidate_header, candidate = occupied_cells(candidate_path)
    same_grid = (
        reference_header["resolution"] == candidate_header["resolution"]
        and np.allclose(reference_header["bounds_min"], candidate_header["bounds_min"], atol=1e-6)
        and np.allclose(reference_header["bounds_max"], candidate_header["bounds_max"], atol=1e-6)
    )
    if not same_grid:
        return {
            "cell_iou": None,
            "reference_resolution": list(reference_header["resolution"]),
            "candidate_resolution": list(candidate_header["resolution"]),
            "reason": "cell IoU requires matching resolution and bounds",
        }
    intersection = reference & candidate
    union = reference | candidate
    return {
        "cell_iou": len(intersection) / max(1, len(union)),
        "reference_cells": len(reference),
        "candidate_cells": len(candidate),
        "cell_intersection": len(intersection),
        "cell_union": len(union),
        "reference_only_cells": len(reference - candidate),
        "candidate_only_cells": len(candidate - reference),
    }


def capture_path(report_base: Path) -> Path:
    return report_base.with_name(report_base.name + "-naadf_aadf_cpu-normal.ppm")


def render(
    args: argparse.Namespace,
    volume: Path,
    camera: dict,
    report_base: Path,
) -> tuple[Path, float]:
    capture = capture_path(report_base)
    if capture.is_file() and not args.force_render:
        return capture, 0.0
    header = volume_header(volume)
    grid = grid_camera(
        camera,
        header["resolution"][0],
        list(header["bounds_min"]),
        list(header["bounds_max"]),
    )
    source = camera["source"]
    command = [
        args.naadf.as_posix(),
        "--mode", "greedy2d",
        "--fptvox", volume.as_posix(),
        "--size", f"{args.image_size}x{args.image_size}",
        "--camera", csv_vector(grid + [camera["yaw"], camera["pitch"]]),
        "--camera-fov", str(camera["fov"]),
        "--camera-roll", str(source.get("camera_roll", 0.0)),
        "--gpu-renderer", "naadf",
        "--gpu-naadf-mode", "aadf",
        "--gpu-naadf-build", "cpu",
        "--gpu-naadf-primary-layout", "direct16",
        "--gpu-naadf-path-tracing", "fixed",
        "--gpu-naadf-path-samples-per-frame", str(args.samples),
        "--gpu-naadf-path-max-samples", str(args.samples),
        "--gpu-naadf-path-bounces", str(args.bounces),
        "--benchmark",
        "--offscreen",
        "--frames", "1",
        "--warmup", "0",
        "--capture",
        "--report", report_base.as_posix(),
    ]
    elapsed = run(
        command,
        ROOT,
        report_base.with_suffix(".stdout.log"),
        report_base.with_suffix(".stderr.log"),
    )
    if not capture.is_file():
        raise RuntimeError(f"renderer did not create capture: {capture}")
    return capture, elapsed


def render_fpt(
    args: argparse.Namespace,
    source: Path,
    scene_output: Path,
) -> tuple[Path, float]:
    directory = scene_output / "fpt-continuous"
    directory.mkdir(parents=True, exist_ok=True)
    image = directory / f"{source.stem}.png"
    if image.is_file() and not args.force_render:
        return image, 0.0
    command = [
        args.fpt.as_posix(), "render", source.as_posix(),
        "--out", directory.as_posix(),
        "--renderer", "sdf",
        "--mandelbulber-root", args.mandelbulber_root.as_posix(),
        "--width", str(args.image_size),
        "--height", str(args.image_size),
        "--samples", str(args.samples),
        "--sdf-accumulation", "per-sample",
    ]
    try:
        elapsed = run(
            command,
            ROOT,
            directory / "stdout.log",
            directory / "stderr.log",
        )
    except RuntimeError as first_error:
        retry = command.copy()
        retry[retry.index("per-sample")] = "chunked"
        retry.extend(("--sdf-chunk-samples", "4"))
        try:
            elapsed = run(
                retry,
                ROOT,
                directory / "retry.stdout.log",
                directory / "retry.stderr.log",
            )
        except RuntimeError as retry_error:
            placeholder = Image.new(
                "RGB", (args.image_size, args.image_size), (27, 29, 32)
            )
            draw = ImageDraw.Draw(placeholder)
            draw.multiline_text(
                (16, 16),
                f"FPT Metal render failed\n{source.stem}\n\n{retry_error}",
                fill=(235, 150, 150),
                font=ImageFont.load_default(),
                spacing=6,
            )
            placeholder.save(image)
            (directory / "failure.txt").write_text(
                f"per-sample:\n{first_error}\n\nchunked:\n{retry_error}\n"
            )
            elapsed = 0.0
    if not image.is_file():
        raise RuntimeError(f"FPT renderer did not create image: {image}")
    return image, elapsed


def load_rgb(path: Path) -> np.ndarray:
    return np.asarray(Image.open(path).convert("RGB"), dtype=np.uint8)


def foreground_mask(image: np.ndarray) -> np.ndarray:
    border = np.concatenate((image[0], image[-1], image[:, 0], image[:, -1]), axis=0)
    background = np.median(border.astype(np.float32), axis=0)
    distance = np.max(np.abs(image.astype(np.float32) - background), axis=2)
    return distance > 10.0


def mask_metrics(reference_path: Path, candidate_path: Path) -> dict:
    reference = foreground_mask(load_rgb(reference_path))
    candidate = foreground_mask(load_rgb(candidate_path))
    intersection = reference & candidate
    union = reference | candidate
    reference_points = np.argwhere(reference)
    candidate_points = np.argwhere(candidate)

    def centroid(points: np.ndarray) -> list[float | None]:
        if not len(points):
            return [None, None]
        value = points.mean(axis=0)
        return [float(value[1]), float(value[0])]

    def bounds(points: np.ndarray) -> list[int | None]:
        if not len(points):
            return [None, None, None, None]
        minimum = points.min(axis=0)
        maximum = points.max(axis=0)
        return [int(minimum[1]), int(minimum[0]), int(maximum[1]), int(maximum[0])]

    reference_count = int(reference.sum())
    candidate_count = int(candidate.sum())
    return {
        "foreground_iou": float(intersection.sum() / max(1, union.sum())),
        "reference_pixels": reference_count,
        "candidate_pixels": candidate_count,
        "visible_miss_pct": float((reference & ~candidate).sum() / max(1, reference_count) * 100),
        "visible_extra_pct": float((candidate & ~reference).sum() / max(1, candidate_count) * 100),
        "area_ratio": float(candidate_count / max(1, reference_count)),
        "reference_centroid_xy": centroid(reference_points),
        "candidate_centroid_xy": centroid(candidate_points),
        "reference_bounds_xyxy": bounds(reference_points),
        "candidate_bounds_xyxy": bounds(candidate_points),
    }


def image_tile(path: Path, size: int) -> Image.Image:
    image = Image.open(path).convert("RGB")
    image.thumbnail((size, size), Image.Resampling.LANCZOS)
    tile = Image.new("RGB", (size, size), (12, 14, 17))
    tile.paste(image, ((size - image.width) // 2, (size - image.height) // 2))
    return tile


def make_sheet(rows: list[dict], path: Path, tile_size: int) -> None:
    label_height = 56
    title_height = 78
    columns = [
        ("Mandelbulber authored", "mandelbulber_image"),
        ("FPT Metal continuous", "fpt_image"),
        ("Mandel PLY -> NAADF", "ply_image"),
        ("Direct V7 -> NAADF", "v7_image"),
    ]
    canvas = Image.new(
        "RGB",
        (tile_size * len(columns), title_height + len(rows) * (tile_size + label_height)),
        (27, 29, 32),
    )
    draw = ImageDraw.Draw(canvas)
    font = ImageFont.load_default()
    for column, (label, _) in enumerate(columns):
        draw.text((column * tile_size + 12, 22), label, fill=(235, 237, 240), font=font)
    for row_index, row in enumerate(rows):
        y = title_height + row_index * (tile_size + label_height)
        for column, (_, key) in enumerate(columns):
            canvas.paste(image_tile(Path(row[key]), tile_size), (column * tile_size, y))
        metrics = row["image_alignment"]
        cells = row["cell_alignment"]
        cell_text = (
            f"cell IoU {cells['cell_iou']:.3f}"
            if cells["cell_iou"] is not None
            else "cell IoU n/a (different grids)"
        )
        summary = (
            f"{row['rank']:02d} {row['name']} | PLY vs V7: "
            f"{cell_text}, visible IoU {metrics['foreground_iou']:.3f}, "
            f"area {metrics['area_ratio']:.3f}"
        )
        draw.text((12, y + tile_size + 14), summary, fill=(218, 221, 225), font=font)
    canvas.save(path)


def main() -> None:
    args = parse_args()
    ranks = [int(value) for value in args.ranks.split(",") if value]
    args.output.mkdir(parents=True, exist_ok=True)
    manifest = {int(row["rank"]): row for row in json.loads(HISTORICAL_MANIFEST.read_text())}
    scene_config = (
        {int(rank): value for rank, value in json.loads(args.scene_config.read_text()).items()}
        if args.scene_config
        else {}
    )
    rows: list[dict] = []

    for rank in ranks:
        overrides = scene_config.get(rank, {})
        scene_directory = first_scene_directory(rank)
        historical_row = manifest[rank]
        camera = json.loads((scene_directory / "naadf-view-result.json").read_text())["camera"]
        archived_grid = np.asarray(camera["grid_position"], dtype=np.float64)
        mapped_grid = np.asarray(grid_camera(camera, 192), dtype=np.float64)
        if not np.allclose(archived_grid, mapped_grid, atol=1e-5, rtol=0.0):
            raise RuntimeError(
                f"rank {rank}: canonical camera mapping diverged from the archived grid camera"
            )
        source = Path(historical_row.get("source") or source_for(scene_directory))
        legacy_image_coordinates = uses_legacy_image_coordinates(source)
        export_bounds_min = overrides.get("bounds_min") or (
            camera["bounds_min"] if args.bounds_mode == "archived" else parse_vector(args.bounds_min)
        )
        export_bounds_max = overrides.get("bounds_max") or (
            camera["bounds_max"] if args.bounds_mode == "archived" else parse_vector(args.bounds_max)
        )
        voxel_resolution = int(overrides.get("voxel_resolution", args.voxel_resolution))
        triangle_resolution = int(
            overrides.get("triangle_resolution", args.triangle_resolution)
        )
        threshold_scale = float(overrides.get("threshold_scale", 1.0))
        scene_output = args.output / f"{rank:02d}"
        scene_output.mkdir(parents=True, exist_ok=True)
        alignment_path = scene_output / "alignment.json"
        if args.reuse_completed and alignment_path.is_file():
            row = json.loads(alignment_path.read_text())
            required_images = (
                row["mandelbulber_image"],
                row["fpt_image"],
                row["ply_image"],
                row["v7_image"],
            )
            if all(Path(image).is_file() for image in required_images):
                rows.append(row)
                print(f"[{rank:02d}] {row['name']}: cached", flush=True)
                continue
        v7 = Path(overrides["v7_volume"]) if "v7_volume" in overrides else (
            args.v7_volume_root / f"{rank:02d}" / "scene-v7.fptvox"
            if args.v7_volume_root
            else scene_output / "scene-view-v7.fptvox"
        )
        export_report_path = scene_output / "v7-export.json"
        export_seconds = 0.0
        topology_watchdog_fallback = False
        if args.v7_volume_root and not v7.is_file():
            raise RuntimeError(f"rank {rank}: external V7 volume is unavailable: {v7}")
        if not args.v7_volume_root and (
            args.force_export or not (v7.is_file() and export_report_path.is_file())
        ):
            command = [
                args.fpt.as_posix(), "voxel-export", source.as_posix(),
                "--out", v7.as_posix(),
                "--voxel-resolution", str(voxel_resolution),
                "--bounds-min", csv_vector(export_bounds_min),
                "--bounds-max", csv_vector(export_bounds_max),
                "--mandelbulber-root", args.mandelbulber_root.as_posix(),
                "--surface-triangles",
                "--surface-triangle-resolution", str(triangle_resolution),
                "--surface-triangle-threshold-scale", str(threshold_scale),
            ]
            try:
                export_seconds = run(
                    command,
                    ROOT,
                    scene_output / "v7-export.stdout.log",
                    scene_output / "v7-export.stderr.log",
                )
                export_stdout = scene_output / "v7-export.stdout.log"
            except RuntimeError as error:
                if "Impacting Interactivity" not in str(error) or triangle_resolution <= 256:
                    raise
                triangle_resolution = 256
                topology_watchdog_fallback = True
                command[command.index("--surface-triangle-resolution") + 1] = str(
                    triangle_resolution
                )
                export_seconds = run(
                    command,
                    ROOT,
                    scene_output / "v7-export-fallback.stdout.log",
                    scene_output / "v7-export-fallback.stderr.log",
                )
                export_stdout = scene_output / "v7-export-fallback.stdout.log"
            shutil.copyfile(export_stdout, export_report_path)

        ply_volume = Path(overrides["reference_volume"]) if "reference_volume" in overrides else (
            args.reference_volume_root / f"{rank:02d}" / "scene-ply.fptvox"
            if args.reference_volume_root
            else Path(historical_row["volume"])
        )
        if not ply_volume.is_file():
            raise RuntimeError(f"rank {rank}: reference volume is unavailable: {ply_volume}")
        ply_image, ply_seconds = render(args, ply_volume, camera, scene_output / "ply")
        v7_image, v7_seconds = render(
            args,
            v7,
            camera,
            scene_output / "v7",
        )
        raw_fpt_image, fpt_seconds = render_fpt(args, source, scene_output)
        comparison_fpt_image = normalized_image(
            raw_fpt_image,
            scene_output / "fpt-continuous-camera-normalized.png",
            legacy_image_coordinates,
        )
        comparison_ply_image = normalized_image(
            ply_image,
            scene_output / "ply-naadf-camera-normalized.png",
            legacy_image_coordinates,
        )
        comparison_v7_image = normalized_image(
            v7_image,
            scene_output / "v7-naadf-camera-normalized.png",
            legacy_image_coordinates,
        )
        image_alignment = mask_metrics(comparison_ply_image, comparison_v7_image)
        cell_alignment = cell_metrics(ply_volume, v7)
        row = {
            "rank": rank,
            "name": historical_row["name"],
            "source": source.as_posix(),
            "reference_bounds_min": camera["bounds_min"],
            "reference_bounds_max": camera["bounds_max"],
            "v7_bounds_min": export_bounds_min,
            "v7_bounds_max": export_bounds_max,
            "world_camera": camera["world_position"],
            "ply_grid_camera": grid_camera(camera, 192),
            "v7_grid_camera": grid_camera(
                camera, voxel_resolution, export_bounds_min, export_bounds_max
            ),
            "yaw": camera["yaw"],
            "pitch": camera["pitch"],
            "roll": camera["source"].get("camera_roll", 0.0),
            "fov": camera["fov"],
            "mandelbulber_image": historical_row["reference"],
            "legacy_coordinate_system": legacy_image_coordinates,
            "fpt_image": comparison_fpt_image.as_posix(),
            "ply_image": comparison_ply_image.as_posix(),
            "v7_image": comparison_v7_image.as_posix(),
            "raw_fpt_image": raw_fpt_image.as_posix(),
            "raw_ply_image": ply_image.as_posix(),
            "raw_v7_image": v7_image.as_posix(),
            "v7_volume": v7.as_posix(),
            "reference_volume": ply_volume.as_posix(),
            "voxel_resolution": voxel_resolution,
            "triangle_resolution": triangle_resolution,
            "topology_watchdog_fallback": topology_watchdog_fallback,
            "threshold_scale": threshold_scale,
            "export_seconds": export_seconds,
            "fpt_render_seconds": fpt_seconds,
            "ply_render_seconds": ply_seconds,
            "v7_render_seconds": v7_seconds,
            "image_alignment": image_alignment,
            "cell_alignment": cell_alignment,
        }
        rows.append(row)
        alignment_path.write_text(json.dumps(row, indent=2) + "\n")
        if args.compact_output:
            for disposable in (v7, ply_image, v7_image):
                try:
                    disposable.relative_to(args.output)
                except ValueError:
                    continue
                disposable.unlink(missing_ok=True)
        cell_text = (
            f"{cell_alignment['cell_iou']:.3f}"
            if cell_alignment["cell_iou"] is not None
            else "n/a"
        )
        print(f"[{rank:02d}] {row['name']}: cell IoU {cell_text}, "
              f"visible IoU {image_alignment['foreground_iou']:.3f}, "
              f"area {image_alignment['area_ratio']:.3f}")

    summary_path = args.output / "summary.json"
    summary_path.write_text(json.dumps(rows, indent=2) + "\n")
    sheet_path = args.output / "latest-naadf-comparison-contact-sheet.png"
    make_sheet(rows, sheet_path, args.sheet_tile_size or args.image_size)
    if args.sheet_page_rows > 0 and len(rows) > args.sheet_page_rows:
        for start in range(0, len(rows), args.sheet_page_rows):
            page = rows[start : start + args.sheet_page_rows]
            page_path = args.output / (
                f"latest-naadf-comparison-{page[0]['rank']:02d}-{page[-1]['rank']:02d}.png"
            )
            make_sheet(page, page_path, args.sheet_tile_size or args.image_size)
    print(sheet_path)


if __name__ == "__main__":
    main()
