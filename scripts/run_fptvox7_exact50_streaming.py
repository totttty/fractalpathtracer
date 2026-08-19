#!/usr/bin/env python3
"""Run representation-matched FPTVOX7 parity without retaining huge volumes."""

from __future__ import annotations

import argparse
import csv
import json
import math
import shutil
import statistics
import struct
import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = (
    ROOT
    / "reports/fptvox7-ranked50-latest-naadf-300px-32spp-20260815/summary.json"
)
DEFAULT_OUTPUT = ROOT / "reports/fptvox7-ranked50-exact-v7-r192-m384-20260816"
DEFAULT_MANDEL_ROOT = Path("/Volumes/Ventura/Projects/mandelbulber2/mandelbulber2")
DEFAULT_MANDEL_BIN = Path(
    "/Volumes/Ventura/Projects/mandelbulber2/build-opencl/"
    "mandelbulber2.app/Contents/MacOS/mandelbulber2"
)
DEFAULT_NAADF = Path(
    "/Users/jordantotty/Desktop/vox/metal-voxel-naadf-pathtracer/"
    "build/MetalVoxel.app/Contents/MacOS/MetalVoxel"
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--ranks", help="comma-separated subset; default is all manifest rows")
    parser.add_argument("--voxel-resolution", type=int, default=192)
    parser.add_argument("--mesh-resolution", type=int, default=384)
    parser.add_argument(
        "--empty-reference-retry-max",
        type=int,
        default=0,
        help=(
            "double both sampling grids when Mandelbulber's reference mesh is empty, "
            "up to this resolution; 0 disables retries"
        ),
    )
    parser.add_argument(
        "--adaptive-parity-threshold",
        type=float,
        default=0.0,
        help=(
            "double both sampling grids while the conservative structural score is below "
            "this threshold; the score is the minimum of occupied-cell and per-cell "
            "triangle-area overlap, "
            "bounded by --empty-reference-retry-max; 0 disables parity retries"
        ),
    )
    parser.add_argument("--image-size", type=int, default=300)
    parser.add_argument("--fpt", type=Path, default=ROOT / "target/release/fpt-metal")
    parser.add_argument("--mandelbulber-root", type=Path, default=DEFAULT_MANDEL_ROOT)
    parser.add_argument("--mandelbulber-bin", type=Path, default=DEFAULT_MANDEL_BIN)
    parser.add_argument("--naadf", type=Path, default=DEFAULT_NAADF)
    parser.add_argument("--force", action="store_true")
    parser.add_argument("--keep-volumes", action="store_true")
    parser.add_argument(
        "--cell-only",
        action="store_true",
        help="compare occupied FPTVOX7 cells without rendering parity images",
    )
    args = parser.parse_args()
    if not 0.0 <= args.adaptive_parity_threshold <= 1.0:
        parser.error("--adaptive-parity-threshold must be between 0 and 1")
    if args.empty_reference_retry_max and args.empty_reference_retry_max < args.mesh_resolution:
        parser.error("--empty-reference-retry-max must be >= --mesh-resolution")
    return args


def run(command: list[str], cwd: Path, stdout: Path, stderr: Path) -> dict:
    result = subprocess.run(command, cwd=cwd, text=True, capture_output=True, timeout=1800)
    stdout.write_text(result.stdout)
    stderr.write_text(result.stderr)
    if result.returncode:
        raise RuntimeError(result.stderr.strip() or result.stdout.strip())
    return json.loads(result.stdout)


def cells(path: Path) -> set[tuple[int, int, int]]:
    data = path.read_bytes()
    if len(data) < 96 or data[:8] != b"FPTVOX7\0":
        raise RuntimeError(f"expected FPTVOX7 exact payload: {path}")
    header_size, version = struct.unpack_from("<II", data, 8)
    count = struct.unpack_from("<Q", data, 56)[0]
    if version != 7 or header_size != 96:
        raise RuntimeError(f"unexpected FPTVOX layout v{version}/{header_size}: {path}")
    expected_records = header_size + count * 32
    if len(data) < expected_records:
        raise RuntimeError(f"truncated FPTVOX7 records: {path}")
    return {
        struct.unpack_from("<III", data, header_size + index * 32)
        for index in range(count)
    }


def cell_metrics(reference: Path, candidate: Path) -> dict:
    reference_cells = cells(reference)
    candidate_cells = cells(candidate)
    intersection = len(reference_cells & candidate_cells)
    union = len(reference_cells | candidate_cells)
    return {
        "representation": "fptvox7-cell-clipped-triangles",
        "reference_cells": len(reference_cells),
        "candidate_cells": len(candidate_cells),
        "intersection": intersection,
        "union": union,
        "cell_iou": intersection / union if union else 1.0,
        "reference_only": len(reference_cells - candidate_cells),
        "candidate_only": len(candidate_cells - reference_cells),
    }


def triangle_payload(path: Path) -> dict:
    data = path.read_bytes()
    if len(data) < 96 or data[:8] != b"FPTVOX7\0":
        raise RuntimeError(f"expected FPTVOX7 triangle payload: {path}")
    header_size, version = struct.unpack_from("<II", data, 8)
    cell_count = struct.unpack_from("<Q", data, 56)[0]
    cell_record_size = struct.unpack_from("<I", data, 76)[0]
    triangle_count = struct.unpack_from("<Q", data, 80)[0]
    triangle_record_size = struct.unpack_from("<I", data, 88)[0]
    if (version, header_size, cell_record_size, triangle_record_size) != (7, 96, 32, 12):
        raise RuntimeError(f"unexpected FPTVOX7 layout: {path}")
    triangle_offset = header_size + cell_count * cell_record_size
    expected = triangle_offset + triangle_count * triangle_record_size
    if len(data) != expected:
        raise RuntimeError(f"invalid FPTVOX7 byte length: {path}")

    def vertex(word: int) -> tuple[float, float, float]:
        return (
            float(word & 1023) / 1023.0,
            float((word >> 10) & 1023) / 1023.0,
            float((word >> 20) & 1023) / 1023.0,
        )

    triangle_areas = []
    for index in range(triangle_count):
        words = struct.unpack_from("<III", data, triangle_offset + index * 12)
        a, b, c = map(vertex, words)
        ab = (b[0] - a[0], b[1] - a[1], b[2] - a[2])
        ac = (c[0] - a[0], c[1] - a[1], c[2] - a[2])
        cross = (
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        )
        triangle_areas.append(0.5 * math.sqrt(sum(value * value for value in cross)))
    area_prefix = [0.0]
    for area in triangle_areas:
        area_prefix.append(area_prefix[-1] + area)

    cells = {}
    for index in range(cell_count):
        offset = header_size + index * cell_record_size
        coordinate = struct.unpack_from("<III", data, offset)
        first, count = struct.unpack_from("<II", data, offset + 24)
        cells[coordinate] = {
            "triangles": count,
            "area": area_prefix[first + count] - area_prefix[first],
        }
    return {
        "triangle_count": int(triangle_count),
        "triangle_area": area_prefix[-1],
        "cells": cells,
    }


def surface_payload_metrics(reference: Path, candidate: Path) -> dict:
    reference_payload = triangle_payload(reference)
    candidate_payload = triangle_payload(candidate)
    coordinates = reference_payload["cells"].keys() | candidate_payload["cells"].keys()
    count_intersection = count_union = 0
    area_intersection = area_union = 0.0
    for coordinate in coordinates:
        reference_cell = reference_payload["cells"].get(coordinate, {})
        candidate_cell = candidate_payload["cells"].get(coordinate, {})
        reference_count = int(reference_cell.get("triangles", 0))
        candidate_count = int(candidate_cell.get("triangles", 0))
        reference_area = float(reference_cell.get("area", 0.0))
        candidate_area = float(candidate_cell.get("area", 0.0))
        count_intersection += min(reference_count, candidate_count)
        count_union += max(reference_count, candidate_count)
        area_intersection += min(reference_area, candidate_area)
        area_union += max(reference_area, candidate_area)
    reference_count = reference_payload["triangle_count"]
    candidate_count = candidate_payload["triangle_count"]
    reference_area = reference_payload["triangle_area"]
    candidate_area = candidate_payload["triangle_area"]
    count_iou = count_intersection / count_union
    area_iou = area_intersection / area_union
    return {
        "reference_triangles": reference_count,
        "candidate_triangles": candidate_count,
        "triangle_retention_ratio": candidate_count / reference_count,
        "triangle_count_iou_proxy": count_iou,
        "reference_triangle_area": reference_area,
        "candidate_triangle_area": candidate_area,
        "triangle_area_retention_ratio": candidate_area / reference_area,
        "triangle_area_iou_proxy": area_iou,
    }


def structural_score(cell: dict, surface: dict) -> float:
    """Tessellation-invariant surface score for representation-matched payloads."""
    return min(
        float(cell["cell_iou"]),
        float(surface["triangle_area_iou_proxy"]),
    )


def compact_render_artifacts(render_root: Path, rank: int) -> None:
    scene = render_root / f"{rank:02d}"
    for name in ("ply-exact", "v7-exact", "v7-voxel", "fpt-geometry"):
        path = scene / name
        if path.is_dir():
            shutil.rmtree(path)


def write_reports(output: Path, rows: list[dict]) -> None:
    rows = sorted(rows, key=lambda row: int(row["rank"]))
    for row in rows:
        if row.get("cell") and row.get("surface_payload"):
            row["structural_score"] = structural_score(
                row["cell"], row["surface_payload"]
            )
    (output / "summary.json").write_text(json.dumps(rows, indent=2) + "\n")
    fields = [
        "rank", "name", "status", "reference_backend", "cell_iou", "reference_cells", "candidate_cells",
        "requested_mesh_resolution", "effective_mesh_resolution", "sampling_retry_count",
        "structural_score", "reference_triangles", "candidate_triangles", "triangle_retention_ratio",
        "triangle_count_iou_proxy", "triangle_area_retention_ratio", "triangle_area_iou_proxy",
        "visible_iou", "visible_miss_pct", "visible_extra_pct", "normal_mean_degrees",
        "ply_v7_depth_mae_pct", "ply_v7_depth_p95_pct", "fpt_vs_ply_visible_iou",
        "fpt_vs_v7_visible_iou", "material_rgb_mae_255", "error",
    ]
    with (output / "summary.csv").open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fields)
        writer.writeheader()
        for row in rows:
            cell = row.get("cell", {})
            geometry = row.get("geometry", {})
            normal = row.get("normal_angle", {})
            material = row.get("material_cells", {})
            depth = row.get("ply_vs_v7_depth", {})
            surface = row.get("surface_payload", {})
            writer.writerow({
                "rank": row["rank"],
                "name": row["name"],
                "status": row["status"],
                "reference_backend": row.get("reference_backend", ""),
                "cell_iou": cell.get("cell_iou", ""),
                "reference_cells": cell.get("reference_cells", ""),
                "candidate_cells": cell.get("candidate_cells", ""),
                "requested_mesh_resolution": row.get("requested_mesh_resolution", ""),
                "effective_mesh_resolution": row.get("effective_mesh_resolution", ""),
                "sampling_retry_count": row.get("sampling_retry_count", ""),
                "structural_score": row.get("structural_score", ""),
                "reference_triangles": surface.get("reference_triangles", ""),
                "candidate_triangles": surface.get("candidate_triangles", ""),
                "triangle_retention_ratio": surface.get("triangle_retention_ratio", ""),
                "triangle_count_iou_proxy": surface.get("triangle_count_iou_proxy", ""),
                "triangle_area_retention_ratio": surface.get("triangle_area_retention_ratio", ""),
                "triangle_area_iou_proxy": surface.get("triangle_area_iou_proxy", ""),
                "visible_iou": geometry.get("visible_iou", ""),
                "visible_miss_pct": geometry.get("visible_miss_pct", ""),
                "visible_extra_pct": geometry.get("visible_extra_pct", ""),
                "normal_mean_degrees": normal.get("mean_degrees", ""),
                "ply_v7_depth_mae_pct": depth.get("relative_mae_pct", ""),
                "ply_v7_depth_p95_pct": depth.get("relative_p95_pct", ""),
                "fpt_vs_ply_visible_iou": row.get("fpt_vs_ply_mask", {}).get("visible_iou", ""),
                "fpt_vs_v7_visible_iou": row.get("fpt_vs_v7_mask", {}).get("visible_iou", ""),
                "material_rgb_mae_255": material.get("rgb_mae_255", ""),
                "error": row.get("error", ""),
            })
    write_contact_sheets(output, rows)
    write_voxel_appearance_sheets(output, rows)
    write_diagnosis(output, rows)


