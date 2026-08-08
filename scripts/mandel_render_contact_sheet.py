#!/usr/bin/env python3
"""Create a labelled contact sheet from a Mandel corpus benchmark report."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("report", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--columns", type=int, default=3)
    parser.add_argument("--tile-width", type=int, default=400)
    parser.add_argument("--label-height", type=int, default=64)
    parser.add_argument("--jpeg-quality", type=int, default=90)
    return parser.parse_args()


def font(size: int) -> ImageFont.ImageFont:
    candidates = (
        Path("/System/Library/Fonts/SFNS.ttf"),
        Path("/System/Library/Fonts/Helvetica.ttc"),
        Path("/Library/Fonts/Arial.ttf"),
    )
    for candidate in candidates:
        if candidate.is_file():
            return ImageFont.truetype(str(candidate), size=size)
    return ImageFont.load_default()


def main() -> int:
    args = parse_args()
    report = json.loads(args.report.read_text(encoding="utf-8"))
    entries = [entry for entry in report["entries"] if entry["status"] == "ok"]
    if not entries:
        raise ValueError("report contains no successful render entries")
    first = Image.open(entries[0]["output"])
    aspect = first.height / first.width
    image_height = round(args.tile_width * aspect)
    tile_height = image_height + args.label_height
    columns = max(1, min(args.columns, len(entries)))
    rows = (len(entries) + columns - 1) // columns
    sheet = Image.new("RGB", (columns * args.tile_width, rows * tile_height), (18, 18, 18))
    title_font = font(18)
    detail_font = font(16)
    for index, entry in enumerate(entries):
        image = Image.open(entry["output"]).convert("RGB")
        image.thumbnail((args.tile_width, image_height), Image.Resampling.LANCZOS)
        x = (index % columns) * args.tile_width
        y = (index // columns) * tile_height
        sheet.paste(image, (x + (args.tile_width - image.width) // 2, y))
        draw = ImageDraw.Draw(sheet)
        name = Path(entry["path"]).stem
        if len(name) > 42:
            name = name[:39] + "..."
        draw.text((x + 10, y + image_height + 7), name, fill=(245, 245, 245), font=title_font)
        gpu_ms = float(entry["gpu_ms"])
        draw.text(
            (x + 10, y + image_height + 34),
            f"GPU {gpu_ms:,.2f} ms  |  {1000.0 / gpu_ms:.2f} FPS",
            fill=(190, 205, 220),
            font=detail_font,
        )
    args.out.parent.mkdir(parents=True, exist_ok=True)
    save_args = {"quality": args.jpeg_quality, "optimize": True} if args.out.suffix.lower() in {".jpg", ".jpeg"} else {}
    sheet.save(args.out, **save_args)
    print(f"wrote {args.out} ({sheet.width}x{sheet.height})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
