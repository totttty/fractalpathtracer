#!/usr/bin/env python3
"""Score and visualise one Mandelbulber/Metal-FPT diffuse-normal batch."""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
from statistics import fmean

from PIL import Image, ImageDraw, ImageFont


def global_ssim(left: list[float], right: list[float]) -> float:
    if len(left) != len(right) or not left:
        raise ValueError("SSIM inputs must be non-empty and equal-sized")
    mean_left = fmean(left)
    mean_right = fmean(right)
    denominator = max(len(left) - 1, 1)
    variance_left = sum((value - mean_left) ** 2 for value in left) / denominator
    variance_right = sum((value - mean_right) ** 2 for value in right) / denominator
    covariance = sum(
        (a - mean_left) * (b - mean_right) for a, b in zip(left, right)
    ) / denominator
    c1 = 0.01**2
    c2 = 0.03**2
    return (
        (2.0 * mean_left * mean_right + c1) * (2.0 * covariance + c2)
        / (
            (mean_left**2 + mean_right**2 + c1)
            * (variance_left + variance_right + c2)
        )
    )


def upstream_diffuse(normal_path: Path, zbuffer_path: Path) -> Image.Image:
    normal = Image.open(normal_path).convert("RGB")
    zbuffer = Image.open(zbuffer_path).convert("I")
    if normal.size != zbuffer.size:
        raise ValueError(f"AOV size mismatch: {normal_path} and {zbuffer_path}")
    z_max = max(zbuffer.getdata())
    output = Image.new("L", normal.size)
    shades: list[int] = []
    for rgb, depth in zip(normal.getdata(), zbuffer.getdata()):
        default_miss_normal = abs(rgb[0] - 128) <= 1 and abs(rgb[1] - 128) <= 1 and rgb[2] >= 254
        if depth >= z_max and default_miss_normal:
            shades.append(0)
            continue
        diffuse = max(2.0 * (rgb[2] / 255.0) - 1.0, 0.0)
        shades.append(round(255.0 * (0.12 + 0.88 * diffuse)))
    output.putdata(shades)
    return output


def compare_images(upstream: Image.Image, fpt: Image.Image) -> dict[str, float]:
    left = [value / 255.0 for value in upstream.convert("L").getdata()]
    right = [value / 255.0 for value in fpt.convert("L").getdata()]
    if upstream.size != fpt.size:
        raise ValueError(f"comparison size mismatch: {upstream.size} versus {fpt.size}")
    mask_left = [value > 0.0 for value in left]
    mask_right = [value > 0.0 for value in right]
    intersection = sum(a and b for a, b in zip(mask_left, mask_right))
    union = sum(a or b for a, b in zip(mask_left, mask_right))
    intersecting_error = [
        abs(a - b)
        for a, b, hit_a, hit_b in zip(left, right, mask_left, mask_right)
        if hit_a and hit_b
    ]
    return {
        "mae": fmean(abs(a - b) for a, b in zip(left, right)),
        "hit_normal_mae": fmean(intersecting_error) if intersecting_error else 0.0,
        "global_ssim": global_ssim(left, right),
        "silhouette_iou": intersection / union if union else 1.0,
        "upstream_hit_fraction": fmean(mask_left),
        "fpt_hit_fraction": fmean(mask_right),
    }


def font(size: int) -> ImageFont.ImageFont:
    for candidate in (
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/System/Library/Fonts/SFNS.ttf",
        "/Library/Fonts/Arial.ttf",
    ):
        if Path(candidate).is_file():
            return ImageFont.truetype(candidate, size)
    return ImageFont.load_default()


def comparison_tile(record: dict[str, object], width: int, height: int) -> Image.Image:
    left = Image.open(str(record["upstream_diffuse"])).convert("L")
    right = Image.open(str(record["fpt_image"])).convert("L")
    left = left.resize((width, height), Image.Resampling.LANCZOS).convert("RGB")
    right = right.resize((width, height), Image.Resampling.LANCZOS).convert("RGB")
    label_height = 42
    tile = Image.new("RGB", (width * 2, height + label_height), (20, 20, 20))
    tile.paste(left, (0, 0))
    tile.paste(right, (width, 0))
    draw = ImageDraw.Draw(tile)
    draw.line((width, 0, width, height), fill=(100, 100, 100), width=1)
    metrics = record["metrics"]
    assert isinstance(metrics, dict)
    scene = f'{int(record["index"]):04d} {record["path"]}'
    if len(scene) > 58:
        scene = scene[:55] + "..."
    draw.text((6, height + 3), scene, fill="white", font=font(14))
    summary = (
        f'MAE {metrics["mae"]:.3f}  hit {metrics["hit_normal_mae"]:.3f}  '
        f'IoU {metrics["silhouette_iou"]:.3f}  SSIM {metrics["global_ssim"]:.3f}'
    )
    draw.text((6, height + 21), summary, fill=(205, 205, 205), font=font(12))
    return tile