def fit(image: Image.Image, size: int) -> Image.Image:
    image = image.convert("RGB")
    image.thumbnail((size, size), Image.Resampling.LANCZOS)
    tile = Image.new("RGB", (size, size), (0, 0, 0))
    tile.paste(image, ((size - image.width) // 2, (size - image.height) // 2))
    return tile


def write_contact_sheets(output: Path, rows: list[dict]) -> None:
    completed = [row for row in rows if row.get("status") == "ok" and row.get("images")]
    columns = [
        ("FPT continuous", "fpt_geometry"),
        ("Mandel PLY V7 exact", "ply_white"),
        ("Direct V7 exact", "v7_white"),
        ("PLY vs V7 diff", "geometry_diff"),
    ]
    tile, header, footer, per_sheet = 300, 58, 48, 10
    font = ImageFont.load_default()
    for sheet_index, first in enumerate(range(0, len(completed), per_sheet), start=1):
        page = completed[first:first + per_sheet]
        canvas = Image.new(
            "RGB", (tile * len(columns), header + len(page) * (tile + footer)), (27, 29, 32)
        )
        draw = ImageDraw.Draw(canvas)
        for index, (label, _) in enumerate(columns):
            draw.text((index * tile + 10, 20), label, fill=(238, 240, 244), font=font)
        for row_index, row in enumerate(page):
            y = header + row_index * (tile + footer)
            for column, (_, key) in enumerate(columns):
                path = Path(row["images"][key])
                canvas.paste(fit(Image.open(path), tile), (column * tile, y))
            cell_iou = row["cell"]["cell_iou"]
            visible_iou = row["geometry"]["visible_iou"]
            surface_iou = row.get("structural_score")
            surface_text = "n/a" if surface_iou is None else f"{surface_iou:.3f}"
            text = (
                f"{int(row['rank']):02d} {row['name']} | cell {cell_iou:.3f} | "
                f"structural {surface_text} | visible {visible_iou:.3f}"
            )
            draw.text((10, y + tile + 12), text, fill=(215, 218, 223), font=font)
        canvas.save(output / f"contact-sheet-{sheet_index:02d}.png")


def write_voxel_appearance_sheets(output: Path, rows: list[dict]) -> None:
    completed = [row for row in rows if row.get("status") == "ok" and row.get("images")]
    columns = [
        ("FPT continuous", "fpt_geometry"),
        ("Direct V7 exact triangles", "v7_exact_white"),
        ("Direct V7 occupied cubes", "v7_voxel_white"),
    ]
    tile, header, footer, per_sheet = 300, 58, 48, 10
    font = ImageFont.load_default()
    for sheet_index, first in enumerate(range(0, len(completed), per_sheet), start=1):
        page = completed[first:first + per_sheet]
        canvas = Image.new(
            "RGB", (tile * len(columns), header + len(page) * (tile + footer)), (27, 29, 32)
        )
        draw = ImageDraw.Draw(canvas)
        for index, (label, _) in enumerate(columns):
            draw.text((index * tile + 10, 20), label, fill=(238, 240, 244), font=font)
        for row_index, row in enumerate(page):
            y = header + row_index * (tile + footer)
            for column, (_, key) in enumerate(columns):
                canvas.paste(
                    fit(Image.open(Path(row["images"][key])), tile),
                    (column * tile, y),
                )
            draw.text(
                (10, y + tile + 12),
                f"{int(row['rank']):02d} {row['name']}",
                fill=(215, 218, 223),
                font=font,
            )
        canvas.save(output / f"voxel-appearance-sheet-{sheet_index:02d}.png")


def equivalent_blank(row: dict) -> bool:
    geometry = row.get("geometry", {})
    return (
        geometry.get("visible_iou") == 0.0
        and geometry.get("visible_miss_pct") == 0.0
        and geometry.get("visible_extra_pct") == 0.0
    )


def write_diagnosis(output: Path, rows: list[dict]) -> None:
    completed = [row for row in rows if row.get("status") == "ok"]
    rendered = [row for row in completed if row.get("images")]
    visible = [row for row in rendered if not equivalent_blank(row)]
    cell_values = [row["cell"]["cell_iou"] for row in completed]
    visible_values = [row["geometry"]["visible_iou"] for row in visible]
    surface_values = [
        row["structural_score"]
        for row in completed
        if row.get("structural_score") is not None
    ]
    worst = sorted(visible, key=lambda row: row["geometry"]["visible_iou"])[:10]
    worst_structural = sorted(
        rendered, key=lambda row: row.get("structural_score", 1.0)
    )[:10]
    worst_cells = sorted(completed, key=lambda row: row["cell"]["cell_iou"])[:10]
    continuous = sorted(
        rendered, key=lambda row: row["fpt_vs_v7_mask"]["visible_iou"]
    )[:10]

    lines = [
        "# Exact FPTVOX7 50-scene diagnosis",
        "",
        "Both compared voxel paths use FPTVOX7 cell-clipped triangles. The authoritative reference is Mandelbulber's CPU/double PLY mesh path; Direct V7 is the Rust/Metal candidate. The harness rejects Mandelbulber OpenCL references because their topology can differ from the CPU exporter. FPT continuous is a separate diagnostic renderer and is not used for the PLY-vs-V7 headline metrics.",
        "",
        "## Aggregate",
        "",
        f"- Completed: {len(completed)}/{len(rows)}",
        f"- Rendered comparisons: {len(rendered)}/{len(completed)}",
        f"- Equivalent blank PLY/V7 views excluded from visible aggregates: {len(rendered) - len(visible)}",
        (
            f"- Cell IoU mean/median: {statistics.fmean(cell_values):.4f} / "
            f"{statistics.median(cell_values):.4f}"
            if cell_values
            else "- Cell IoU: no completed comparisons"
        ),
        (f"- PLY/V7 visible IoU mean/median: {statistics.fmean(visible_values):.4f} / {statistics.median(visible_values):.4f}" if visible_values else "- PLY/V7 visible IoU: not collected"),
        f"- PLY/V7 visible IoU >= 0.99: {sum(value >= 0.99 for value in visible_values)}/{len(visible)}",
        f"- PLY/V7 visible IoU >= 0.95: {sum(value >= 0.95 for value in visible_values)}/{len(visible)}",
        f"- Cell IoU >= 0.95: {sum(value >= 0.95 for value in cell_values)}/{len(completed)}",
        f"- Adaptive sampling retries: {sum(int(row.get('sampling_retry_count', 0)) for row in completed)}",
        (
            f"- Structural score mean/median: {statistics.fmean(surface_values):.4f} / "
            f"{statistics.median(surface_values):.4f}"
            if surface_values
            else "- Structural score: not collected"
        ),
        f"- Structural score >= 0.95: {sum(value >= 0.95 for value in surface_values)}/{len(surface_values)}",
        "",
        "## Worst structural parity",
        "",
        "The structural score is the minimum of occupied-cell IoU and per-cell triangle-area overlap. Triangle-count overlap remains diagnostic because equivalent surfaces can have different valid tessellations.",
        "",
        "| Rank | Scene | Structural | Cell | Triangle count | Triangle area |",
        "|---:|---|---:|---:|---:|---:|",
    ]
    for row in sorted(completed, key=lambda row: row.get("structural_score", 1.0))[:10]:
        surface = row["surface_payload"]
        lines.append(
            f"| {row['rank']} | {row['name']} | {row['structural_score']:.4f} | "
            f"{row['cell']['cell_iou']:.4f} | "
            f"{surface['triangle_count_iou_proxy']:.4f} | "
            f"{surface['triangle_area_iou_proxy']:.4f} |"
        )
    lines.extend([
        "",
        "## Worst Mandelbulber CPU vs Direct V7 cell parity",
        "",
        "| Rank | Scene | Cell IoU | Reference only | Candidate only |",
        "|---:|---|---:|---:|---:|",
    ])
    for row in worst_cells:
        cell = row["cell"]
        lines.append(
            f"| {row['rank']} | {row['name']} | {cell['cell_iou']:.4f} | "
            f"{cell['reference_only']} | {cell['candidate_only']} |"
        )
    lines.extend([
        "",
        "## Worst PLY V7 vs Direct V7 visible parity",
        "",
        "| Rank | Scene | Cell IoU | Visible IoU | Depth MAE | Miss | Extra |",
        "|---:|---|---:|---:|---:|---:|---:|",
    ])
    for row in worst:
        geometry = row["geometry"]
        depth = row.get("ply_vs_v7_depth", {})
        depth_mae = depth.get("relative_mae_pct")
        depth_text = "n/a" if depth_mae is None else f"{depth_mae:.3f}%"
        lines.append(
            f"| {row['rank']} | {row['name']} | {row['cell']['cell_iou']:.4f} | "
            f"{geometry['visible_iou']:.4f} | {depth_text} | {geometry['visible_miss_pct']:.2f}% | "
            f"{geometry['visible_extra_pct']:.2f}% |"
        )
    lines.extend([
        "",
        "## Worst FPT continuous vs Direct V7 masks",
        "",
        "These rows diagnose the continuous renderer/camera/DE path and must not be confused with Direct V7 conversion parity.",
        "",
        "| Rank | Scene | FPT/V7 visible IoU | PLY/V7 visible IoU |",
        "|---:|---|---:|---:|",
    ])
    for row in continuous:
        lines.append(
            f"| {row['rank']} | {row['name']} | {row['fpt_vs_v7_mask']['visible_iou']:.4f} | "
            f"{row['geometry']['visible_iou']:.4f} |"
        )
    (output / "diagnosis.md").write_text("\n".join(lines) + "\n")
    if worst:
        write_outlier_sheet(output, worst, "outliers-ply-v7.png")
    if worst_structural:
        write_outlier_sheet(output, worst_structural, "outliers-structural.png")


def write_outlier_sheet(output: Path, rows: list[dict], filename: str) -> None:
    columns = [
        ("FPT continuous", "fpt_geometry"),
        ("Mandel PLY V7 exact", "ply_white"),
        ("Direct V7 exact", "v7_white"),
        ("PLY vs V7 diff", "geometry_diff"),
    ]
    tile, header, footer = 300, 58, 48
    canvas = Image.new(
        "RGB", (tile * len(columns), header + len(rows) * (tile + footer)), (27, 29, 32)
    )
    draw = ImageDraw.Draw(canvas)
    font = ImageFont.load_default()
    for index, (label, _) in enumerate(columns):
        draw.text((index * tile + 10, 20), label, fill=(238, 240, 244), font=font)
    for row_index, row in enumerate(rows):
        y = header + row_index * (tile + footer)
        for column, (_, key) in enumerate(columns):
            canvas.paste(fit(Image.open(Path(row["images"][key])), tile), (column * tile, y))
        structural = row.get("structural_score", float("nan"))
        text = (
            f"{int(row['rank']):02d} {row['name']} | cell {row['cell']['cell_iou']:.3f} | "
            f"structural {structural:.3f} | "
            f"PLY/V7 visible {row['geometry']['visible_iou']:.3f} | "
            f"FPT/V7 visible {row['fpt_vs_v7_mask']['visible_iou']:.3f}"
        )
        draw.text((10, y + tile + 12), text, fill=(215, 218, 223), font=font)
    canvas.save(output / filename)


def load_completed(output: Path) -> dict[int, dict]:
    path = output / "summary.json"
    if not path.is_file():
        return {}
    return {int(row["rank"]): row for row in json.loads(path.read_text())}


def main() -> None:
    args = parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    manifest = json.loads(args.manifest.read_text())
    requested = None
    if args.ranks:
        requested = {int(value) for value in args.ranks.split(",") if value}
    selected = [row for row in manifest if requested is None or int(row["rank"]) in requested]
    completed = load_completed(args.output)
    render_script = ROOT / "scripts/render_fptvox7_parity_sheets.py"

    for source_row in selected:
        rank = int(source_row["rank"])
        if rank in completed and completed[rank].get("status") == "ok" and not args.force:
            print(f"[{rank:02d}] {source_row['name']}: cached")
            continue
        free = shutil.disk_usage(args.output).free
        if free < 2_000_000_000:
            raise RuntimeError(f"only {free / 1e9:.2f} GB free before rank {rank}")
        scene = args.output / f"{rank:02d}"
        scene.mkdir(parents=True, exist_ok=True)
        candidate = scene / "direct-v7.fptvox"
        reference = scene / "mandel-ply-v7.fptvox"
        raw_ply = scene / "mandel.ply"
        alignment_path = scene / "alignment.json"
        render_root = scene / "render"
        common = [
            args.fpt.as_posix(), "voxel-export", source_row["source"],
            "--voxel-resolution", str(args.voxel_resolution),
            "--bounds-min", ",".join(map(str, source_row["reference_bounds_min"])),
            "--bounds-max", ",".join(map(str, source_row["reference_bounds_max"])),
            "--mandelbulber-root", args.mandelbulber_root.as_posix(),
        ]
        try:
            effective_mesh_resolution = args.mesh_resolution
            sampling_retry_count = 0
            while True:
                candidate_report = run(
                    common[:3] + ["--out", candidate.as_posix()] + common[3:] + [
                        "--surface-triangles",
                        "--surface-triangle-resolution", str(effective_mesh_resolution),
                    ], ROOT, scene / "direct.stdout.log", scene / "direct.stderr.log")
                (scene / "direct.json").write_text(
                    json.dumps(candidate_report, indent=2) + "\n"
                )
                try:
                    reference_report = run(
                        common[:3] + ["--out", reference.as_posix()] + common[3:] + [
                            "--surface-source", "mandelbulber-mesh",
                            "--mandel-mesh-resolution", str(effective_mesh_resolution),
                            "--mandelbulber-bin", args.mandelbulber_bin.as_posix(),
                            "--mandel-mesh-ply-out", raw_ply.as_posix(),
                            "--surface-triangles",
                        ], ROOT, scene / "ply.stdout.log", scene / "ply.stderr.log")
                    break
                except RuntimeError as error:
                    next_resolution = effective_mesh_resolution * 2
                    can_retry = (
                        "Mandelbulber mesh is empty inside the requested bounds" in str(error)
                        and args.empty_reference_retry_max > 0
                        and next_resolution <= args.empty_reference_retry_max
                    )
                    if not can_retry:
                        raise
                    effective_mesh_resolution = next_resolution
                    sampling_retry_count += 1
                    print(
                        f"[{rank:02d}] {source_row['name']}: empty reference at "
                        f"m{effective_mesh_resolution // 2}; retrying m{effective_mesh_resolution}",
                        flush=True,
                    )
            if reference_report.get("opencl") is not False:
                raise RuntimeError(
                    "authoritative parity requires Mandelbulber CPU/double mesh export; "
                    f"reference reported opencl={reference_report.get('opencl')!r}"
                )
            (scene / "ply.json").write_text(json.dumps(reference_report, indent=2) + "\n")
            cell = cell_metrics(reference, candidate)
            surface_payload = surface_payload_metrics(reference, candidate)
            score = structural_score(cell, surface_payload)
            while (
                args.adaptive_parity_threshold > 0.0
                and score < args.adaptive_parity_threshold
                and effective_mesh_resolution * 2 <= args.empty_reference_retry_max
            ):
                previous_resolution = effective_mesh_resolution
                effective_mesh_resolution *= 2
                sampling_retry_count += 1
                print(
                    f"[{rank:02d}] {source_row['name']}: structural score "
                    f"{score:.3f} at m{previous_resolution}; "
                    f"retrying m{effective_mesh_resolution}",
                    flush=True,
                )
                candidate_report = run(
                    common[:3] + ["--out", candidate.as_posix()] + common[3:] + [
                        "--surface-triangles",
                        "--surface-triangle-resolution", str(effective_mesh_resolution),
                    ], ROOT, scene / "direct.stdout.log", scene / "direct.stderr.log")
                (scene / "direct.json").write_text(
                    json.dumps(candidate_report, indent=2) + "\n"
                )
                reference_report = run(
                    common[:3] + ["--out", reference.as_posix()] + common[3:] + [
                        "--surface-source", "mandelbulber-mesh",
                        "--mandel-mesh-resolution", str(effective_mesh_resolution),
                        "--mandelbulber-bin", args.mandelbulber_bin.as_posix(),
                        "--mandel-mesh-ply-out", raw_ply.as_posix(),
                        "--surface-triangles",
                    ], ROOT, scene / "ply.stdout.log", scene / "ply.stderr.log")
                if reference_report.get("opencl") is not False:
                    raise RuntimeError(
                        "authoritative parity requires Mandelbulber CPU/double mesh export; "
                        f"reference reported opencl={reference_report.get('opencl')!r}"
                    )
                (scene / "ply.json").write_text(
                    json.dumps(reference_report, indent=2) + "\n"
                )
                cell = cell_metrics(reference, candidate)
                surface_payload = surface_payload_metrics(reference, candidate)
                score = structural_score(cell, surface_payload)
            if args.cell_only:
                row = {
                    "rank": rank,
                    "name": source_row["name"],
                    "status": "ok",
                    "cell": cell,
                    "surface_payload": surface_payload,
                    "structural_score": score,
                    "source": source_row["source"],
                    "reference_backend": "mandelbulber-cpu-double",
                    "reference_representation": "fptvox7-cell-clipped-triangles",
                    "candidate_representation": "fptvox7-cell-clipped-triangles",
                    "requested_mesh_resolution": args.mesh_resolution,
                    "effective_mesh_resolution": effective_mesh_resolution,
                    "sampling_retry_count": sampling_retry_count,
                }
                (scene / "metrics.json").write_text(json.dumps(row, indent=2) + "\n")
                completed[rank] = row
                print(
                    f"[{rank:02d}] {source_row['name']}: "
                    f"cell {cell['cell_iou']:.3f}, structural {score:.3f}"
                )
                continue
            alignment = [{
                "rank": rank,
                "name": source_row["name"],
                "source": source_row["source"],
                "reference_volume": reference.resolve().as_posix(),
                "v7_volume": candidate.resolve().as_posix(),
                "world_camera": source_row["world_camera"],
                "yaw": source_row["yaw"],
                "pitch": source_row["pitch"],
                "roll": source_row.get("roll", 0.0),
                "fov": source_row["fov"],
                "legacy_coordinate_system": source_row.get("legacy_coordinate_system", False),
            }]
            alignment_path.write_text(json.dumps(alignment, indent=2) + "\n")
            render_result = subprocess.run([
                sys.executable, render_script.as_posix(),
                "--alignment-summary", alignment_path.as_posix(),
                "--output", render_root.as_posix(),
                "--fpt", args.fpt.as_posix(),
                "--naadf", args.naadf.as_posix(),
                "--mandelbulber-root", args.mandelbulber_root.as_posix(),
                "--image-size", str(args.image_size),
                "--camera-mode", "fit-volume", "--force",
            ], cwd=ROOT, text=True, capture_output=True, timeout=1800)
            (scene / "render.stdout.log").write_text(render_result.stdout)
            (scene / "render.stderr.log").write_text(render_result.stderr)
            if render_result.returncode:
                raise RuntimeError(render_result.stderr.strip() or render_result.stdout.strip())
            render_summary = json.loads((render_root / "summary.json").read_text())[0]
            row = {
                **render_summary,
                "status": "ok",
                "cell": cell,
                "surface_payload": surface_payload,
                "structural_score": score,
                "source": source_row["source"],
                "reference_backend": "mandelbulber-cpu-double",
                "reference_representation": "fptvox7-cell-clipped-triangles",
                "candidate_representation": "fptvox7-cell-clipped-triangles",
                "requested_mesh_resolution": args.mesh_resolution,
                "effective_mesh_resolution": effective_mesh_resolution,
                "sampling_retry_count": sampling_retry_count,
            }
            (scene / "metrics.json").write_text(json.dumps(row, indent=2) + "\n")
            compact_render_artifacts(render_root, rank)
            completed[rank] = row
            print(
                f"[{rank:02d}] {source_row['name']}: "
                f"cell {cell['cell_iou']:.3f}, visible {row['geometry']['visible_iou']:.3f}"
            )
        except Exception as error:
            row = {
                "rank": rank,
                "name": source_row["name"],
                "source": source_row["source"],
                "status": "failed",
                "error": str(error),
            }
            (scene / "metrics.json").write_text(json.dumps(row, indent=2) + "\n")
            completed[rank] = row
            print(f"[{rank:02d}] {source_row['name']}: FAILED: {error}")
        finally:
            if not args.keep_volumes:
                for transient in (candidate, reference, raw_ply):
                    if transient.is_file():
                        transient.unlink()
            write_reports(args.output, list(completed.values()))
    write_reports(args.output, list(completed.values()))


if __name__ == "__main__":
    main()
