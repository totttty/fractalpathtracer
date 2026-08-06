#!/usr/bin/env python3
"""Autotune scene-bound Mandel formula compiler candidates with exact PNG gates."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import time
from pathlib import Path
from statistics import median
from typing import Any

from PIL import Image, ImageChops


EXPERIMENT_VARIABLES = (
    "FPT_MANDEL_FORMULA_PARTIAL_EVAL",
    "FPT_MANDEL_FORMULA_PHASES",
    "FPT_MANDEL_FORMULA_SCALARIZE_LOOPS",
    "FPT_MANDEL_FORMULA_DCE",
    "FPT_MANDEL_FORMULA_CSE",
    "FPT_MANDEL_FORMULA_IDS",
    "FPT_MANDEL_FORMULA_STATS",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("mandelbulber_root", type=Path)
    parser.add_argument("--binary", type=Path, default=Path("target/release/fpt-metal"))
    parser.add_argument(
        "--cohort", type=Path, default=Path("reports/mandel-optimization/cohort.json")
    )
    parser.add_argument(
        "--indices", default="19,135,266,278,638", help="comma-separated corpus indices"
    )
    parser.add_argument(
        "--candidate-formula-ids",
        help="limit formula-specific candidates to these comma-separated IDs",
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=Path("reports/mandel-optimization/formula-compiler-autotune"),
    )
    parser.add_argument("--width", type=int, default=120)
    parser.add_argument("--height", type=int, default=68)
    parser.add_argument(
        "--workload",
        choices=("diffuse-normal", "pathtrace"),
        default="diffuse-normal",
    )
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--minimum-speedup", type=float, default=1.10)
    parser.add_argument("--timeout", type=float, default=120.0)
    return parser.parse_args()


def candidate_matrix(
    formula_ids: list[int], hybrid: bool
) -> list[tuple[str, dict[str, str]]]:
    candidates: list[tuple[str, dict[str, str]]] = [("baseline", {})]
    id_sets = [(str(formula_id), [formula_id]) for formula_id in formula_ids]
    if len(formula_ids) > 1:
        id_sets.append(("all", formula_ids))
    for label, selected in id_sets:
        selected_ids = ",".join(map(str, selected))
        candidates.extend(
            [
                (
                    f"structural-{label}",
                    {
                        "FPT_MANDEL_FORMULA_PARTIAL_EVAL": "1",
                        "FPT_MANDEL_FORMULA_IDS": selected_ids,
                    },
                ),
                (
                    f"structural-loop-dce-{label}",
                    {
                        "FPT_MANDEL_FORMULA_PARTIAL_EVAL": "1",
                        "FPT_MANDEL_FORMULA_SCALARIZE_LOOPS": "1",
                        "FPT_MANDEL_FORMULA_DCE": "1",
                        "FPT_MANDEL_FORMULA_CSE": "1",
                        "FPT_MANDEL_FORMULA_IDS": selected_ids,
                    },
                ),
                (
                    f"phase-loop-dce-{label}",
                    {
                        "FPT_MANDEL_FORMULA_PHASES": "1",
                        "FPT_MANDEL_FORMULA_SCALARIZE_LOOPS": "1",
                        "FPT_MANDEL_FORMULA_DCE": "1",
                        "FPT_MANDEL_FORMULA_CSE": "1",
                        "FPT_MANDEL_FORMULA_IDS": selected_ids,
                    },
                ),
            ]
        )
    return candidates


def image_changed_pixels(baseline: Path, candidate: Path) -> int:
    with Image.open(baseline) as first_image, Image.open(candidate) as second_image:
        first = first_image.convert("RGB")
        second = second_image.convert("RGB")
        if first.size != second.size:
            return first.width * first.height
        difference = ImageChops.difference(first, second)
        return sum(pixel != (0, 0, 0) for pixel in difference.getdata())


def render(
    *,
    binary: Path,
    root: Path,
    scene: Path,
    output: Path,
    width: int,
    height: int,
    environment: dict[str, str],
    workload: str,
    timeout: float,
) -> tuple[Path, dict[str, Any], str]:
    output.mkdir(parents=True, exist_ok=True)
    command = [
        str(binary),
        "render",
        str(scene),
        "--out",
        str(output),
        "--renderer",
        "sdf",
        "--mandelbulber-root",
        str(root),
        "--width",
        str(width),
        "--height",
        str(height),
        "--samples",
        "1",
    ]
    if workload == "diffuse-normal":
        command.extend(["--mode", "diffuse-normal"])
    child_environment = os.environ.copy()
    for name in EXPERIMENT_VARIABLES:
        child_environment.pop(name, None)
    child_environment.update(environment)
    started = time.perf_counter()
    child = subprocess.run(
        command,
        env=child_environment,
        capture_output=True,
        text=True,
        timeout=timeout,
        check=False,
    )
    if child.returncode != 0:
        raise RuntimeError(child.stderr[-4000:])
    pngs = sorted(output.glob("*.png"))
    if len(pngs) != 1:
        raise RuntimeError(f"expected one PNG in {output}, found {len(pngs)}")
    metadata_path = pngs[0].with_suffix(pngs[0].suffix + ".render.json")
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    metadata["wall_ms"] = (time.perf_counter() - started) * 1000.0
    return pngs[0], metadata, child.stderr[-4000:]


def safe_name(path: str) -> str:
    return re.sub(r"[^A-Za-z0-9_.-]+", "-", Path(path).stem).strip("-")


def main() -> int:
    args = parse_args()
    root = args.mandelbulber_root.resolve()
    binary = args.binary.resolve()
    cohort_path = args.cohort.resolve()
    output = args.out.resolve()
    output.mkdir(parents=True, exist_ok=True)
    selected_indices = {int(value) for value in args.indices.split(",") if value.strip()}
    candidate_formula_ids = (
        {
            int(value)
            for value in args.candidate_formula_ids.split(",")
            if value.strip()
        }
        if args.candidate_formula_ids
        else None
    )
    cohort = json.loads(cohort_path.read_text(encoding="utf-8"))
    source_scenes = cohort.get("scenes")
    if not isinstance(source_scenes, list):
        source_scenes = [
            {**scene, "corpus_index": index}
            for index, scene in enumerate(cohort["entries"], start=1)
        ]
    scenes = [
        scene for scene in source_scenes if int(scene["corpus_index"]) in selected_indices
    ]
    if not scenes:
        raise ValueError("no selected cohort scenes")
    if args.runs < 1:
        raise ValueError("--runs must be positive")

    report: dict[str, Any] = {
        "schema_version": 1,
        "renderer_binary": str(binary),
        "renderer_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "mandelbulber_root": str(root),
        "width": args.width,
        "height": args.height,
        "workload": args.workload,
        "runs": args.runs,
        "minimum_speedup": args.minimum_speedup,
        "scenes": [],
    }
    for scene_record in scenes:
        corpus_index = int(scene_record["corpus_index"])
        scene_path = root / scene_record["path"]
        scene_output = output / f"{corpus_index:04d}-{safe_name(scene_record['path'])}"
        scene_formula_ids = list(dict.fromkeys(int(value) for value in scene_record["formula_ids"]))
        selected_formula_ids = [
            value
            for value in scene_formula_ids
            if candidate_formula_ids is None or value in candidate_formula_ids
        ]
        if not selected_formula_ids:
            continue
        candidates = candidate_matrix(
            selected_formula_ids,
            bool(scene_record["hybrid"]),
        )
        entries: list[dict[str, Any]] = []
        baseline_image: Path | None = None
        baseline_gpu_ms: float | None = None
        for candidate_name, environment in candidates:
            gpu_times: list[float] = []
            candidate_image: Path | None = None
            metadata: dict[str, Any] | None = None
            error: str | None = None
            stderr_tail = ""
            for run in range(args.runs):
                try:
                    candidate_image, metadata, stderr_tail = render(
                        binary=binary,
                        root=root,
                        scene=scene_path,
                        output=scene_output / candidate_name / f"run-{run + 1}",
                        width=args.width,
                        height=args.height,
                        environment=environment,
                        workload=args.workload,
                        timeout=args.timeout,
                    )
                    gpu_times.append(float(metadata["elapsed_ms"]))
                except (RuntimeError, subprocess.TimeoutExpired) as failure:
                    error = str(failure)
                    break
            median_gpu_ms = median(gpu_times) if gpu_times else None
            changed_pixels = None
            if candidate_name == "baseline" and candidate_image and median_gpu_ms:
                baseline_image = candidate_image
                baseline_gpu_ms = median_gpu_ms
                changed_pixels = 0
            elif baseline_image and candidate_image:
                changed_pixels = image_changed_pixels(baseline_image, candidate_image)
            speedup = (
                baseline_gpu_ms / median_gpu_ms
                if baseline_gpu_ms and median_gpu_ms
                else None
            )
            accepted = bool(
                candidate_name != "baseline"
                and error is None
                and changed_pixels == 0
                and speedup is not None
                and speedup >= args.minimum_speedup
            )
            entry = {
                "candidate": candidate_name,
                "environment": environment,
                "status": "error" if error else "ok",
                "error": error,
                "gpu_ms_runs": gpu_times,
                "median_gpu_ms": median_gpu_ms,
                "speedup": speedup,
                "changed_pixels": changed_pixels,
                "accepted": accepted,
                "source_bytes": metadata.get("mandel_source_bytes") if metadata else None,
                "compile_mode": metadata.get("mandel_compile_mode") if metadata else None,
                "device": metadata.get("metal_device") if metadata else None,
                "stderr_tail": stderr_tail,
            }
            entries.append(entry)
            print(
                f"{corpus_index:04d} {candidate_name}: "
                f"{median_gpu_ms if median_gpu_ms is not None else 'error'} ms, "
                f"{speedup if speedup is not None else 0.0:.3f}x, "
                f"changed={changed_pixels}, accepted={accepted}",
                flush=True,
            )
        accepted = [entry for entry in entries if entry["accepted"]]
        winner = max(accepted, key=lambda entry: entry["speedup"], default=None)
        report["scenes"].append(
            {
                "corpus_index": corpus_index,
                "path": scene_record["path"],
                "formula_ids": scene_record["formula_ids"],
                "winner": winner["candidate"] if winner else "baseline",
                "winner_environment": winner["environment"] if winner else {},
                "winner_speedup": winner["speedup"] if winner else 1.0,
                "entries": entries,
            }
        )
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
