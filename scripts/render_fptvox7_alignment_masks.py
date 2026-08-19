#!/usr/bin/env python3
"""Render raw hit masks and visualise first-three alignment errors."""

from __future__ import annotations

import importlib.util
import argparse
import json
import struct
import subprocess
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont


ROOT = Path(__file__).resolve().parents[1]
HISTORICAL = ROOT / "reports/fpt-mandel-authoritative-50-r192-m384-20260812"
ALIGNED = ROOT / "reports/fptvox7-first3-aligned-final/summary.json"
OUTPUT = ROOT / "reports/fptvox7-first3-mask-diagnosis"
NAADF = Path(
    "/Users/jordantotty/Desktop/vox/metal-voxel-naadf-pathtracer/"
    "build/MetalVoxel.app/Contents/MacOS/MetalVoxel"
)
VISIBILITY_RECORD = struct.Struct("<f7I")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--aligned", type=Path, default=ALIGNED)
    parser.add_argument("--output", type=Path, default=OUTPUT)
    return parser.parse_args()


def alignment_module():
    path = ROOT / "scripts/run_fptvox7_first3_alignment.py"
    spec = importlib.util.spec_from_file_location("alignment", path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader
    spec.loader.exec_module(module)
    return module


def run(command: list[str], stdout_path: Path, stderr_path: Path) -> None:
    completed = subprocess.run(command, text=True, capture_output=True, timeout=1800)
    stdout_path.write_text(completed.stdout)
    stderr_path.write_text(completed.stderr)
    if completed.returncode:
        raise RuntimeError(completed.stderr.strip() or completed.stdout.strip())


def visibility_mask(base: Path) -> np.ndarray:
    manifest = json.loads(Path(str(base) + ".json").read_text())
    width = int(manifest["width"])
    height = int(manifest["height"])
    data = (base.parent / manifest["svdag_visibility"]).read_bytes()
    mask = np.zeros(width * height, dtype=bool)
    for index in range(width * height):
        depth, material, *_ = VISIBILITY_RECORD.unpack_from(
            data, index * VISIBILITY_RECORD.size
        )
        mask[index] = material != 0 and np.isfinite(depth)
    return mask.reshape(height, width)


def render_visibility(row: dict, kind: str, module, output_root: Path) -> np.ndarray:
    output = output_root / f"{row['rank']:02d}" / kind
    output.mkdir(parents=True, exist_ok=True)
    base = output / "visibility"
    volume = Path(row["reference_volume"] if kind == "ply" else row["v7_volume"])
    header = module.volume_header(volume)
    grid = module.grid_camera(
        row_as_camera(row),
        header["resolution"][0],
        list(header["bounds_min"]),
        list(header["bounds_max"]),
    )
    camera = ",".join(str(value) for value in [*grid, row["yaw"], row["pitch"]])
    command = [
        NAADF.as_posix(), "--mode", "greedy2d", "--fptvox", volume.as_posix(),
        "--size", "512x512", "--camera", camera,
        "--camera-fov", str(row["fov"]), "--camera-roll", str(row["roll"]),
        "--gpu-renderer", "naadf", "--gpu-naadf-mode", "aadf",
        "--gpu-naadf-build", "cpu", "--gpu-naadf-primary-layout", "direct16",
        "--gpu-naadf-path-tracing", "fixed",
        "--gpu-naadf-path-samples-per-frame", "1",
        "--gpu-naadf-path-max-samples", "1",
        "--gpu-naadf-path-bounces", "1",
        "--benchmark", "--offscreen",
        "--frames", "1", "--warmup", "0", "--gpu-visibility-dump", base.as_posix(),
        "--report", (output / "render").as_posix(),
    ]
    run(command, output / "stdout.log", output / "stderr.log")
    return visibility_mask(base)


def row_as_camera(row: dict) -> dict:
    return {
        "world_position": row["world_camera"],
        "bounds_min": row["reference_bounds_min"],
        "bounds_max": row["reference_bounds_max"],
    }


def image_mask(path: Path) -> np.ndarray:
    image = np.asarray(Image.open(path).convert("L"), dtype=np.uint8)
    return image >= 128


def normalize_image_coordinates(mask: np.ndarray, row: dict) -> np.ndarray:
    return np.flipud(mask) if row.get("legacy_coordinate_system", False) else mask


def bounds(mask: np.ndarray) -> list[int | None]:
    points = np.argwhere(mask)
    if not len(points):
        return [None, None, None, None]
    minimum = points.min(axis=0)
    maximum = points.max(axis=0)
    return [int(minimum[1]), int(minimum[0]), int(maximum[1]), int(maximum[0])]


def centroid(mask: np.ndarray) -> list[float | None]:
    points = np.argwhere(mask)
    if not len(points):
        return [None, None]
    center = points.mean(axis=0)
    return [float(center[1]), float(center[0])]


def metrics(reference: np.ndarray, candidate: np.ndarray) -> dict:
    intersection = reference & candidate
    union = reference | candidate
    result = {
        "iou": float(intersection.sum() / max(1, union.sum())),
        "reference_pixels": int(reference.sum()),
        "candidate_pixels": int(candidate.sum()),
        "area_ratio": float(candidate.sum() / max(1, reference.sum())),
        "reference_centroid_xy": centroid(reference),
        "candidate_centroid_xy": centroid(candidate),
        "reference_bounds_xyxy": bounds(reference),
        "candidate_bounds_xyxy": bounds(candidate),
    }
    best = best_rigid_translation(reference, candidate)
    result["best_rigid_translation_xy"] = [best[1], best[2]]
    result["best_rigid_translation_iou"] = best[0]
    result["rigid_translation_iou_gain"] = best[0] - result["iou"]
    return result


def translated(mask: np.ndarray, dx: int, dy: int) -> np.ndarray:
    height, width = mask.shape
    output = np.zeros_like(mask)
    source_x0 = max(0, -dx)
    source_y0 = max(0, -dy)
    source_x1 = min(width, width - dx)
    source_y1 = min(height, height - dy)
    target_x0 = source_x0 + dx
    target_y0 = source_y0 + dy
    target_x1 = source_x1 + dx
    target_y1 = source_y1 + dy
    output[target_y0:target_y1, target_x0:target_x1] = mask[
        source_y0:source_y1, source_x0:source_x1
    ]
    return output


def best_rigid_translation(
    reference: np.ndarray, candidate: np.ndarray, radius: int = 12
) -> tuple[float, int, int]:
    best = (-1.0, 0, 0)
    for dy in range(-radius, radius + 1):
        for dx in range(-radius, radius + 1):
            shifted = translated(candidate, dx, dy)
            intersection = reference & shifted
            union = reference | shifted
            score = float(intersection.sum() / max(1, union.sum()))
            if score > best[0]:
                best = (score, dx, dy)
    return best


def mask_image(mask: np.ndarray) -> Image.Image:
    return Image.fromarray(np.where(mask, 255, 0).astype(np.uint8), mode="L").convert("RGB")


def xor_image(reference: np.ndarray, candidate: np.ndarray) -> Image.Image:
    output = np.zeros((*reference.shape, 3), dtype=np.uint8)
    output[reference & candidate] = (255, 255, 255)
    output[reference & ~candidate] = (255, 55, 55)
    output[~reference & candidate] = (50, 180, 255)
    return Image.fromarray(output, mode="RGB")


def make_sheet(rows: list[dict], masks: dict[int, dict[str, np.ndarray]], path: Path) -> None:
    tile = 320
    header = 70
    footer = 60
    columns = ["FPT hit mask", "PLY NAADF", "V7 NAADF", "FPT vs V7", "PLY vs V7"]
    canvas = Image.new("RGB", (tile * len(columns), header + len(rows) * (tile + footer)), (25, 27, 30))
    draw = ImageDraw.Draw(canvas)
    font = ImageFont.load_default()
    for index, title in enumerate(columns):
        draw.text((index * tile + 12, 25), title, fill="white", font=font)
    draw.text(
        (12, 45),
        "XOR legend: white = agreement, red = reference only, blue = candidate only",
        fill=(205, 208, 212),
        font=font,
    )
    for row_index, row in enumerate(rows):
        y = header + row_index * (tile + footer)
        scene_masks = masks[row["rank"]]
        images = [
            mask_image(scene_masks["fpt"]),
            mask_image(scene_masks["ply"]),
            mask_image(scene_masks["v7"]),
            xor_image(scene_masks["fpt"], scene_masks["v7"]),
            xor_image(scene_masks["ply"], scene_masks["v7"]),
        ]
        for column, image in enumerate(images):
            image = image.resize((tile, tile), Image.Resampling.NEAREST)
            canvas.paste(image, (column * tile, y))
        fpt = row["mask_metrics"]["fpt_vs_v7"]
        ply = row["mask_metrics"]["ply_vs_v7"]
        text = (
            f"{row['rank']:02d} {row['name']} | FPT/V7 IoU {fpt['iou']:.3f}, area {fpt['area_ratio']:.3f} | "
            f"PLY/V7 IoU {ply['iou']:.3f}, area {ply['area_ratio']:.3f} | "
            f"best shift FPT/V7 {fpt['best_rigid_translation_xy']}"
        )
        draw.text((12, y + tile + 16), text, fill=(225, 228, 232), font=font)
    canvas.save(path)


def main() -> None:
    args = parse_args()
    module = alignment_module()
    rows = json.loads(args.aligned.read_text())
    args.output.mkdir(parents=True, exist_ok=True)
    masks: dict[int, dict[str, np.ndarray]] = {}
    for row in rows:
        rank = row["rank"]
        fpt = normalize_image_coordinates(
            image_mask(
                OUTPUT / "fpt-corrected" / f"{rank:02d}" / f"{row['name']}.png"
            ),
            row,
        )
        ply = normalize_image_coordinates(
            render_visibility(row, "ply", module, args.output), row
        )
        v7 = normalize_image_coordinates(
            render_visibility(row, "v7", module, args.output), row
        )
        masks[rank] = {"fpt": fpt, "ply": ply, "v7": v7}
        row["mask_metrics"] = {
            "fpt_vs_ply": metrics(fpt, ply),
            "fpt_vs_v7": metrics(fpt, v7),
            "ply_vs_v7": metrics(ply, v7),
        }
        for name, mask in masks[rank].items():
            output = args.output / f"{rank:02d}" / f"{name}-mask.png"
            output.parent.mkdir(parents=True, exist_ok=True)
            mask_image(mask).save(output)
    (args.output / "summary.json").write_text(json.dumps(rows, indent=2) + "\n")
    make_sheet(rows, masks, args.output / "first3-hit-mask-alignment.png")
    print(args.output / "first3-hit-mask-alignment.png")


if __name__ == "__main__":
    main()
