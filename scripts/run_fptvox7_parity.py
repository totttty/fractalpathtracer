#!/usr/bin/env python3
"""Differential FPTVOX7 vs Mandelbulber PLY -> NAADF geometry harness."""

from __future__ import annotations

import argparse
import csv
import json
import math
import struct
import subprocess
import time
from pathlib import Path

import numpy as np
from scipy.spatial import cKDTree


ROOT = Path(__file__).resolve().parents[1]
HISTORICAL = ROOT / "reports/fpt-mandel-authoritative-50-r192-m384-20260812"
DEFAULT_MANDEL_ROOT = Path("/Volumes/Ventura/Projects/mandelbulber2/mandelbulber2")
DEFAULT_MANDEL_BIN = Path(
    "/Volumes/Ventura/Projects/mandelbulber2/build-opencl/"
    "mandelbulber2.app/Contents/MacOS/mandelbulber2"
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, default=ROOT / "reports/fptvox7-parity")
    parser.add_argument("--ranks", default="1,2,3,4,5")
    parser.add_argument("--voxel-resolution", type=int, default=96)
    parser.add_argument("--mesh-resolution", type=int, default=192)
    parser.add_argument("--bounds-min", default="-4,-4,-4")
    parser.add_argument("--bounds-max", default="4,4,4")
    parser.add_argument("--fpt", type=Path, default=ROOT / "target/release/fpt-metal")
    parser.add_argument("--mandelbulber-root", type=Path, default=DEFAULT_MANDEL_ROOT)
    parser.add_argument("--mandelbulber-bin", type=Path, default=DEFAULT_MANDEL_BIN)
    parser.add_argument("--force", action="store_true")
    return parser.parse_args()


def run(command: list[str], cwd: Path, stdout: Path, stderr: Path) -> tuple[dict, float]:
    started = time.perf_counter()
    completed = subprocess.run(command, cwd=cwd, text=True, capture_output=True, timeout=1800)
    elapsed = time.perf_counter() - started
    stdout.write_text(completed.stdout)
    stderr.write_text(completed.stderr)
    if completed.returncode:
        raise RuntimeError(completed.stderr.strip() or completed.stdout.strip())
    return json.loads(completed.stdout), elapsed


def scene_rows(ranks: list[int]) -> list[dict]:
    summary = json.loads((HISTORICAL / "summary.json").read_text())
    by_rank = {int(row["rank"]): row for row in summary}
    result = []
    for rank in ranks:
        directories = list((HISTORICAL / "scenes").glob(f"{rank:02d}-*"))
        if len(directories) != 1:
            raise RuntimeError(f"rank {rank}: expected one historical scene directory")
        command_path = directories[0] / "command.json"
        if not command_path.is_file():
            command_path = directories[0] / "geometry-command.json"
        if not command_path.is_file():
            raise RuntimeError(f"rank {rank}: archived source command is unavailable")
        command = json.loads(command_path.read_text())
        result.append({**by_rank[rank], "source": command[2]})
    return result


def u32(data: bytes, offset: int) -> int:
    return struct.unpack_from("<I", data, offset)[0]


def u64(data: bytes, offset: int) -> int:
    return struct.unpack_from("<Q", data, offset)[0]


def fptvox_cells(path: Path) -> tuple[set[tuple[int, int, int]], dict]:
    data = path.read_bytes()
    version = u32(data, 12)
    header = u32(data, 8)
    record_size = {1: 24, 2: 28, 3: 28, 5: 32, 6: 36, 7: 32}[version]
    count = u64(data, 56)
    cells = {
        struct.unpack_from("<III", data, header + index * record_size)
        for index in range(count)
    }
    metadata = {"version": version, "cell_count": count, "bytes": len(data)}
    if version == 7:
        counts = [u32(data, header + index * record_size + 28) for index in range(count)]
        metadata.update(
            triangle_count=u64(data, 80),
            triangle_per_cell_min=min(counts),
            triangle_per_cell_median=float(np.median(counts)),
            triangle_per_cell_p90=float(np.quantile(counts, 0.90)),
            triangle_per_cell_p99=float(np.quantile(counts, 0.99)),
            triangle_per_cell_max=max(counts),
        )
    return cells, metadata


