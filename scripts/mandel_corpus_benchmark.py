#!/usr/bin/env python3
"""Benchmark Mandelbulber scenes in isolated Metal-FPT renderer processes."""

from __future__ import annotations

import argparse
import json
import math
import os
import shutil
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from statistics import fmean, median
from typing import Any


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")
    temporary.replace(path)


def percentile(values: list[float], fraction: float) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    position = fraction * (len(ordered) - 1)
    lower = math.floor(position)
    upper = math.ceil(position)
    if lower == upper:
        return ordered[lower]
    weight = position - lower
    return ordered[lower] * (1.0 - weight) + ordered[upper] * weight


def markdown_cell(value: object) -> str:
    return str(value).replace("|", "\\|").replace("\n", " ")


def ranking_markdown(
    entries: list[dict[str, Any]], settings: dict[str, object]
) -> str:
    render_mode = str(settings.get("render_mode", "pathtrace"))
    workload = (
        f'{settings["samples"]} spp'
        if render_mode == "pathtrace"
        else "one primary ray and one geometric-normal evaluation per pixel"
    )
    lines = [
        "# Mandelbulber corpus GPU ranking",
        "",
        (
            f'Fixed settings: {settings["width"]}x{settings["height"]}, '
            f'{workload}, `{settings["renderer"]}` renderer, '
            f'`{render_mode}` workload. '
            "GPU ms is command-buffer execution time; FPS is `1000 / GPU ms`."
        ),
        "",
        "| Rank | Corpus | Scene | GPU ms | FPS | Build ms | Cache |",
        "| ---: | ---: | :--- | ---: | ---: | ---: | :--- |",
    ]
    for rank, entry in enumerate(entries, start=1):
        scene = markdown_cell(Path(entry["path"]).name)
        lines.append(
            f'| {rank} | {entry["corpus_index"]:04d} | {scene} | '
            f'{entry["gpu_ms"]:.3f} | {entry["fps"]:.2f} | '
            f'{entry["build_ms"]:.3f} | {entry["cache_status"]} |'
        )
    return "\n".join(lines) + "\n"


def ranking_tsv(entries: list[dict[str, Any]]) -> str:
    lines = ["rank\tcorpus_index\tpath\tgpu_ms\tfps\tbuild_ms\tcache_status"]
    for rank, entry in enumerate(entries, start=1):
        lines.append(
            f'{rank}\t{entry["corpus_index"]}\t{entry["path"]}\t'
            f'{entry["gpu_ms"]:.6f}\t{entry["fps"]:.6f}\t'
            f'{entry["build_ms"]:.6f}\t{entry["cache_status"]}'
        )
    return "\n".join(lines) + "\n"


def aggregate_report(
    *,
    audit: dict[str, Any],
    entries: list[dict[str, Any]],
    settings: dict[str, object],
    started_at: str,
    elapsed_seconds: float,
) -> dict[str, Any]:
    successful = [entry for entry in entries if entry["status"] == "ok"]
    failed = [entry for entry in entries if entry["status"] == "failed"]
    gpu_values = [float(entry["gpu_ms"]) for entry in successful]
    wall_values = [float(entry["wall_ms"]) for entry in successful]
    devices = sorted(
        {str(entry["metal_device"]) for entry in successful if entry.get("metal_device")}
    )
    ranked = sorted(
        successful,
        key=lambda entry: (float(entry["gpu_ms"]), str(entry["path"])),
        reverse=True,
    )
    return {
        "schema_version": 1,
        "started_at": started_at,
        "updated_at": datetime.now(timezone.utc).isoformat(),
        "elapsed_seconds": elapsed_seconds,
        "settings": settings,
        "metal_devices": devices,
        "corpus_scenes": int(audit["scenes"]),
        "eligible_scenes": int(audit["generated"]),
        "audit_generation_failures": int(audit["generation_failed"]),
        "attempted_scenes": len(entries),
        "successful_scenes": len(successful),
        "failed_scenes": len(failed),
        "summary": {
            "gpu_ms_min": min(gpu_values) if gpu_values else None,
            "gpu_ms_median": median(gpu_values) if gpu_values else None,
            "gpu_ms_mean": fmean(gpu_values) if gpu_values else None,
            "gpu_ms_p90": percentile(gpu_values, 0.90),
            "gpu_ms_p95": percentile(gpu_values, 0.95),
            "gpu_ms_max": max(gpu_values) if gpu_values else None,
            "wall_ms_median": median(wall_values) if wall_values else None,
        },
        "ranking_order": "gpu_ms_descending",
        "ranked_entries": ranked,
        "failed_entries": failed,
        "entries": entries,
    }


