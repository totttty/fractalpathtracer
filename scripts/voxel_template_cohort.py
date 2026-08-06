#!/usr/bin/env python3
"""Compare sparse and occupancy-template voxel bricks across scene cohorts."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path
from statistics import median

from PIL import Image, ImageChops


DEFAULT_SCENES = (
    "scenes/readme/01-Render005.json",
    "scenes/readme/02-Render_14.json",
    "scenes/readme/03-Cornell_box.json",
    "scenes/readme/04-Glass_Ball.json",
    "scenes/readme/07-M4.json",
    "scenes/readme/08-Render0ad03.json",
    "scenes/readme/09-Glass.json",
)


def render(
    binary: Path,
    scene: Path,
    out: Path,
    storage: str,
    width: int,
    height: int,
    samples: int,
    resolution: int,
) -> tuple[Path, dict[str, object]]:
    out.mkdir(parents=True, exist_ok=True)
    command = [
        str(binary),
        "render",
        str(scene),
        "--renderer",
        "voxel",
        "--voxel-resolution",
        str(resolution),
        "--voxel-surface-band",
        "0.50",
        "--voxel-storage",
        storage,
        "--voxel-build",
        "staging",
        "--voxel-material",
        "exact",
        "--voxel-normal",
        "face",
        "--width",
        str(width),
        "--height",
        str(height),
        "--samples",
        str(samples),
        "--out",
        str(out),
    ]
    completed = subprocess.run(
        command,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(
            f"{storage} render failed for {scene}:\n{completed.stdout[-4000:]}"
        )
    metadata_path = next(out.glob("*.render.json"))
    image_path = Path(str(metadata_path)[: -len(".render.json")])
    return image_path, json.loads(metadata_path.read_text())


def difference(reference: Path, candidate: Path) -> dict[str, float | int]:
    first = Image.open(reference).convert("RGB")
    second = Image.open(candidate).convert("RGB")
    delta = ImageChops.difference(first, second)
    values = list(delta.getdata())
    return {
        "changed_pixels": sum(value != (0, 0, 0) for value in values),
        "maximum_channel_error": max(max(value) for value in values),
        "mae": sum(sum(value) for value in values) / (len(values) * 3),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=Path("target/release/fpt-metal"))
    parser.add_argument("--scene", action="append")
    parser.add_argument(
        "--out", type=Path, default=Path("reports/gpu-fractal-experiments/template-cohort")
    )
    parser.add_argument("--width", type=int, default=240)
    parser.add_argument("--height", type=int, default=135)
    parser.add_argument("--samples", type=int, default=1)
    parser.add_argument("--resolution", type=int, default=256)
    parser.add_argument("--runs", type=int, default=3)
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be at least one")

    binary = args.binary.resolve()
    scenes = [Path(value) for value in (args.scene or DEFAULT_SCENES)]
    entries: list[dict[str, object]] = []
    for index, scene in enumerate(scenes, 1):
        scene = scene.resolve()
        scene_out = args.out / scene.stem
        results: dict[str, list[tuple[Path, dict[str, object]]]] = {
            "sparse-bricks": [],
            "template-bricks": [],
        }
        differences: list[dict[str, float | int]] = []
        for run in range(args.runs):
            order = (
                ("sparse-bricks", "template-bricks")
                if run % 2 == 0
                else ("template-bricks", "sparse-bricks")
            )
            for storage in order:
                results[storage].append(
                    render(
                        binary,
                        scene,
                        scene_out / f"run-{run + 1:02d}" / storage,
                        storage,
                        args.width,
                        args.height,
                        args.samples,
                        args.resolution,
                    )
                )
            differences.append(
                difference(
                    results["sparse-bricks"][run][0],
                    results["template-bricks"][run][0],
                )
            )
        sparse_runs = [result[1] for result in results["sparse-bricks"]]
        template_runs = [result[1] for result in results["template-bricks"]]
        sparse = sparse_runs[0]
        template = template_runs[0]
        sparse_gpu_ms = median(run["elapsed_ms"] for run in sparse_runs)
        template_gpu_ms = median(run["elapsed_ms"] for run in template_runs)
        sparse_build_ms = median(run["acceleration_build_ms"] for run in sparse_runs)
        template_build_ms = median(
            run["acceleration_build_ms"] for run in template_runs
        )
        delta = {
            "changed_pixels": max(item["changed_pixels"] for item in differences),
            "maximum_channel_error": max(
                item["maximum_channel_error"] for item in differences
            ),
            "mae": max(item["mae"] for item in differences),
        }
        entry = {
            "scene": str(scene),
            "runs": args.runs,
            "sparse_gpu_ms": sparse_gpu_ms,
            "template_gpu_ms": template_gpu_ms,
            "render_speedup": sparse_gpu_ms / template_gpu_ms,
            "sparse_build_ms": sparse_build_ms,
            "template_build_ms": template_build_ms,
            "build_ratio": template_build_ms / sparse_build_ms,
            "sparse_memory_bytes": sparse["acceleration_memory_bytes"],
            "template_memory_bytes": template["acceleration_memory_bytes"],
            "memory_reduction": sparse["acceleration_memory_bytes"]
            / template["acceleration_memory_bytes"],
            "active_bricks": sparse["voxel_active_bricks"],
            "active_cells": sparse["voxel_active_cells"],
            "occupancy_fraction": sparse["voxel_active_cells"]
            / float(args.resolution**3),
            "difference": delta,
        }
        entries.append(entry)
        print(
            f"{index:02d}/{len(scenes):02d} {scene.stem}: "
            f"memory {entry['memory_reduction']:.2f}x, "
            f"render {entry['render_speedup']:.2f}x, "
            f"build {entry['build_ratio']:.2f}x, "
            f"changed {delta['changed_pixels']}"
        )

    report = {
        "schema_version": 1,
        "width": args.width,
        "height": args.height,
        "samples": args.samples,
        "runs": args.runs,
        "voxel_resolution": args.resolution,
        "scenes": len(entries),
        "all_images_bit_exact": all(
            entry["difference"]["changed_pixels"] == 0 for entry in entries
        ),
        "median_memory_reduction": median(
            entry["memory_reduction"] for entry in entries
        ),
        "median_render_speedup": median(entry["render_speedup"] for entry in entries),
        "median_build_ratio": median(entry["build_ratio"] for entry in entries),
        "entries": entries,
    }
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
