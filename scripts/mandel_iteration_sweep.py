#!/usr/bin/env python3
"""Select and cache Mandel iteration approximations against exact renders."""

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

import numpy as np
from PIL import Image
from skimage.metrics import structural_similarity


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("mandelbulber_root", type=Path)
    parser.add_argument("--binary", type=Path, default=Path("target/release/fpt-metal"))
    parser.add_argument(
        "--cohort", type=Path, default=Path("reports/mandel-optimization/cohort.json")
    )
    parser.add_argument("--indices", default="")
    parser.add_argument(
        "--slowest",
        type=int,
        help="select the N slowest cohort scenes by frozen baseline GPU time",
    )
    parser.add_argument("--cohort-label", default="slow")
    parser.add_argument("--scales", default="0.75,0.80,1.0")
    parser.add_argument("--screen-lod-rates", default="0.5,1.0")
    parser.add_argument(
        "--out", type=Path, default=Path("reports/mandel-iteration-sweep")
    )
    parser.add_argument("--width", type=int, default=160)
    parser.add_argument("--height", type=int, default=90)
    parser.add_argument("--samples", type=int, default=1)
    parser.add_argument("--runs", type=int, default=1)
    parser.add_argument("--minimum-ssim", type=float, default=0.98)
    parser.add_argument("--minimum-speedup", type=float, default=1.05)
    parser.add_argument(
        "--native-confirm-count",
        type=int,
        default=0,
        help="confirm the N fastest qualifying screen candidates at scene-native resolution",
    )
    parser.add_argument("--native-samples", type=int, default=1)
    parser.add_argument("--confirmation-width", type=int)
    parser.add_argument("--confirmation-height", type=int)
    parser.add_argument("--native-timeout", type=float, default=900.0)
    parser.add_argument(
        "--selection-cache",
        type=Path,
        help="write confirmed exact/approximation decisions for --mandel-optimization auto",
    )
    parser.add_argument(
        "--no-profile",
        action="store_true",
        help="do not run the existing ray/formula work profiler on exact candidates",
    )
    parser.add_argument("--timeout", type=float, default=300.0)
    parser.add_argument("--resume", action="store_true")
    return parser.parse_args()


def safe_name(value: str) -> str:
    return re.sub(r"[^A-Za-z0-9_.-]+", "-", Path(value).stem).strip("-")


def image_metrics(baseline: Path, candidate: Path) -> dict[str, float]:
    first = np.asarray(Image.open(baseline).convert("RGB"), dtype=np.float32)
    second = np.asarray(Image.open(candidate).convert("RGB"), dtype=np.float32)
    if first.shape != second.shape:
        raise ValueError(f"image dimensions differ: {first.shape} != {second.shape}")
    difference = first - second
    return {
        "mae": float(np.mean(np.abs(difference))),
        "rmse": float(np.sqrt(np.mean(np.square(difference)))),
        "ssim": float(
            structural_similarity(first, second, channel_axis=2, data_range=255.0)
        ),
    }


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")
    temporary.replace(path)


def profile_summary(
    metadata: dict[str, Any], formula_ids: list[int]
) -> dict[str, Any] | None:
    profile = metadata.get("sdf_profile")
    if not isinstance(profile, dict):
        return None
    slot_iterations = [int(value) for value in profile["formula_slot_iterations"]]
    total_iterations = sum(slot_iterations)
    elapsed_ms = float(metadata["elapsed_ms"])
    slots = []
    for slot, iterations in enumerate(slot_iterations):
        if iterations == 0:
            continue
        share = iterations / total_iterations if total_iterations else 0.0
        slots.append(
            {
                "slot": slot,
                "formula_id": formula_ids[slot] if slot < len(formula_ids) else None,
                "iterations": iterations,
                "iteration_share": share,
                "estimated_gpu_ms": elapsed_ms * share,
            }
        )
    phase_names = profile.get(
        "ray_phases", ["primary", "secondary", "shadow", "normal"]
    )
    phase_iterations = [int(value) for value in profile["orbit_iterations_by_phase"]]
    phase_total = sum(phase_iterations)
    phases = [
        {
            "phase": phase,
            "orbit_iterations": iterations,
            "iteration_share": iterations / phase_total if phase_total else 0.0,
        }
        for phase, iterations in zip(phase_names, phase_iterations, strict=True)
    ]
    return {
        "sampled_pixels": int(profile["sampled_pixels"]),
        "distance_evaluations": int(profile["distance_evaluations"]),
        "march_orbit_iterations": int(profile["march_orbit_iterations"]),
        "average_iterations_per_distance_evaluation": (
            int(profile["march_orbit_iterations"])
            / max(int(profile["distance_evaluations"]), 1)
        ),
        "formula_slots": slots,
        "ray_phases": phases,
        "estimated_phase_ms": profile.get("estimated_ms"),
        "estimate_note": (
            "formula time is apportioned by ray-weighted executed orbit iterations; "
            "it is a prioritization estimate, not an isolated GPU timer"
        ),
    }


