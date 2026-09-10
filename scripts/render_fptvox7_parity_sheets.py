#!/usr/bin/env python3
"""Render deterministic geometry, material, and voxel-appearance comparisons."""

from __future__ import annotations

import argparse
import json
import math
import shutil
import struct
import subprocess
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_ALIGNMENT = (
    ROOT
    / "reports/fptvox7-first3-zero-gradient-interior-20260815/alignment-summary.json"
)
DEFAULT_OUTPUT = ROOT / "reports/fptvox7-first3-zero-gradient-interior-20260815/parity-direct"
DEFAULT_NAADF = Path(
    "/Users/jordantotty/Desktop/vox/metal-voxel-naadf-pathtracer/"
    "build/MetalVoxel.app/Contents/MacOS/MetalVoxel"
)
DEFAULT_MANDEL_ROOT = Path("/Volumes/Ventura/Projects/mandelbulber2/mandelbulber2")
RECORD_SIZES = {1: 24, 2: 28, 3: 28, 5: 32, 6: 36, 7: 32, 8: 32, 10: 32}
STRUCTURAL_RECORD_SIZES = {2: 64, 3: 80}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--alignment-summary", type=Path, default=DEFAULT_ALIGNMENT)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--reference-volume-root", type=Path)
    parser.add_argument("--fpt", type=Path, default=ROOT / "target/release/fpt-metal")
    parser.add_argument("--naadf", type=Path, default=DEFAULT_NAADF)
    parser.add_argument("--mandelbulber-root", type=Path, default=DEFAULT_MANDEL_ROOT)
    parser.add_argument("--image-size", type=int, default=512)
    parser.add_argument(
        "--camera-mode",
        choices=("authored", "fit-volume"),
        default="authored",
        help="use the authored camera or fit its orientation to the compared occupied volume",
    )
    parser.add_argument(
        "--minimum-exact-coverage",
        type=float,
        default=0.02,
        help=(
            "retain the authored camera and mark the exact surface under-resolved when no "
            "fitted direction covers this fraction of the image"
        ),
    )
    parser.add_argument(
        "--native-capture-orientation",
        choices=("native", "mirror-x"),
        default="native",
        help=(
            "orientation applied to native NAADF captures before image-space metrics; "
            "native is the verified default and mirror-x is diagnostic only"
        ),
    )
    parser.add_argument("--force", action="store_true")
    args = parser.parse_args()
    if not 0.0 <= args.minimum_exact_coverage <= 1.0:
        parser.error("--minimum-exact-coverage must be between 0 and 1")
    return args


def run(command: list[str], cwd: Path, stdout: Path, stderr: Path) -> None:
    result = subprocess.run(command, cwd=cwd, text=True, capture_output=True, timeout=1800)
    stdout.write_text(result.stdout)
    stderr.write_text(result.stderr)
    if result.returncode:
        raise RuntimeError(result.stderr.strip() or result.stdout.strip())


def csv(values: list[float]) -> str:
    return ",".join(f"{value:.9g}" for value in values)


def naadf_fov_degrees(fpt_fov_degrees: float) -> float:
    """Convert FPT's half-width image-plane FOV to NAADF's full NDC span."""
    tangent = math.tan(math.radians(fpt_fov_degrees) * 0.5) * 0.5
    return math.degrees(2.0 * math.atan(tangent))


def metric(value: float | None, digits: int = 2) -> str:
    return "n/a" if value is None else f"{value:.{digits}f}"


def is_fptvox_magic(data: bytes) -> bool:
    return data[:6] == b"FPTVOX" and (data[7] == 0 or data[:8] == b"FPTVOX10")


def volume_arrays(path: Path) -> dict:
    data = path.read_bytes()
    if len(data) < 64 or not is_fptvox_magic(data):
        raise RuntimeError(f"invalid FPTVOX volume: {path}")
    header_size, version = struct.unpack_from("<II", data, 8)
    if version not in RECORD_SIZES:
        raise RuntimeError(f"unsupported FPTVOX version {version}: {path}")
    resolution = struct.unpack_from("<III", data, 16)
    count = struct.unpack_from("<Q", data, 56)[0]
    record_size = RECORD_SIZES[version]
    expected = header_size + count * record_size
    if len(data) < expected:
        raise RuntimeError(f"truncated FPTVOX records: {path}")

    def words(offset: int) -> np.ndarray:
        return np.ndarray(
            shape=(count,), dtype="<u4", buffer=data, offset=header_size + offset,
            strides=(record_size,),
        ).copy()

    x, y, z = words(0), words(4), words(8)
    packed_color, packed_pbr, emission = words(12), words(16), words(20)
    linear = x.astype(np.uint64)
    linear += y.astype(np.uint64) * resolution[0]
    linear += z.astype(np.uint64) * resolution[0] * resolution[1]
    sample_stride = max(1, math.ceil(int(count) / 100_000))
    projection_sample = np.column_stack(
        (x[::sample_stride], y[::sample_stride], z[::sample_stride])
    ).astype(np.float64)

    material_lookup: dict[tuple[int, int, int], int] = {}
    material_colors = [[0.0, 0.0, 0.0]]
    for color, pbr, emit in zip(packed_color, packed_pbr, emission, strict=True):
        key = (int(color), int(pbr), int(emit))
        if key in material_lookup:
            continue
        material_lookup[key] = len(material_colors)
        material_colors.append(
            [
                float((int(color) >> 0) & 255) / 255.0,
                float((int(color) >> 8) & 255) / 255.0,
                float((int(color) >> 16) & 255) / 255.0,
            ]
        )
    return {
        "resolution": resolution,
        "linear": linear,
        "occupied_min": [int(x.min()), int(y.min()), int(z.min())] if count else [0, 0, 0],
        "occupied_max": [int(x.max()), int(y.max()), int(z.max())] if count else [0, 0, 0],
        "projection_sample": projection_sample,
        "packed_color": packed_color,
        "material_colors": np.asarray(material_colors, dtype=np.float32),
    }