def fptvox7_vertices(path: Path, limit: int = 100_000) -> np.ndarray:
    data = path.read_bytes()
    resolution = np.asarray(struct.unpack_from("<III", data, 16), dtype=np.float64)
    bounds_min = np.asarray(struct.unpack_from("<fff", data, 32), dtype=np.float64)
    bounds_max = np.asarray(struct.unpack_from("<fff", data, 44), dtype=np.float64)
    cell_count = u64(data, 56)
    triangle_offset = 96 + cell_count * 32
    output: list[tuple[float, float, float]] = []
    stride = max(1, math.ceil((u64(data, 80) * 3) / limit))
    serial = 0
    for index in range(cell_count):
        offset = 96 + index * 32
        coordinate = np.asarray(struct.unpack_from("<III", data, offset), dtype=np.float64)
        first = u32(data, offset + 24)
        count = u32(data, offset + 28)
        for triangle in range(count):
            for vertex in range(3):
                if serial % stride == 0:
                    packed = u32(data, triangle_offset + (first + triangle) * 12 + vertex * 4)
                    local = np.asarray(
                        [packed & 1023, (packed >> 10) & 1023, (packed >> 20) & 1023],
                        dtype=np.float64,
                    ) / 1023.0
                    position = bounds_min + (coordinate + local) / resolution * (bounds_max - bounds_min)
                    output.append(tuple(position))
                serial += 1
    return np.asarray(output)


def ply_vertices(path: Path) -> np.ndarray:
    data = path.read_bytes()
    marker = data.index(b"end_header\n") + len(b"end_header\n")
    header = data[:marker].decode()
    vertex_count = int(next(line.split()[2] for line in header.splitlines() if line.startswith("element vertex ")))
    properties = [line for line in header.splitlines() if line.startswith("property ")]
    if properties[:8] != [
        "property double x", "property double y", "property double z",
        "property double s", "property double t", "property uchar red",
        "property uchar green", "property uchar blue",
    ]:
        raise RuntimeError("unsupported Mandelbulber PLY vertex layout")
    stride = 43
    return np.asarray([struct.unpack_from("<ddd", data, marker + index * stride) for index in range(vertex_count)])


def surface_distance(v7: np.ndarray, ply: np.ndarray) -> dict:
    v7_to_ply = cKDTree(ply).query(v7)[0]
    ply_to_v7 = cKDTree(v7).query(ply)[0]
    return {
        "v7_to_ply_mean": float(v7_to_ply.mean()),
        "v7_to_ply_p95": float(np.quantile(v7_to_ply, 0.95)),
        "v7_to_ply_max": float(v7_to_ply.max()),
        "ply_to_v7_mean": float(ply_to_v7.mean()),
        "ply_to_v7_p95": float(np.quantile(ply_to_v7, 0.95)),
        "ply_to_v7_max": float(ply_to_v7.max()),
    }


def material_metrics(v7_path: Path, ply_path: Path) -> dict:
    def records(path: Path) -> dict[tuple[int, int, int], tuple[int, int, int]]:
        data = path.read_bytes()
        version = u32(data, 12)
        header = u32(data, 8)
        stride = {6: 36, 7: 32}[version]
        return {
            struct.unpack_from("<III", data, header + index * stride):
                struct.unpack_from("<III", data, header + index * stride + 12)
            for index in range(u64(data, 56))
        }

    v7 = records(v7_path)
    ply = records(ply_path)
    common = sorted(v7.keys() & ply.keys())
    v7_color = np.asarray([v7[cell][0] & 0x00ff_ffff for cell in common], dtype=np.uint32)
    ply_color = np.asarray([ply[cell][0] & 0x00ff_ffff for cell in common], dtype=np.uint32)
    unpack = lambda values: np.column_stack((
        values & 255, (values >> 8) & 255, (values >> 16) & 255)).astype(np.float64)
    difference = np.abs(unpack(v7_color) - unpack(ply_color))
    return {
        "material_common_cells": len(common),
        "material_color_exact_pct": float(np.mean(v7_color == ply_color) * 100.0),
        "material_rgb_mae_255": float(difference.mean()),
        "material_rgb_rmse_255": float(np.sqrt(np.mean(difference * difference))),
        "material_rgb_p95_max_255": float(np.quantile(difference.max(axis=1), 0.95)),
        "material_properties_exact_pct": float(np.mean(
            [v7[cell][1] == ply[cell][1] for cell in common]) * 100.0),
        "material_emission_exact_pct": float(np.mean(
            [v7[cell][2] == ply[cell][2] for cell in common]) * 100.0),
    }