def contact_sheet(
    records: list[dict[str, object]], output: Path, columns: int, width: int, height: int
) -> None:
    tiles = [comparison_tile(record, width, height) for record in records]
    rows = math.ceil(len(tiles) / columns)
    sheet = Image.new(
        "RGB", (columns * width * 2, rows * (height + 42)), (15, 15, 15)
    )
    for index, tile in enumerate(tiles):
        sheet.paste(tile, ((index % columns) * tile.width, (index // columns) * tile.height))
    output.parent.mkdir(parents=True, exist_ok=True)
    sheet.save(output, format="PNG", optimize=True)


def locate_fpt_image(batch_dir: Path, scene_id: str) -> Path | None:
    candidates = sorted((batch_dir / "fpt" / scene_id).glob("*.png"))
    if not candidates:
        return None
    if len(candidates) != 1:
        raise ValueError(f"expected one FPT image for {scene_id}, found {len(candidates)}")
    return candidates[0]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("batch_dir", type=Path)
    parser.add_argument("--worst", type=int, default=20)
    args = parser.parse_args()
    batch_dir = args.batch_dir.resolve()
    scenes = json.loads((batch_dir / "scenes.json").read_text(encoding="utf-8"))
    manifest = json.loads(
        (batch_dir / "upstream" / "manifest.json").read_text(encoding="utf-8")
    )
    upstream_by_id = {record["id"]: record for record in manifest["scenes"]}
    diffuse_dir = batch_dir / "upstream-diffuse"
    diffuse_dir.mkdir(parents=True, exist_ok=True)
    records: list[dict[str, object]] = []
    missing: list[dict[str, object]] = []
    for scene in scenes:
        upstream_record = upstream_by_id[scene["id"]]
        output = Path(upstream_record["output"])
        normal = output.with_name(f"{output.stem}_normal.png")
        zbuffer = output.with_name(f"{output.stem}_zbuffer.png")
        upstream = upstream_diffuse(normal, zbuffer)
        upstream_path = diffuse_dir / f'{scene["index"]:04d}-{scene["id"]}.png'
        upstream.save(upstream_path, format="PNG", optimize=True)
        fpt_path = locate_fpt_image(batch_dir, scene["id"])
        if fpt_path is None:
            missing.append(
                {
                    "index": scene["index"],
                    "id": scene["id"],
                    "path": scene["path"],
                    "reason": "missing Metal-FPT image",
                }
            )
            continue
        metrics = compare_images(upstream, Image.open(fpt_path))
        records.append(
            {
                "index": scene["index"],
                "id": scene["id"],
                "path": scene["path"],
                "upstream_diffuse": str(upstream_path),
                "fpt_image": str(fpt_path),
                "metrics": metrics,
            }
        )

    ranked = sorted(
        records,
        key=lambda record: (
            record["metrics"]["mae"],
            1.0 - record["metrics"]["silhouette_iou"],
        ),
        reverse=True,
    )
    report = {
        "schema_version": 1,
        "catalog_scenes": len(scenes),
        "scenes": len(records),
        "missing_scenes": missing,
        "mean_mae": fmean(record["metrics"]["mae"] for record in records),
        "mean_hit_normal_mae": fmean(
            record["metrics"]["hit_normal_mae"] for record in records
        ),
        "mean_global_ssim": fmean(
            record["metrics"]["global_ssim"] for record in records
        ),
        "mean_silhouette_iou": fmean(
            record["metrics"]["silhouette_iou"] for record in records
        ),
        "records": records,
        "ranked_scene_ids": [record["id"] for record in ranked],
    }
    (batch_dir / "metrics.json").write_text(
        json.dumps(report, indent=2) + "\n", encoding="utf-8"
    )
    contact_sheet(records, batch_dir / "contact-sheet.png", 5, 240, 135)
    contact_sheet(
        ranked[: args.worst], batch_dir / "worst-contact-sheet.png", 4, 240, 135
    )
    ranking_lines: list[str] = []
    for record in ranked:
        metrics = record["metrics"]
        ranking_lines.append(
            f'{record["index"]:04d}\t{metrics["mae"]:.6f}\t'
            f'{metrics["hit_normal_mae"]:.6f}\t{metrics["silhouette_iou"]:.6f}\t'
            f'{metrics["global_ssim"]:.6f}\t{record["path"]}'
        )
    ranking_text = "\n".join(ranking_lines) + "\n"
    (batch_dir / "ranking.tsv").write_text(ranking_text, encoding="utf-8")
    print(ranking_text, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