def volume_header(path: Path) -> dict:
    with path.open("rb") as handle:
        data = handle.read(64)
    if len(data) != 64 or not is_fptvox_magic(data):
        raise RuntimeError(f"invalid FPTVOX volume: {path}")
    return {
        "resolution": struct.unpack_from("<III", data, 16),
        "bounds_min": struct.unpack_from("<fff", data, 32),
        "bounds_max": struct.unpack_from("<fff", data, 44),
    }


def basis_from_forward(
    forward: np.ndarray, roll: float = 0.0
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    forward = np.asarray(forward, dtype=np.float64)
    forward /= np.linalg.norm(forward)
    world_up = np.asarray([0.0, 1.0, 0.0], dtype=np.float64)
    if abs(float(np.dot(forward, world_up))) > 0.999:
        world_up = np.asarray([0.0, 0.0, 1.0], dtype=np.float64)
    right = np.cross(world_up, forward)
    right /= np.linalg.norm(right)
    up = np.cross(forward, right)
    if roll != 0.0:
        unrolled_right = right.copy()
        right = unrolled_right * math.cos(roll) + up * math.sin(roll)
        up = up * math.cos(roll) - unrolled_right * math.sin(roll)
    return forward, right, up


def camera_basis(row: dict) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    forward = np.asarray(
        [
            -math.sin(row["yaw"]) * math.cos(row["pitch"]),
            math.sin(row["pitch"]),
            -math.cos(row["yaw"]) * math.cos(row["pitch"]),
        ],
        dtype=np.float64,
    )
    return basis_from_forward(forward, float(row.get("roll", 0.0)))


def projection_coverage(points: np.ndarray, forward: np.ndarray, roll: float) -> int:
    _, right, up = basis_from_forward(forward, roll)
    centered = points - np.median(points, axis=0)
    projected = np.column_stack((centered @ right, centered @ up))
    low, high = np.quantile(projected, [0.005, 0.995], axis=0)
    extent = np.maximum(high - low, 1.0e-12)
    normalized = (projected - low) / extent
    inside = np.all((normalized >= 0.0) & (normalized <= 1.0), axis=1)
    bins = np.clip((normalized[inside] * 95.0).astype(np.int32), 0, 95)
    return int(np.unique(bins[:, 0] + bins[:, 1] * 96).size)


def comparison_directions(row: dict, points: np.ndarray) -> list[tuple[np.ndarray, float, str]]:
    authored = camera_basis(row)[0]
    candidates = [authored]
    candidates.extend(
        np.asarray([x, y, z], dtype=np.float64)
        for x in (-1.0, 0.0, 1.0)
        for y in (-1.0, 0.0, 1.0)
        for z in (-1.0, 0.0, 1.0)
        if (x, y, z) != (0.0, 0.0, 0.0)
    )
    scores = [
        projection_coverage(points, direction, float(row.get("roll", 0.0)) if index == 0 else 0.0)
        for index, direction in enumerate(candidates)
    ]
    best_index = int(np.argmax(scores))
    # Preserve the authored composition unless another view exposes substantially
    # more of the clipped surface.
    preferred = 0 if scores[0] >= scores[best_index] * 0.9 else best_index
    order = [preferred]
    order.extend(
        index
        for index in sorted(range(len(candidates)), key=lambda index: scores[index], reverse=True)
        if index != preferred
    )
    result = []
    seen = set()
    for index in order:
        direction = candidates[index] / np.linalg.norm(candidates[index])
        key = tuple(np.round(direction, 6))
        if key in seen:
            continue
        seen.add(key)
        result.append(
            (
                direction,
                float(row.get("roll", 0.0)) if index == 0 else 0.0,
                "authored-direction" if index == 0 else "coverage-direction",
            )
        )
    return result


def direction_angles(forward: np.ndarray) -> tuple[float, float]:
    return (
        math.atan2(-float(forward[0]), -float(forward[2])),
        math.asin(float(np.clip(forward[1], -1.0, 1.0))),
    )


def volume_world_points(header: dict, volumes: list[dict]) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    resolution = np.asarray(header["resolution"], dtype=np.float64)
    bounds_min = np.asarray(header["bounds_min"], dtype=np.float64)
    bounds_max = np.asarray(header["bounds_max"], dtype=np.float64)
    grid_points = np.concatenate(
        [volume["projection_sample"] + 0.5 for volume in volumes], axis=0
    )
    points = bounds_min + grid_points / resolution * (bounds_max - bounds_min)
    return points, bounds_min, bounds_max


def fit_points_camera(
    row: dict,
    points: np.ndarray,
    bounds_min: np.ndarray,
    bounds_max: np.ndarray,
    forward: np.ndarray,
    roll: float,
    direction_mode: str,
) -> dict:
    forward, right, up = basis_from_forward(forward, roll)
    projected = np.column_stack((points @ right, points @ up, points @ forward))
    low, high = np.quantile(projected, [0.005, 0.995], axis=0)
    midpoint = (low + high) * 0.5
    center = right * midpoint[0] + up * midpoint[1] + forward * midpoint[2]
    centered = projected - midpoint
    inside = np.all((projected >= low) & (projected <= high), axis=1)
    centered = centered[inside]
    tan_half_fov = math.tan(math.radians(float(row["fov"])) * 0.5)
    distance = max(
        float(np.max(np.abs(centered[:, 0]) / tan_half_fov - centered[:, 2])),
        float(np.max(np.abs(centered[:, 1]) / tan_half_fov - centered[:, 2])),
        float(np.max(-centered[:, 2])),
    )
    # Leave a stable border while retaining the authored view direction and roll.
    distance = max(distance * 1.15, float(np.linalg.norm(high - low)) * 0.05)
    fitted = dict(row)
    fitted["world_camera"] = (center - forward * distance).tolist()
    fitted["yaw"], fitted["pitch"] = direction_angles(forward)
    fitted["roll"] = roll
    fitted["comparison_camera_mode"] = "fit-volume"
    fitted["comparison_direction_mode"] = direction_mode
    fitted["comparison_target"] = center.tolist()
    fitted["comparison_bounds_min"] = bounds_min.tolist()
    fitted["comparison_bounds_max"] = bounds_max.tolist()
    return fitted


def fit_volume_camera_candidates(row: dict, header: dict, volumes: list[dict]) -> list[dict]:
    points, bounds_min, bounds_max = volume_world_points(header, volumes)
    return [
        fit_points_camera(
            row, points, bounds_min, bounds_max, forward, roll, direction_mode
        )
        for forward, roll, direction_mode in comparison_directions(row, points)
    ]


def fit_volume_camera(row: dict, header: dict, volumes: list[dict]) -> dict:
    return fit_volume_camera_candidates(row, header, volumes)[0]


def cell_color_metrics(reference: dict, candidate: dict) -> dict:
    if reference["resolution"] != candidate["resolution"]:
        return {"reason": "different resolutions"}
    common, reference_indices, candidate_indices = np.intersect1d(
        reference["linear"], candidate["linear"], assume_unique=True, return_indices=True
    )
    reference_rgb = packed_rgb(reference["packed_color"][reference_indices])
    candidate_rgb = packed_rgb(candidate["packed_color"][candidate_indices])
    delta = np.abs(reference_rgb - candidate_rgb)
    return {
        "common_cells": int(common.size),
        "exact_rgb_pct": float(np.all(delta == 0.0, axis=1).mean() * 100.0),
        "rgb_mae_255": float(delta.mean() * 255.0),
        "rgb_rmse_255": float(np.sqrt(np.mean(np.square(delta))) * 255.0),
        "rgb_max_255": float(delta.max(initial=0.0) * 255.0),
    }


def packed_rgb(packed: np.ndarray) -> np.ndarray:
    return np.stack(
        [
            (packed >> 0) & 255,
            (packed >> 8) & 255,
            (packed >> 16) & 255,
        ],
        axis=1,
    ).astype(np.float32) / 255.0


def render_visibility(
    args: argparse.Namespace,
    row: dict,
    volume: Path,
    surface_mode: str,
    output: Path,
) -> Path:
    base = output / "visibility"
    manifest = Path(f"{base}.json")
    binary = Path(f"{base}.svdagVisibility.bin")
    camera = row["world_camera"] + [row["yaw"], row["pitch"]]
    camera_contract = {
        "version": 1,
        "volume": volume.resolve().as_posix(),
        "surface_mode": surface_mode,
        "image_size": args.image_size,
        "world_camera": row["world_camera"],
        "yaw": row["yaw"],
        "pitch": row["pitch"],
        "roll": row.get("roll", 0.0),
        "fpt_fov_degrees": row["fov"],
        "naadf_fov_degrees": naadf_fov_degrees(float(row["fov"])),
        "naadf_pixel_offset": [-0.5, -0.5],
    }
    contract_path = output / "camera-contract.json"
    if manifest.is_file() and binary.is_file() and contract_path.is_file() and not args.force:
        if json.loads(contract_path.read_text()) == camera_contract:
            return base
    output.mkdir(parents=True, exist_ok=True)
    header = volume_header(volume)
    bounds_min = header["bounds_min"]
    bounds_max = header["bounds_max"]
    grid_camera = [
        (row["world_camera"][axis] - bounds_min[axis])
        / (bounds_max[axis] - bounds_min[axis])
        * header["resolution"][axis]
        for axis in range(3)
    ]
    camera = grid_camera + camera[3:]
    report = output / "render"
    command = [
        args.naadf.as_posix(),
        "--mode", "greedy2d",
        "--fptvox", volume.as_posix(),
        "--size", f"{args.image_size}x{args.image_size}",
        "--camera", csv(camera),
        "--camera-fov", str(naadf_fov_degrees(float(row["fov"]))),
        "--camera-roll", str(row.get("roll", 0.0)),
        "--camera-pixel-offset", "-0.5,-0.5",
        "--gpu-renderer", "naadf",
        "--gpu-naadf-mode", "aadf",
        "--gpu-naadf-build", "cpu",
        "--gpu-naadf-primary-layout", "direct16",
        "--gpu-naadf-path-tracing", "fixed",
        "--gpu-naadf-path-samples-per-frame", "1",
        "--gpu-naadf-path-max-samples", "1",
        "--gpu-naadf-path-bounces", "1",
        "--gpu-naadf-fptvox-surface-mode", surface_mode,
        "--gpu-visibility-dump", base.as_posix(),
        "--benchmark", "--offscreen", "--frames", "1", "--warmup", "0",
        "--report", report.as_posix(),
    ]
    run(command, ROOT, output / "stdout.log", output / "stderr.log")
    if not manifest.is_file() or not binary.is_file():
        raise RuntimeError(f"NAADF visibility dump was not created: {base}")
    contract_path.write_text(json.dumps(camera_contract, indent=2) + "\n")
    return base


def visibility_images(
    base: Path,
    volume: dict,
    forward: np.ndarray,
    fov_degrees: float,
    roll: float,
    flip_y: bool,
    flip_x: bool,
) -> dict[str, np.ndarray]:
    manifest = json.loads(Path(f"{base}.json").read_text())
    width, height = int(manifest["width"]), int(manifest["height"])
    words = np.fromfile(f"{base}.svdagVisibility.bin", dtype="<u4")
    words = words.reshape(height, width, 8)
    depth = words[:, :, 0].copy().view("<f4")
    material = words[:, :, 1]
    normal = words[:, :, 5:8].copy().view("<f4").reshape(height, width, 3)
    hit = material != 0
    if flip_y:
        depth = np.flipud(depth)
        material = np.flipud(material)
        normal = np.flipud(normal)
        hit = np.flipud(hit)
    if flip_x:
        # Keep any diagnostic orientation transform explicit and apply it to
        # every visibility lane before calculating image-space metrics.
        depth = np.fliplr(depth)
        material = np.fliplr(material)
        normal = np.fliplr(normal)
        hit = np.fliplr(hit)
    normal_length = np.linalg.norm(normal, axis=2, keepdims=True)
    normal = np.divide(normal, normal_length, out=np.zeros_like(normal), where=normal_length > 0)
    world_up = np.asarray([0.0, 1.0, 0.0], dtype=np.float32)
    right = np.cross(world_up, forward)
    right /= np.linalg.norm(right)
    up = np.cross(forward, right)
    if roll != 0.0:
        unrolled_right = right.copy()
        right = unrolled_right * math.cos(roll) + up * math.sin(roll)
        up = up * math.cos(roll) - unrolled_right * math.sin(roll)
    px = np.arange(width, dtype=np.float32) / width
    py = np.arange(height, dtype=np.float32) / height
    ndc_x = px * 2.0 - 1.0
    ndc_y = 1.0 - py * 2.0
    tan_y = math.tan(math.radians(naadf_fov_degrees(fov_degrees)) * 0.5)
    rays = (
        forward[None, None, :]
        + right[None, None, :] * (ndc_x[None, :, None] * tan_y * width / height)
        + up[None, None, :] * (ndc_y[:, None, None] * tan_y)
    )
    rays /= np.linalg.norm(rays, axis=2, keepdims=True)
    normal = np.where((np.sum(normal * rays, axis=2) > 0.0)[:, :, None], -normal, normal)
    shade = 0.12 + 0.88 * np.maximum(np.sum(normal * -forward, axis=2), 0.0)
    white = np.zeros((height, width, 3), dtype=np.float32)
    white[hit] = shade[hit, None]
    colors = volume["material_colors"]
    if int(material.max(initial=0)) >= len(colors):
        raise RuntimeError(f"visibility material index exceeds volume table: {base}")
    unlit = colors[material]
    unlit[~hit] = 0.0
    lit = unlit * shade[:, :, None]
    lit[~hit] = 0.0
    return {
        "white": white,
        "unlit": unlit,
        "lit": lit,
        "hit": hit,
        "depth": depth,
        "normal": normal,
    }


def render_fpt_geometry(args: argparse.Namespace, row: dict, output: Path) -> dict:
    output.mkdir(parents=True, exist_ok=True)
    existing = list(output.glob("*.png"))
    structural = output / "structural.bin"
    manifest = Path(f"{structural}.json")
    if existing and structural.is_file() and manifest.is_file() and not args.force:
        metadata = json.loads(manifest.read_text())
        if metadata.get("version") in STRUCTURAL_RECORD_SIZES:
            return load_fpt_structural(existing[0], structural, args.image_size)
    command = [
        args.fpt.as_posix(), "diagnostic", row["source"],
        "--out", output.as_posix(),
        "--mode", "diffuse-normal",
        "--structural-dump", structural.as_posix(),
        "--mandelbulber-root", args.mandelbulber_root.as_posix(),
        "--width", str(args.image_size),
        "--height", str(args.image_size),
    ]
    if row.get("comparison_camera_mode") == "fit-volume":
        command.extend(
            [
                "--camera-position", csv(row["world_camera"]),
                "--camera-yaw-pitch", csv([row["yaw"], row["pitch"]]),
                "--camera-roll", str(row.get("roll", 0.0)),
                "--camera-fov", str(row["fov"]),
                "--diagnostic-clip-voxel-bounds",
                "--diagnostic-bounds-min", csv(row["comparison_bounds_min"]),
                "--diagnostic-bounds-max", csv(row["comparison_bounds_max"]),
            ]
        )
    run(command, ROOT, output / "stdout.log", output / "stderr.log")
    images = list(output.glob("*.png"))
    if len(images) != 1:
        raise RuntimeError(f"expected one FPT diagnostic image in {output}")
    return load_fpt_structural(images[0], structural, args.image_size)


def load_fpt_structural(image: Path, structural: Path, image_size: int) -> dict:
    metadata = json.loads(Path(f"{structural}.json").read_text())
    record_bytes = STRUCTURAL_RECORD_SIZES.get(metadata.get("version"))
    if (record_bytes is None or metadata.get("record_bytes") != record_bytes
            or metadata.get("format") != "FptStructuralDiagnostic"
            or metadata.get("byte_order") != "little-endian"
            or metadata.get("row_order") != "top-to-bottom"
            or metadata.get("width") != image_size or metadata.get("height") != image_size):
        raise RuntimeError(f"unsupported or mismatched FPT structural manifest: {structural}")
    expected_bytes = image_size * image_size * record_bytes
    if structural.stat().st_size != expected_bytes:
        raise RuntimeError(f"FPT structural dump must contain {expected_bytes} bytes: {structural}")
    values = np.fromfile(structural, dtype="<f4")
    records = values.reshape(image_size, image_size, record_bytes // 4)
    return {
        "image": image,
        "position": records[:, :, 0:3],
        "depth": records[:, :, 3],
        "normal": records[:, :, 4:7],
        "hit": records[:, :, 7] > 0.5,
        "color_coordinate": records[:, :, 8],
        "palette_position": records[:, :, 9],
        "material": records[:, :, 12:15],
    }


def load_float_image(path: Path) -> np.ndarray:
    return np.asarray(Image.open(path).convert("RGB"), dtype=np.float32) / 255.0


def save_image(image: np.ndarray, path: Path) -> None:
    encoded = np.clip(np.round(image * 255.0), 0, 255).astype(np.uint8)
    Image.fromarray(encoded, "RGB").save(path)


def image_metrics(reference: np.ndarray, candidate: np.ndarray) -> dict:
    reference_hit = np.any(reference > 0.02, axis=2)
    candidate_hit = np.any(candidate > 0.02, axis=2)
    union = reference_hit | candidate_hit
    common = reference_hit & candidate_hit
    delta = np.abs(reference - candidate)
    return {
        "visible_iou": float(common.sum() / max(1, union.sum())),
        "visible_miss_pct": float((reference_hit & ~candidate_hit).sum() / max(1, reference_hit.sum()) * 100),
        "visible_extra_pct": float((candidate_hit & ~reference_hit).sum() / max(1, candidate_hit.sum()) * 100),
        "image_mae_255": float(delta.mean() * 255.0),
        "common_visible_mae_255": float(delta[common].mean() * 255.0) if common.any() else None,
    }


def mask_metrics(reference: np.ndarray, candidate: np.ndarray) -> dict:
    union = reference | candidate
    common = reference & candidate
    return {
        "visible_iou": float(common.sum() / max(1, union.sum())),
        "visible_miss_pct": float((reference & ~candidate).sum() / max(1, reference.sum()) * 100),
        "visible_extra_pct": float((candidate & ~reference).sum() / max(1, candidate.sum()) * 100),
    }


def depth_metrics(reference: dict, candidate: dict) -> dict:
    common = reference["hit"] & candidate["hit"]
    if not common.any():
        return {"reason": "no common visible pixels"}
    reference_depth = reference["depth"][common]
    candidate_depth = candidate["depth"][common]
    valid = np.isfinite(reference_depth) & np.isfinite(candidate_depth)
    reference_depth = reference_depth[valid]
    candidate_depth = candidate_depth[valid]
    scale = float(np.median(reference_depth / np.maximum(candidate_depth, 1.0e-20)))
    scaled_candidate = candidate_depth * scale
    delta = np.abs(reference_depth - scaled_candidate)
    per_pixel_relative = delta / np.maximum(np.abs(reference_depth), 1.0e-20)
    normalizer = max(float(np.median(np.abs(reference_depth))), 1.0e-20)
    positive = (reference_depth > 0.0) & (scaled_candidate > 0.0)
    log_correlation = None
    if positive.sum() > 1:
        log_correlation = float(
            np.corrcoef(
                np.log(reference_depth[positive]), np.log(scaled_candidate[positive])
            )[0, 1]
        )
    return {
        "common_pixels": int(reference_depth.size),
        "candidate_to_reference_scale": scale,
        "relative_mae_pct": float(delta.mean() / normalizer * 100.0),
        "relative_p95_pct": float(np.quantile(delta, 0.95) / normalizer * 100.0),
        "per_pixel_relative_mean_pct": float(per_pixel_relative.mean() * 100.0),
        "per_pixel_relative_median_pct": float(np.median(per_pixel_relative) * 100.0),
        "per_pixel_relative_p95_pct": float(np.quantile(per_pixel_relative, 0.95) * 100.0),
        "within_5pct_pct": float((per_pixel_relative <= 0.05).mean() * 100.0),
        "within_10pct_pct": float((per_pixel_relative <= 0.10).mean() * 100.0),
        "log_depth_correlation": log_correlation,
    }


def normal_metrics(reference: dict, candidate: dict) -> dict:
    common = reference["hit"] & candidate["hit"]
    if not common.any():
        return {"reason": "no common visible pixels"}
    dot = np.sum(reference["normal"][common] * candidate["normal"][common], axis=1)
    angle = np.degrees(np.arccos(np.clip(dot, -1.0, 1.0)))
    return {
        "common_pixels": int(angle.size),
        "mean_degrees": float(angle.mean()),
        "median_degrees": float(np.median(angle)),
        "p95_degrees": float(np.quantile(angle, 0.95)),
        "over_25_degrees_pct": float((angle > 25.0).mean() * 100.0),
    }


def normal_difference(reference: dict, candidate: dict) -> np.ndarray:
    common = reference["hit"] & candidate["hit"]
    dot = np.sum(reference["normal"] * candidate["normal"], axis=2)
    angle = np.degrees(np.arccos(np.clip(dot, -1.0, 1.0)))
    intensity = np.clip(angle / 90.0, 0.0, 1.0)
    image = np.zeros((*common.shape, 3), dtype=np.float32)
    image[common] = np.stack(
        [intensity[common], 0.25 * (1.0 - intensity[common]), 1.0 - intensity[common]],
        axis=1,
    )
    return image


def mask_difference(reference: np.ndarray, candidate: np.ndarray) -> np.ndarray:
    image = np.zeros((*reference.shape, 3), dtype=np.float32)
    image[reference & ~candidate] = [1.0, 0.25, 0.1]
    image[candidate & ~reference] = [0.1, 0.65, 1.0]
    image[reference & candidate] = [0.18, 0.18, 0.18]
    return image


def difference(reference: np.ndarray, candidate: np.ndarray, gain: float = 4.0) -> np.ndarray:
    return np.clip(np.abs(reference - candidate) * gain, 0.0, 1.0)


def make_sheet(rows: list[dict], columns: list[tuple[str, str]], path: Path, subtitle: str) -> None:
    tile = 360
    title = 82
    footer = 60
    canvas = Image.new(
        "RGB", (tile * len(columns), title + len(rows) * (tile + footer)), (27, 29, 32)
    )
    draw = ImageDraw.Draw(canvas)
    font = ImageFont.load_default()
    draw.text((12, 12), subtitle, fill=(185, 190, 198), font=font)
    for index, (label, _) in enumerate(columns):
        draw.text((index * tile + 12, 48), label, fill=(240, 242, 245), font=font)
    for row_index, row in enumerate(rows):
        y = title + row_index * (tile + footer)
        for column, (_, key) in enumerate(columns):
            image = Image.open(row[key]).convert("RGB")
            image.thumbnail((tile, tile), Image.Resampling.LANCZOS)
            cell = Image.new("RGB", (tile, tile), (0, 0, 0))
            cell.paste(image, ((tile - image.width) // 2, (tile - image.height) // 2))
            canvas.paste(cell, (column * tile, y))
        draw.text((12, y + tile + 12), row["footer"], fill=(218, 221, 225), font=font)
    canvas.save(path)


def main() -> None:
    args = parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    alignment = json.loads(args.alignment_summary.read_text())
    geometry_rows = []
    material_rows = []
    voxel_rows = []
    summary = []

    for row in alignment:
        authored_row = dict(row)
        rank = int(row["rank"])
        scene_output = args.output / f"{rank:02d}"
        scene_output.mkdir(parents=True, exist_ok=True)
        ply_path = (
            args.reference_volume_root / f"{rank:02d}" / "scene-ply-exact-v7.fptvox"
            if args.reference_volume_root
            else Path(row["reference_volume"])
        )
        v7_path = Path(row["v7_volume"])
        ply_volume = volume_arrays(ply_path)
        v7_volume = volume_arrays(v7_path)
        camera_candidates = []
        if args.camera_mode == "fit-volume":
            camera_candidates = fit_volume_camera_candidates(
                row, volume_header(ply_path), [ply_volume, v7_volume]
            )
            row = camera_candidates[0]
        forward = camera_basis(row)[0].astype(np.float32)
        flip_y = bool(row.get("legacy_coordinate_system", False))
        flip_x = args.native_capture_orientation == "mirror-x"

        ply_visibility = render_visibility(args, row, ply_path, "exact", scene_output / "ply-exact")
        roll = float(row.get("roll", 0.0))
        ply = visibility_images(
            ply_visibility, ply_volume, forward, row["fov"], roll, flip_y, flip_x
        )
        probe_count = 0
        fit_best_coverage = float(ply["hit"].mean())
        if camera_candidates and fit_best_coverage < args.minimum_exact_coverage:
            best = (float(ply["hit"].mean()), row, ply_visibility, ply)
            for index, candidate in enumerate(camera_candidates[1:], start=1):
                probe_count += 1
                candidate_forward = camera_basis(candidate)[0].astype(np.float32)
                candidate_visibility = render_visibility(
                    args,
                    candidate,
                    ply_path,
                    "exact",
                    scene_output / f"ply-probe-{index:02d}",
                )
                candidate_ply = visibility_images(
                    candidate_visibility,
                    ply_volume,
                    candidate_forward,
                    candidate["fov"],
                    float(candidate.get("roll", 0.0)),
                    flip_y,
                    flip_x,
                )
                coverage = float(candidate_ply["hit"].mean())
                if coverage > best[0]:
                    best = (coverage, candidate, candidate_visibility, candidate_ply)
                if coverage >= 0.15:
                    break
            fit_best_coverage, row, ply_visibility, ply = best
            row["comparison_direction_mode"] = "visibility-probed-direction"
            forward = camera_basis(row)[0].astype(np.float32)
            roll = float(row.get("roll", 0.0))
            for probe in scene_output.glob("ply-probe-*"):
                shutil.rmtree(probe)
        if camera_candidates and fit_best_coverage < args.minimum_exact_coverage:
            row = authored_row
            row["comparison_camera_mode"] = "authored"
            row["comparison_direction_mode"] = "authored-under-resolved-fallback"
            row["comparison_surface_status"] = "under-resolved"
            row["comparison_fit_best_coverage"] = fit_best_coverage
            forward = camera_basis(row)[0].astype(np.float32)
            roll = float(row.get("roll", 0.0))
            ply_visibility = render_visibility(
                args, row, ply_path, "exact", scene_output / "ply-authored"
            )
            ply = visibility_images(
                ply_visibility, ply_volume, forward, row["fov"], roll, flip_y, flip_x
            )
        else:
            row["comparison_surface_status"] = "resolved"
            row["comparison_fit_best_coverage"] = fit_best_coverage
        row["comparison_probe_count"] = probe_count
        row["comparison_visible_coverage"] = float(ply["hit"].mean())

        v7_visibility = render_visibility(args, row, v7_path, "exact", scene_output / "v7-exact")
        voxel_visibility = render_visibility(args, row, v7_path, "voxel", scene_output / "v7-voxel")
        v7 = visibility_images(
            v7_visibility, v7_volume, forward, row["fov"], roll, flip_y, flip_x
        )
        voxel = visibility_images(
            voxel_visibility, v7_volume, forward, row["fov"], roll, flip_y, flip_x
        )
        fpt_structural = render_fpt_geometry(args, row, scene_output / "fpt-geometry")
        fpt = load_float_image(fpt_structural["image"])

        paths: dict[str, str] = {}
        images = {
            "fpt_geometry": fpt,
            "ply_white": ply["white"],
            "v7_white": v7["white"],
            "geometry_diff": difference(ply["white"], v7["white"]),
            "normal_diff": normal_difference(ply, v7),
            "fpt_ply_mask_diff": mask_difference(fpt_structural["hit"], ply["hit"]),
            "fpt_v7_mask_diff": mask_difference(fpt_structural["hit"], v7["hit"]),
            "ply_material": ply["lit"],
            "v7_material": v7["lit"],
            "material_diff": difference(ply["lit"], v7["lit"]),
            "ply_unlit": ply["unlit"],
            "v7_unlit": v7["unlit"],
            "v7_exact_material": v7["lit"],
            "v7_voxel_material": voxel["lit"],
            "v7_exact_white": v7["white"],
            "v7_voxel_white": voxel["white"],
        }
        for key, image in images.items():
            path = scene_output / f"{key}.png"
            save_image(image, path)
            paths[key] = path.as_posix()

        geometry_metrics = image_metrics(ply["white"], v7["white"])
        normal_angle_metrics = normal_metrics(ply, v7)
        fpt_ply_mask_metrics = mask_metrics(fpt_structural["hit"], ply["hit"])
        fpt_v7_mask_metrics = mask_metrics(fpt_structural["hit"], v7["hit"])
        fpt_ply_depth_metrics = depth_metrics(fpt_structural, ply)
        fpt_v7_depth_metrics = depth_metrics(fpt_structural, v7)
        ply_v7_depth_metrics = depth_metrics(ply, v7)
        material_metrics = image_metrics(ply["lit"], v7["lit"])
        voxel_metrics = image_metrics(v7["white"], voxel["white"])
        cell_metrics = cell_color_metrics(ply_volume, v7_volume)
        geometry_rows.append(
            {
                **paths,
                "footer": (
                    f"{rank:02d} {row['name']} | mask IoU FPT/PLY "
                    f"{fpt_ply_mask_metrics['visible_iou']:.3f}, FPT/V7 "
                    f"{fpt_v7_mask_metrics['visible_iou']:.3f}, PLY/V7 "
                    f"{geometry_metrics['visible_iou']:.3f} | depth MAE FPT/PLY "
                    f"{fpt_ply_depth_metrics.get('relative_mae_pct', float('nan')):.2f}%, "
                    f"FPT/V7 {fpt_v7_depth_metrics.get('relative_mae_pct', float('nan')):.2f}%, "
                    f"PLY/V7 {ply_v7_depth_metrics.get('relative_mae_pct', float('nan')):.2f}% | "
                    f"normal mean/p95 {normal_angle_metrics.get('mean_degrees', float('nan')):.1f}/"
                    f"{normal_angle_metrics.get('p95_degrees', float('nan')):.1f} deg"
                ),
            }
        )
        material_rows.append(
            {
                **paths,
                "footer": (
                    f"{rank:02d} {row['name']} | lit common MAE "
                    f"{metric(material_metrics['common_visible_mae_255'])}/255, cell RGB MAE "
                    f"{metric(cell_metrics.get('rgb_mae_255'))}/255"
                ),
            }
        )
        voxel_rows.append(
            {
                **paths,
                "footer": (
                    f"{rank:02d} {row['name']} | exact surface vs occupied cubes, visible IoU "
                    f"{voxel_metrics['visible_iou']:.3f} | "
                    f"{row.get('comparison_surface_status', 'resolved')}"
                ),
            }
        )
        summary.append(
            {
                "rank": rank,
                "name": row["name"],
                "reference_volume": ply_path.as_posix(),
                "camera": {
                    "mode": row.get("comparison_camera_mode", "authored"),
                    "direction_mode": row.get("comparison_direction_mode", "authored-direction"),
                    "position": row["world_camera"],
                    "target": row.get("comparison_target"),
                    "yaw": row["yaw"],
                    "pitch": row["pitch"],
                    "roll": row.get("roll", 0.0),
                    "fov": row["fov"],
                    "naadf_fov": naadf_fov_degrees(float(row["fov"])),
                    "naadf_pixel_offset": [-0.5, -0.5],
                    "probe_count": row.get("comparison_probe_count", 0),
                    "visible_coverage": row.get("comparison_visible_coverage"),
                    "fit_best_coverage": row.get("comparison_fit_best_coverage"),
                    "native_capture_orientation": args.native_capture_orientation,
                    "native_capture_flip_x": flip_x,
                },
                "exact_surface_status": row.get("comparison_surface_status", "resolved"),
                "geometry": geometry_metrics,
                "normal_angle": normal_angle_metrics,
                "fpt_vs_ply_mask": fpt_ply_mask_metrics,
                "fpt_vs_v7_mask": fpt_v7_mask_metrics,
                "fpt_vs_ply_depth": fpt_ply_depth_metrics,
                "fpt_vs_v7_depth": fpt_v7_depth_metrics,
                "ply_vs_v7_depth": ply_v7_depth_metrics,
                "material_image": material_metrics,
                "material_cells": cell_metrics,
                "voxel_appearance": voxel_metrics,
                "images": paths,
            }
        )
        print(f"[{rank:02d}] geometry IoU {geometry_metrics['visible_iou']:.3f}; "
              f"material cell MAE {cell_metrics.get('rgb_mae_255', float('nan')):.2f}/255")

    make_sheet(
        geometry_rows,
        [
            ("FPT continuous - white diffuse", "fpt_geometry"),
            ("Mandel PLY exact -> NAADF - white diffuse", "ply_white"),
            ("Direct V7 -> NAADF - white diffuse", "v7_white"),
            ("FPT vs PLY mask: red miss, blue extra", "fpt_ply_mask_diff"),
            ("FPT vs V7 mask: red miss, blue extra", "fpt_v7_mask_diff"),
            ("PLY vs V7 shade diff (4x)", "geometry_diff"),
            ("PLY vs V7 normal angle", "normal_diff"),
        ],
        args.output / "geometry-parity.png",
        "One white material; camera-relative direct diffuse only; no AO, specular, or path tracing",
    )
    make_sheet(
        material_rows,
        [
            ("Mandel PLY exact colour - unlit", "ply_unlit"),
            ("Direct V7 colour - unlit", "v7_unlit"),
            ("Mandel PLY exact - direct diffuse", "ply_material"),
            ("Direct V7 - direct diffuse", "v7_material"),
            ("Lit colour diff (4x)", "material_diff"),
        ],
        args.output / "material-parity.png",
        "Authoritative PLY RGB versus generated V7 RGB under an identical deterministic light",
    )
    make_sheet(
        voxel_rows,
        [
            ("V7 exact triangles - colour", "v7_exact_material"),
            ("V7 occupied cubes - colour", "v7_voxel_material"),
            ("V7 exact triangles - white", "v7_exact_white"),
            ("V7 occupied cubes - white", "v7_voxel_white"),
        ],
        args.output / "voxel-appearance.png",
        "Same V7 occupancy/material data; exact sub-voxel surfaces versus explicit cube faces",
    )
    (args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(args.output)


if __name__ == "__main__":
    main()
