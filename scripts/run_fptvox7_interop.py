#!/usr/bin/env python3
"""Generate ranked FPTVOX7 scenes and smoke them through native NAADF."""

from __future__ import annotations

import argparse
import csv
import json
import subprocess
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = (
    ROOT / "reports/fptvox7-ranked50-latest-naadf-300px-32spp-20260815/summary.json"
)
DEFAULT_NAADF = Path(
    "/Users/jordantotty/Desktop/vox/metal-voxel-naadf-pathtracer/"
    "build/MetalVoxel.app/Contents/MacOS/MetalVoxel"
)
DEFAULT_MANDEL_ROOT = Path("/Volumes/Ventura/Projects/mandelbulber2/mandelbulber2")


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--ranks", default=",".join(str(value) for value in range(1, 51)))
    parser.add_argument("--voxel-resolution", type=int, default=48)
    parser.add_argument("--triangle-resolution", type=int, default=96)
    parser.add_argument("--fpt", type=Path, default=ROOT / "target/release/fpt-metal")
    parser.add_argument("--naadf", type=Path, default=DEFAULT_NAADF)
    parser.add_argument("--mandelbulber-root", type=Path, default=DEFAULT_MANDEL_ROOT)
    parser.add_argument("--force", action="store_true")
    return parser.parse_args()


def ranked_sources(manifest: Path, ranks: list[int]) -> list[dict]:
    rows = {int(row["rank"]): row for row in json.loads(manifest.read_text())}
    output = []
    for rank in ranks:
        row = rows.get(rank)
        if row is None:
            output.append({"rank": rank, "name": f"rank {rank}", "source_error": "rank missing from manifest"})
            continue
        required = ("source", "reference_bounds_min", "reference_bounds_max")
        if any(key not in row for key in required):
            output.append({**row, "source_error": "manifest lacks source or reference bounds"})
        else:
            output.append(row)
    return output


def run(command: list[str], stdout: Path, stderr: Path, timeout: int = 1800) -> tuple[int, float]:
    started = time.perf_counter()
    result = subprocess.run(command, text=True, capture_output=True, timeout=timeout)
    elapsed = time.perf_counter() - started
    stdout.write_text(result.stdout)
    stderr.write_text(result.stderr)
    return result.returncode, elapsed


def main() -> None:
    args = arguments()
    ranks = [int(value) for value in args.ranks.split(",") if value]
    args.output.mkdir(parents=True, exist_ok=True)
    summary = []
    for scene in ranked_sources(args.manifest, ranks):
        rank = int(scene["rank"])
        directory = args.output / f"{rank:02d}"
        directory.mkdir(parents=True, exist_ok=True)
        row = {"rank": rank, "name": scene["name"], "source": scene.get("source", "")}
        if "source_error" in scene:
            row.update(status="failed", error=scene["source_error"])
            summary.append(row)
            continue
        volume = directory / "scene.fptvox"
        export_json = directory / "export.json"
        try:
            if args.force or not (volume.is_file() and export_json.is_file()):
                code, wall = run([
                    args.fpt.as_posix(), "voxel-export", scene["source"],
                    "--out", volume.as_posix(),
                    "--voxel-resolution", str(args.voxel_resolution),
                    "--surface-triangles",
                    "--surface-triangle-resolution", str(args.triangle_resolution),
                    "--bounds-min", ",".join(map(str, scene["reference_bounds_min"])),
                    "--bounds-max", ",".join(map(str, scene["reference_bounds_max"])),
                    "--mandelbulber-root", args.mandelbulber_root.as_posix(),
                ], directory / "export.stdout.log", directory / "export.stderr.log")
                if code:
                    raise RuntimeError((directory / "export.stderr.log").read_text().strip())
                report = json.loads((directory / "export.stdout.log").read_text())
                report["wall_s"] = wall
                export_json.write_text(json.dumps(report, indent=2) + "\n")
            report = json.loads(export_json.read_text())
            native_report = directory / "native"
            code, native_wall = run([
                args.naadf.as_posix(), "--mode", "greedy2d", "--fptvox", volume.as_posix(),
                "--size", "256x144", "--gpu-renderer", "naadf", "--gpu-naadf-mode", "aadf",
                "--gpu-naadf-build", "cpu", "--gpu-naadf-primary-layout", "direct16",
                "--gpu-naadf-path-tracing", "fixed", "--gpu-naadf-path-samples-per-frame", "1",
                "--gpu-naadf-path-max-samples", "1", "--gpu-naadf-path-bounces", "2",
                "--benchmark", "--offscreen", "--frames", "1", "--warmup", "0",
                "--report", native_report.as_posix(),
            ], directory / "native.stdout.log", directory / "native.stderr.log", 300)
            if code:
                raise RuntimeError((directory / "native.stderr.log").read_text().strip())
            native_row = next(csv.DictReader((native_report.with_suffix(".csv")).open()))
            surface = report.get("surface", {})
            export_summary = report.get("summary", {})
            row.update(
                status="complete",
                occupied_cells=surface.get("occupied_cells", export_summary.get("voxel_count", 0)),
                triangle_count=surface.get("clipped_triangles", 0),
                topology_policy=surface.get("topology_policy", ""),
                export_wall_s=report.get("wall_s", 0.0),
                native_wall_s=native_wall,
                native_gpu_ms=float(native_row["gpu_ms"]),
                native_fps=float(native_row["fps"]),
                fptvox_bytes=volume.stat().st_size,
                error="",
            )
        except Exception as error:
            row.update(status="failed", error=str(error))
        summary.append(row)
        print(f"[{rank:02d}] {row['name']}: {row['status']}", flush=True)

    fields = sorted({key for row in summary for key in row})
    with (args.output / "summary.csv").open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fields)
        writer.writeheader()
        writer.writerows(summary)
    (args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")


if __name__ == "__main__":
    main()