def cache_key(scene_sha256: str, width: int, height: int, samples: int) -> str:
    return f"{scene_sha256}:{width}x{height}:{samples}"


def candidate_selection(candidate: dict[str, Any]) -> dict[str, Any]:
    if float(candidate.get("screen_lod_rate", 0.0)) > 0.0:
        return {"kind": "screen_lod", "value": candidate["screen_lod_rate"]}
    if float(candidate.get("iteration_scale", 1.0)) < 1.0:
        return {"kind": "iteration_scale", "value": candidate["iteration_scale"]}
    return {"kind": "exact", "value": 1.0}


def candidates(
    scales: list[float], screen_lod_rates: list[float]
) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = [
        {"name": "exact", "iteration_scale": 1.0, "args": []}
    ]
    for scale in scales:
        if scale == 1.0:
            continue
        result.append(
            {
                "name": f"iteration-{scale:.2f}",
                "iteration_scale": scale,
                "args": ["--mandel-iteration-scale", f"{scale:.4g}"],
            }
        )
    for rate in screen_lod_rates:
        if rate == 0.0:
            continue
        result.append(
            {
                "name": f"screen-lod-{rate:.2f}",
                "iteration_scale": 1.0,
                "screen_lod_rate": rate,
                "args": ["--mandel-screen-lod-rate", f"{rate:.4g}"],
            }
        )
    return result


def render(
    *,
    binary: Path,
    root: Path,
    scene: Path,
    output: Path,
    width: int | None,
    height: int | None,
    samples: int,
    extra_args: list[str],
    timeout: float,
    profile: bool = False,
) -> tuple[Path, dict[str, Any]]:
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
        "--samples",
        str(samples),
        "--sdf-accumulation",
        "batch",
    ]
    if width is not None and height is not None:
        command.extend(["--width", str(width), "--height", str(height)])
    if profile:
        command.append("--sdf-profile")
    command.extend(extra_args)
    environment = os.environ.copy()
    started = time.perf_counter()
    child = subprocess.run(
        command,
        env=environment,
        capture_output=True,
        text=True,
        timeout=timeout,
        check=False,
    )
    if child.returncode != 0:
        raise RuntimeError((child.stdout + "\n" + child.stderr)[-6000:])
    pngs = sorted(output.glob("*.png"))
    if len(pngs) != 1:
        raise RuntimeError(f"expected one PNG in {output}, found {len(pngs)}")
    metadata = json.loads(
        pngs[0].with_suffix(pngs[0].suffix + ".render.json").read_text(
            encoding="utf-8"
        )
    )
    metadata["wall_ms"] = (time.perf_counter() - started) * 1000.0
    return pngs[0], metadata