def persist_outputs(output_dir: Path, report: dict[str, Any]) -> None:
    ranked = report["ranked_entries"]
    assert isinstance(ranked, list)
    write_json(output_dir / "report.json", report)
    (output_dir / "gpu-ranking.tsv").write_text(
        ranking_tsv(ranked), encoding="utf-8"
    )
    (output_dir / "gpu-ranking.md").write_text(
        ranking_markdown(ranked, report["settings"]), encoding="utf-8"
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Render a Mandelbulber scene corpus sequentially, one child process per "
            "scene, and rank command-buffer GPU time."
        )
    )
    parser.add_argument("mandelbulber_root", type=Path)
    parser.add_argument("--binary", type=Path, default=Path("target/release/fpt-metal"))
    parser.add_argument(
        "--audit",
        type=Path,
        default=Path("reports/mandel-benchmark/full-corpus-scene-audit.json"),
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=Path("reports/mandel-benchmark/full-corpus-120x68-1spp"),
    )
    parser.add_argument("--width", type=int, default=120)
    parser.add_argument("--height", type=int, default=68)
    parser.add_argument("--samples", type=int, default=1)
    parser.add_argument(
        "--sdf-accumulation",
        choices=("auto", "per-sample", "batch", "chunked"),
        default="auto",
        help="SDF sample accumulation dispatch mode",
    )
    parser.add_argument(
        "--render-mode",
        choices=("pathtrace", "diffuse-normal"),
        default="pathtrace",
        help=(
            "pathtrace runs the ordinary renderer; diffuse-normal runs one primary "
            "march and one geometric-normal evaluation without shading paths"
        ),
    )
    parser.add_argument("--offset", type=int, default=0)
    parser.add_argument("--limit", type=int)
    parser.add_argument(
        "--homogeneous-hybrids",
        action="store_true",
        help="benchmark generated hybrid scenes with exactly one active formula",
    )
    parser.add_argument(
        "--cohort",
        type=Path,
        help="optional cohort JSON whose scenes[].corpus_index selects the run",
    )
    parser.add_argument("--timeout", type=float, default=120.0)
    parser.add_argument("--retry-failed", action="store_true")
    parser.add_argument(
        "--no-tiled-watchdog-retry",
        action="store_true",
        help="do not retry GPU-watchdog failures in 32-row command buffers",
    )
    parser.add_argument(
        "--gpu-archiver-cache",
        type=Path,
        help=(
            "exact com.apple.gpuarchiver temporary directory to purge between "
            "isolated scenes; use only for a confirmed regenerable cache"
        ),
    )
    parser.add_argument("--print-rows", type=int, default=30)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    root = args.mandelbulber_root.resolve()
    binary = args.binary.resolve()
    audit_path = args.audit.resolve()
    output_dir = args.out.resolve()
    if not binary.is_file():
        raise FileNotFoundError(f"renderer binary not found: {binary}")
    if not root.is_dir():
        raise FileNotFoundError(f"Mandelbulber root not found: {root}")
    if not audit_path.is_file():
        audit_path.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(
            [
                str(binary),
                "mandel-scene-audit",
                str(root),
                "--report",
                str(audit_path),
            ],
            check=True,
        )

    audit = json.loads(audit_path.read_text(encoding="utf-8"))
    eligible = [
        (index, entry)
        for index, entry in enumerate(audit["entries"], start=1)
        if entry["generated"]
    ]
    if args.homogeneous_hybrids:
        eligible = [
            (index, entry)
            for index, entry in eligible
            if entry.get("hybrid") and len(entry.get("formula_ids", [])) == 1
        ]
    if args.cohort is not None:
        cohort = json.loads(args.cohort.resolve().read_text(encoding="utf-8"))
        cohort_indices = {
            int(scene["corpus_index"]) for scene in cohort.get("scenes", [])
        }
        eligible = [
            (index, entry)
            for index, entry in eligible
            if index in cohort_indices
        ]
        missing = cohort_indices.difference(index for index, _ in eligible)
        if missing:
            raise ValueError(
                "cohort contains ineligible or unknown corpus indices: "
                + ", ".join(str(index) for index in sorted(missing))
            )
    if args.offset < 0 or args.offset > len(eligible):
        raise ValueError("--offset must be within the eligible scene corpus")
    selected = eligible[args.offset :]
    if args.limit is not None:
        if args.limit < 0:
            raise ValueError("--limit must be non-negative")
        selected = selected[: args.limit]
    if args.width < 16 or args.height < 16 or args.samples < 1:
        raise ValueError("width/height must be at least 16 and samples must be positive")
    if args.timeout <= 0:
        raise ValueError("--timeout must be positive")

    output_dir.mkdir(parents=True, exist_ok=True)
    entries_dir = output_dir / "entries"
    entries_dir.mkdir(parents=True, exist_ok=True)
    cache_dir = output_dir / "cache"
    process_temp_dir = output_dir / "process-temporary"
    process_temp_dir.mkdir(parents=True, exist_ok=True)
    gpu_archiver_cache = (
        args.gpu_archiver_cache.resolve() if args.gpu_archiver_cache else None
    )
    if gpu_archiver_cache is not None and gpu_archiver_cache.name != "com.apple.gpuarchiver":
        raise ValueError("--gpu-archiver-cache must name com.apple.gpuarchiver exactly")
    images_dir = output_dir / "images"
    settings: dict[str, object] = {
        "renderer": "sdf",
        "render_mode": args.render_mode,
        "width": args.width,
        "height": args.height,
        "samples": args.samples,
        "sdf_accumulation": args.sdf_accumulation,
        "process_isolated": True,
        "sequential": True,
        "timeout_seconds": args.timeout,
        "tiled_watchdog_retry": (
            args.render_mode == "pathtrace" and not args.no_tiled_watchdog_retry
        ),
        "diagnostic_compile_policy": (
            "production-scene-policy"
            if args.render_mode == "diffuse-normal"
            else None
        ),
        "fps_definition": "1000 / command-buffer GPU milliseconds",
        "mandelbulber_root": str(root),
        "renderer_binary": str(binary),
        "cohort": str(args.cohort.resolve()) if args.cohort else None,
        "cache_directory": str(cache_dir),
        "process_temporary_directory": str(process_temp_dir),
        "purged_gpu_archiver_cache": (
            str(gpu_archiver_cache) if gpu_archiver_cache is not None else None
        ),
    }
    started = time.perf_counter()
    started_at = datetime.now(timezone.utc).isoformat()
    completed: dict[int, dict[str, Any]] = {}
    for entry_path in sorted(entries_dir.glob("*.json")):
        prior = json.loads(entry_path.read_text(encoding="utf-8"))
        if prior.get("status") == "ok" or not args.retry_failed:
            completed[int(prior["corpus_index"])] = prior

    environment = os.environ.copy()
    environment["FPT_MANDEL_RENDER_CACHE_DIR"] = str(cache_dir)
    environment["TMPDIR"] = str(process_temp_dir) + os.sep
    if args.render_mode == "diffuse-normal":
        environment.pop("FPT_MANDEL_DIAGNOSTIC_OPTIMIZED", None)
        environment["FPT_MANDEL_DIAGNOSTIC_CACHE_DIR"] = str(cache_dir)
        environment["FPT_MANDEL_DIAGNOSTIC_PRODUCTION_COMPILE"] = "1"

    total = len(selected)
    for position, (corpus_index, audit_entry) in enumerate(selected, start=1):
        if corpus_index in completed:
            print(
                f"[{position:04d}/{total:04d}] resume {corpus_index:04d} "
                f'{audit_entry["path"]}',
                flush=True,
            )
            continue
        scene = root / audit_entry["path"]
        scene_dir = images_dir / f"{corpus_index:04d}"
        scene_dir.mkdir(parents=True, exist_ok=True)
        command = [str(binary)]
        if args.render_mode == "pathtrace":
            command.extend(["render", str(scene)])
        else:
            command.extend(
                ["diagnostic", str(scene), "--mode", "diffuse-normal"]
            )
        command.extend(
            [
                "--out",
                str(scene_dir),
                "--renderer",
                "sdf",
                "--mandelbulber-root",
                str(root),
                "--width",
                str(args.width),
                "--height",
                str(args.height),
                "--samples",
                str(args.samples),
            ]
        )
        if args.sdf_accumulation != "auto":
            command.extend(["--sdf-accumulation", args.sdf_accumulation])
        print(
            f"[{position:04d}/{total:04d}] {args.render_mode} {corpus_index:04d} "
            f'{audit_entry["path"]}',
            flush=True,
        )
        wall_started = time.perf_counter()
        try:
            child = subprocess.run(
                command,
                env=environment,
                capture_output=True,
                text=True,
                timeout=args.timeout,
                check=False,
            )
            dispatch_mode = "single-command-buffer"
            watchdog_retry = False
            recovery_retry = False
            if (
                child.returncode != 0
                and args.render_mode == "pathtrace"
                and not args.no_tiled_watchdog_retry
                and (
                    "Impacting Interactivity" in child.stderr
                    or "victim of GPU error/recovery" in child.stderr
                )
            ):
                retry_environment = environment.copy()
                watchdog_retry = "Impacting Interactivity" in child.stderr
                recovery_retry = not watchdog_retry
                if watchdog_retry:
                    retry_environment["FPT_MANDEL_TILED_DISPATCH"] = "1"
                child = subprocess.run(
                    command,
                    env=retry_environment,
                    capture_output=True,
                    text=True,
                    timeout=args.timeout,
                    check=False,
                )
                dispatch_mode = (
                    "tiled-watchdog-retry"
                    if watchdog_retry
                    else "single-command-buffer-recovery-retry"
                )
            wall_ms = (time.perf_counter() - wall_started) * 1000.0
            if child.returncode != 0:
                raise RuntimeError(
                    f"renderer exited {child.returncode}: "
                    f"{child.stderr[-4000:].strip()}"
                )
            metadata_paths = sorted(
                scene_dir.glob("*.png.render.json"),
                key=lambda path: path.stat().st_mtime_ns,
                reverse=True,
            )
            if not metadata_paths:
                raise RuntimeError("renderer produced no .png.render.json metadata")
            metadata_path = metadata_paths[0]
            metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
            gpu_ms = float(metadata["elapsed_ms"])
            if not math.isfinite(gpu_ms) or gpu_ms <= 0.0:
                raise RuntimeError(f"invalid GPU time: {gpu_ms}")
            record: dict[str, Any] = {
                "corpus_index": corpus_index,
                "path": audit_entry["path"],
                "hybrid": audit_entry.get("hybrid"),
                "formula_ids": audit_entry.get("formula_ids", []),
                "status": "ok",
                "gpu_ms": gpu_ms,
                "fps": 1000.0 / gpu_ms,
                "build_ms": float(metadata.get("acceleration_build_ms") or 0.0),
                "wall_ms": wall_ms,
                "cache_status": metadata.get("sdf_stitch_cache_status", "unknown"),
                "render_mode": args.render_mode,
                "compile_mode": metadata.get("mandel_compile_mode"),
                "formula_dispatch_mode": metadata.get(
                    "mandel_formula_dispatch_mode"
                ),
                "dispatch_mode": dispatch_mode,
                "watchdog_retry": watchdog_retry,
                "recovery_retry": recovery_retry,
                "metal_device": metadata.get("metal_device"),
                "output": metadata.get("output"),
                "metadata": str(metadata_path),
                "stderr_tail": child.stderr[-2000:],
                "error": None,
            }
        except subprocess.TimeoutExpired as error:
            wall_ms = (time.perf_counter() - wall_started) * 1000.0
            record = {
                "corpus_index": corpus_index,
                "path": audit_entry["path"],
                "hybrid": audit_entry.get("hybrid"),
                "formula_ids": audit_entry.get("formula_ids", []),
                "status": "failed",
                "wall_ms": wall_ms,
                "error": f"renderer timed out after {error.timeout:.1f} seconds",
            }
        except Exception as error:
            wall_ms = (time.perf_counter() - wall_started) * 1000.0
            record = {
                "corpus_index": corpus_index,
                "path": audit_entry["path"],
                "hybrid": audit_entry.get("hybrid"),
                "formula_ids": audit_entry.get("formula_ids", []),
                "status": "failed",
                "wall_ms": wall_ms,
                "error": str(error),
            }
        write_json(entries_dir / f"{corpus_index:04d}.json", record)
        owned_archiver_cache = process_temp_dir / "C" / "com.apple.gpuarchiver"
        shutil.rmtree(owned_archiver_cache, ignore_errors=True)
        if gpu_archiver_cache is not None:
            shutil.rmtree(gpu_archiver_cache, ignore_errors=True)
        completed[corpus_index] = record
        selected_entries = [
            completed[index]
            for index, _ in selected
            if index in completed
        ]
        report = aggregate_report(
            audit=audit,
            entries=selected_entries,
            settings=settings,
            started_at=started_at,
            elapsed_seconds=time.perf_counter() - started,
        )
        persist_outputs(output_dir, report)
        if record["status"] == "ok":
            print(
                f'  GPU {record["gpu_ms"]:.3f} ms | {record["fps"]:.2f} FPS | '
                f'wall {record["wall_ms"]:.1f} ms',
                flush=True,
            )
        else:
            print(f'  FAILED: {record["error"]}', file=sys.stderr, flush=True)

    final_entries = [completed[index] for index, _ in selected if index in completed]
    report = aggregate_report(
        audit=audit,
        entries=final_entries,
        settings=settings,
        started_at=started_at,
        elapsed_seconds=time.perf_counter() - started,
    )
    persist_outputs(output_dir, report)
    ranked = report["ranked_entries"]
    print()
    print(
        ranking_markdown(ranked[: max(args.print_rows, 0)], settings),
        end="",
    )
    print(
        f'Completed: {report["successful_scenes"]}/{report["attempted_scenes"]} '
        f'succeeded; full ranking: {output_dir / "gpu-ranking.md"}'
    )
    return 1 if report["failed_scenes"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