def main() -> None:
    args = parse_args()
    ranks = [int(value) for value in args.ranks.split(",") if value]
    args.output.mkdir(parents=True, exist_ok=True)
    rows = []
    for scene in scene_rows(ranks):
        rank = int(scene["rank"])
        out = args.output / f"{rank:02d}"
        out.mkdir(parents=True, exist_ok=True)
        v7 = out / "scene-v7.fptvox"
        ply_volume = out / "scene-ply.fptvox"
        ply = out / "scene.ply"
        common = [
            args.fpt.as_posix(), "voxel-export", scene["source"],
            "--voxel-resolution", str(args.voxel_resolution),
            "--bounds-min", args.bounds_min, "--bounds-max", args.bounds_max,
            "--mandelbulber-root", args.mandelbulber_root.as_posix(),
        ]
        try:
            if args.force or not (v7.is_file() and (out / "v7.json").is_file()):
                report, wall = run(
                    common[:3] + ["--out", v7.as_posix()] + common[3:] + [
                        "--surface-triangles", "--surface-triangle-resolution", str(args.mesh_resolution)
                    ], ROOT, out / "v7.stdout.log", out / "v7.stderr.log")
                report["wall_s"] = wall
                (out / "v7.json").write_text(json.dumps(report, indent=2) + "\n")
            v7_report = json.loads((out / "v7.json").read_text())
            if args.force or not (ply_volume.is_file() and ply.is_file() and (out / "ply.json").is_file()):
                report, wall = run(
                    common[:3] + ["--out", ply_volume.as_posix()] + common[3:] + [
                        "--surface-source", "mandelbulber-mesh",
                        "--mandel-mesh-resolution", str(args.mesh_resolution),
                        "--mandelbulber-bin", args.mandelbulber_bin.as_posix(),
                        "--mandel-mesh-ply-out", ply.as_posix(),
                    ], ROOT, out / "ply.stdout.log", out / "ply.stderr.log")
                report["wall_s"] = wall
                (out / "ply.json").write_text(json.dumps(report, indent=2) + "\n")
        except Exception as error:
            row = {
                "rank": rank, "name": scene["name"], "source": scene["source"],
                "status": "failed", "error": str(error),
            }
            rows.append(row)
            (out / "metrics.json").write_text(json.dumps(row, indent=2) + "\n")
            print(f"[{rank:02d}] {scene['name']}: FAILED: {error}")
            continue
        ply_report = json.loads((out / "ply.json").read_text())
        v7_cells, v7_meta = fptvox_cells(v7)
        ply_cells, ply_meta = fptvox_cells(ply_volume)
        intersection = v7_cells & ply_cells
        union = v7_cells | ply_cells
        distances = surface_distance(fptvox7_vertices(v7), ply_vertices(ply))
        row = {
            "rank": rank, "name": scene["name"], "source": scene["source"],
            "status": "complete", "error": "",
            "topology_policy": v7_report["surface"]["topology_policy"],
            "topology_fallback_offset": v7_report["surface"]["topology_fallback_offset"],
            "v7_source_triangles": v7_report["surface"]["source_triangles"],
            "ply_source_triangles": ply_report["mesh"]["mesh_triangles"],
            "v7_cells": len(v7_cells), "ply_cells": len(ply_cells),
            "cell_intersection": len(intersection), "cell_union": len(union),
            "cell_iou": len(intersection) / max(1, len(union)),
            "v7_only_cells": len(v7_cells - ply_cells),
            "ply_only_cells": len(ply_cells - v7_cells),
            "v7_warm_ms": sum(v7_report["surface"][key] for key in (
                "metal_topology_ms", "marching_cubes_ms", "clipping_ms", "metal_material_ms")),
            "ply_warm_ms": ply_report["mesh"]["mesh_export_ms"] +
                ply_report["mesh"]["ply_parse_ms"] + ply_report["mesh"]["voxelize_ms"],
            **distances, **material_metrics(v7, ply_volume),
            **{f"v7_{key}": value for key, value in v7_meta.items()},
            **{f"ply_{key}": value for key, value in ply_meta.items()},
        }
        rows.append(row)
        (out / "metrics.json").write_text(json.dumps(row, indent=2) + "\n")
        print(f"[{rank:02d}] {scene['name']}: IoU {row['cell_iou']:.6f}, policy {row['topology_policy']}")
    (args.output / "summary.json").write_text(json.dumps(rows, indent=2) + "\n")
    fieldnames = list(dict.fromkeys(key for row in rows for key in row))
    with (args.output / "summary.csv").open("w", newline="") as file:
        writer = csv.DictWriter(file, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(rows)


if __name__ == "__main__":
    main()