def main() -> int:
    args = parse_args()
    if (args.confirmation_width is None) != (args.confirmation_height is None):
        raise ValueError("confirmation width and height must be provided together")
    root = args.mandelbulber_root.resolve()
    binary = args.binary.resolve()
    cohort = json.loads(args.cohort.read_text(encoding="utf-8"))
    selected = {int(value) for value in args.indices.split(",") if value.strip()}
    scenes = list(cohort["scenes"])
    if selected:
        scenes = [
            scene for scene in scenes if int(scene["corpus_index"]) in selected
        ]
    else:
        scenes = [scene for scene in scenes if scene.get("cohort") == args.cohort_label]
        scenes.sort(
            key=lambda scene: float(scene.get("baseline_gpu_ms") or 0.0),
            reverse=True,
        )
        if args.slowest is not None:
            if args.slowest < 1:
                raise ValueError("--slowest must be positive")
            scenes = scenes[: args.slowest]
    if not scenes:
        raise ValueError("no selected scenes")
    scales = sorted({float(value) for value in args.scales.split(",")})
    if any(scale < 0.125 or scale > 1.0 for scale in scales):
        raise ValueError("iteration scales must be within 0.125..1")
    screen_lod_rates = sorted(
        {float(value) for value in args.screen_lod_rates.split(",") if value.strip()}
    )
    if any(rate < 0.0 or rate > 8.0 for rate in screen_lod_rates):
        raise ValueError("screen LOD rates must be within 0..8")
    output = args.out.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report: dict[str, Any] = {
        "schema_version": 2,
        "binary": str(binary),
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "mandelbulber_root": str(root),
        "settings": {
            "width": args.width,
            "height": args.height,
            "samples": args.samples,
            "runs": args.runs,
            "minimum_ssim": args.minimum_ssim,
            "minimum_speedup": args.minimum_speedup,
            "iteration_scales": scales,
            "screen_lod_rates": screen_lod_rates,
            "native_confirm_count": args.native_confirm_count,
            "native_samples": args.native_samples,
            "confirmation_width": args.confirmation_width,
            "confirmation_height": args.confirmation_height,
            "profile_exact": not args.no_profile,
        },
        "scenes": [],
        "formula_priorities": [],
    }
    report_path = output / "report.json"
    if args.resume and report_path.is_file():
        report = json.loads(report_path.read_text(encoding="utf-8"))
        report["settings"]["native_confirm_count"] = args.native_confirm_count
        report["settings"]["native_samples"] = args.native_samples
        report["settings"]["confirmation_width"] = args.confirmation_width
        report["settings"]["confirmation_height"] = args.confirmation_height
        report["settings"]["minimum_ssim"] = args.minimum_ssim
        report["settings"]["minimum_speedup"] = args.minimum_speedup
    completed_scenes = {int(scene["corpus_index"]) for scene in report["scenes"]}
    formula_totals: dict[int, dict[str, float | int]] = {}
    for scene_record in scenes:
        index = int(scene_record["corpus_index"])
        if index in completed_scenes:
            print(f"{index:04d} resume screen sweep", flush=True)
            continue
        path = root / scene_record["path"]
        scene_output = output / f"{index:04d}-{safe_name(scene_record['path'])}"
        baseline_image: Path | None = None
        baseline_gpu: float | None = None
        entries: list[dict[str, Any]] = []
        for candidate in candidates(scales, screen_lod_rates):
            gpu_times: list[float] = []
            candidate_image: Path | None = None
            failure: str | None = None
            exact_metadata: dict[str, Any] | None = None
            for run in range(args.runs):
                run_output = scene_output / candidate["name"] / f"run-{run + 1}"
                try:
                    candidate_image, metadata = render(
                        binary=binary,
                        root=root,
                        scene=path,
                        output=run_output,
                        width=args.width,
                        height=args.height,
                        samples=args.samples,
                        extra_args=candidate["args"],
                        timeout=args.timeout,
                        profile=(candidate["name"] == "exact" and not args.no_profile),
                    )
                    gpu_times.append(float(metadata["elapsed_ms"]))
                    if candidate["name"] == "exact":
                        exact_metadata = metadata
                except (RuntimeError, subprocess.TimeoutExpired) as error:
                    failure = str(error)
                    break
            entry: dict[str, Any] = {
                "name": candidate["name"],
                "iteration_scale": candidate["iteration_scale"],
                "screen_lod_rate": candidate.get("screen_lod_rate", 0.0),
                "status": "failed" if failure else "ok",
                "gpu_ms_runs": gpu_times,
                "failure": failure,
            }
            if not failure and candidate_image is not None:
                gpu_ms = median(gpu_times)
                entry.update({"gpu_ms": gpu_ms, "image": str(candidate_image)})
                if candidate["name"] == "exact":
                    baseline_image = candidate_image
                    baseline_gpu = gpu_ms
                    entry["qualifies"] = True
                    entry["profile"] = profile_summary(
                        exact_metadata or {},
                        [int(value) for value in scene_record["formula_ids"]],
                    )
                elif baseline_image is not None and baseline_gpu is not None:
                    metrics = image_metrics(baseline_image, candidate_image)
                    entry.update(
                        {
                            "speedup": baseline_gpu / gpu_ms,
                            "image_metrics": metrics,
                            "qualifies": (
                                metrics["ssim"] >= args.minimum_ssim
                                and baseline_gpu / gpu_ms >= args.minimum_speedup
                            ),
                        }
                    )
            entries.append(entry)
            print(f"{index:04d} {candidate['name']}: {entry['status']}", flush=True)
        qualifying = [
            entry
            for entry in entries
            if entry.get("qualifies") and entry["name"] != "exact" and "speedup" in entry
        ]
        qualifying.sort(key=lambda entry: float(entry["speedup"]), reverse=True)
        exact_profile = entries[0].get("profile")
        if isinstance(exact_profile, dict):
            for slot in exact_profile["formula_slots"]:
                formula_id = slot.get("formula_id")
                if formula_id is None:
                    continue
                aggregate = formula_totals.setdefault(
                    int(formula_id),
                    {"formula_id": int(formula_id), "iterations": 0, "estimated_gpu_ms": 0.0},
                )
                aggregate["iterations"] = int(aggregate["iterations"]) + int(
                    slot["iterations"]
                )
                aggregate["estimated_gpu_ms"] = float(
                    aggregate["estimated_gpu_ms"]
                ) + float(slot["estimated_gpu_ms"])
        report["scenes"].append(
            {
                "corpus_index": index,
                "path": scene_record["path"],
                "formula_ids": scene_record["formula_ids"],
                "hybrid": scene_record.get("hybrid"),
                "frozen_baseline_gpu_ms": scene_record.get("baseline_gpu_ms"),
                "best_qualifying": qualifying[0] if qualifying else None,
                "native_confirmation": None,
                "candidates": entries,
            }
        )
        report["formula_priorities"] = sorted(
            formula_totals.values(),
            key=lambda entry: float(entry["estimated_gpu_ms"]),
            reverse=True,
        )
        write_json(report_path, report)

    confirmation_candidates = [
        scene for scene in report["scenes"] if scene["best_qualifying"] is not None
    ]
    confirmation_candidates.sort(
        key=lambda scene: float(scene["best_qualifying"]["speedup"]), reverse=True
    )
    confirmation_candidates = confirmation_candidates[: args.native_confirm_count]
    cache_path = args.selection_cache.resolve() if args.selection_cache else None
    if cache_path is not None and cache_path.is_file():
        cache = json.loads(cache_path.read_text(encoding="utf-8"))
        if cache.get("schema_version") != 1 or not isinstance(
            cache.get("entries"), dict
        ):
            raise ValueError(f"unsupported selection cache: {cache_path}")
        cache["minimum_ssim"] = max(
            float(cache.get("minimum_ssim", 0.0)), args.minimum_ssim
        )
        cache["minimum_speedup"] = max(
            float(cache.get("minimum_speedup", 0.0)), args.minimum_speedup
        )
    else:
        cache = {
            "schema_version": 1,
            "minimum_ssim": args.minimum_ssim,
            "minimum_speedup": args.minimum_speedup,
            "entries": {},
        }
    for scene_result in confirmation_candidates:
        index = int(scene_result["corpus_index"])
        if scene_result.get("native_confirmation") is not None:
            continue
        path = root / scene_result["path"]
        candidate = scene_result["best_qualifying"]
        native_output = output / f"{index:04d}-{safe_name(scene_result['path'])}" / "native"
        confirmation: dict[str, Any]
        try:
            exact_image, exact_metadata = render(
                binary=binary,
                root=root,
                scene=path,
                output=native_output / "exact",
                width=args.confirmation_width,
                height=args.confirmation_height,
                samples=args.native_samples,
                extra_args=[],
                timeout=args.native_timeout,
            )
            candidate_image, candidate_metadata = render(
                binary=binary,
                root=root,
                scene=path,
                output=native_output / candidate["name"],
                width=args.confirmation_width,
                height=args.confirmation_height,
                samples=args.native_samples,
                extra_args=(
                    ["--mandel-screen-lod-rate", str(candidate["screen_lod_rate"])]
                    if float(candidate.get("screen_lod_rate", 0.0)) > 0.0
                    else ["--mandel-iteration-scale", str(candidate["iteration_scale"])]
                ),
                timeout=args.native_timeout,
            )
            metrics = image_metrics(exact_image, candidate_image)
            exact_gpu = float(exact_metadata["elapsed_ms"])
            candidate_gpu = float(candidate_metadata["elapsed_ms"])
            speedup = exact_gpu / candidate_gpu
            qualifies = (
                metrics["ssim"] >= args.minimum_ssim
                and speedup >= args.minimum_speedup
            )
            selection = candidate_selection(candidate) if qualifies else {
                "kind": "exact",
                "value": 1.0,
            }
            width = int(exact_metadata["width"])
            height = int(exact_metadata["height"])
            scene_sha256 = hashlib.sha256(path.read_bytes()).hexdigest()
            key = cache_key(scene_sha256, width, height, args.native_samples)
            cache["entries"][key] = {
                "scene_sha256": scene_sha256,
                "width": width,
                "height": height,
                "samples": args.native_samples,
                "formula_ids": scene_result["formula_ids"],
                "selection": selection,
                "speedup": speedup,
                "ssim": metrics["ssim"],
                "mae": metrics["mae"],
            }
            confirmation = {
                "status": "ok",
                "candidate": candidate["name"],
                "exact_gpu_ms": exact_gpu,
                "candidate_gpu_ms": candidate_gpu,
                "speedup": speedup,
                "image_metrics": metrics,
                "qualifies": qualifies,
                "selection": selection,
                "width": width,
                "height": height,
                "samples": args.native_samples,
            }
        except (RuntimeError, subprocess.TimeoutExpired) as error:
            confirmation = {"status": "failed", "error": str(error)}
        scene_result["native_confirmation"] = confirmation
        write_json(report_path, report)
        if cache_path is not None:
            write_json(cache_path, cache)

    if cache_path is not None:
        write_json(cache_path, cache)
    write_json(report_path, report)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
