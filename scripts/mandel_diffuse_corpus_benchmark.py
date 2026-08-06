#!/usr/bin/env python3
"""Benchmark diffuse-normal Mandel scenes with parallel CPU setup and serial GPU work."""

from __future__ import annotations

import argparse
import json
import math
import os
import subprocess
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from mandel_corpus_benchmark import aggregate_report, persist_outputs, write_json


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Compile diagnostic pipelines concurrently, serialize their timed Metal "
            "command buffers, and rank geometry-only Mandel performance."
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
        default=Path(
            "reports/mandel-optimization/diffuse-normal-full-corpus-120x68"
        ),
    )
    parser.add_argument("--width", type=int, default=120)
    parser.add_argument("--height", type=int, default=68)
    parser.add_argument("--workers", type=int, default=1)
    parser.add_argument("--chunk-size", type=int, default=5)
    parser.add_argument("--offset", type=int, default=0)
    parser.add_argument("--limit", type=int)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--retry-failed", action="store_true")
    return parser.parse_args()


def diagnostic_job(
    *, scene: Path, output: Path, root: Path, width: int, height: int
) -> list[str]:
    return [
        str(scene),
        "--out",
        str(output),
        "--renderer",
        "sdf",
        "--mode",
        "diffuse-normal",
        "--mandelbulber-root",
        str(root),
        "--width",
        str(width),
        "--height",
        str(height),
        "--samples",
        "1",
    ]


def main() -> int:
    args = parse_args()
    root = args.mandelbulber_root.resolve()
    binary = args.binary.resolve()
    audit_path = args.audit.resolve()
    output_dir = args.out.resolve()
    if not root.is_dir() or not binary.is_file() or not audit_path.is_file():
        raise FileNotFoundError("Mandel root, renderer binary, or audit report is missing")
    if args.width < 16 or args.height < 16:
        raise ValueError("width and height must be at least 16")
    if not 1 <= args.workers <= 8:
        raise ValueError("workers must be between 1 and 8")
    if args.chunk_size < args.workers:
        raise ValueError("chunk size must be at least the worker count")
    if args.timeout <= 0:
        raise ValueError("timeout must be positive")

    audit = json.loads(audit_path.read_text(encoding="utf-8"))
    eligible = [
        (index, entry)
        for index, entry in enumerate(audit["entries"], start=1)
        if entry["generated"]
    ]
    selected = eligible[args.offset :]
    if args.limit is not None:
        selected = selected[: args.limit]

    entries_dir = output_dir / "entries"
    images_dir = output_dir / "images"
    cache_dir = output_dir / "cache"
    process_temp_dir = output_dir / "process-temporary"
    for directory in (entries_dir, images_dir, cache_dir, process_temp_dir):
        directory.mkdir(parents=True, exist_ok=True)

    completed: dict[int, dict[str, Any]] = {}
    for entry_path in sorted(entries_dir.glob("*.json")):
        entry = json.loads(entry_path.read_text(encoding="utf-8"))
        if entry.get("status") == "ok" or not args.retry_failed:
            completed[int(entry["corpus_index"])] = entry
    pending = [item for item in selected if item[0] not in completed]

    settings: dict[str, object] = {
        "renderer": "sdf",
        "render_mode": "diffuse-normal",
        "width": args.width,
        "height": args.height,
        "samples": 1,
        "process_isolated": False,
        "sequential": False,
        "parallel_cpu_pipeline_setup": True,
        "serialized_gpu_command_buffers": True,
        "workers": args.workers,
        "chunk_size": args.chunk_size,
        "timeout_seconds_per_scene": args.timeout,
        "tiled_watchdog_retry": False,
        "diagnostic_compile_policy": "production-scene-policy",
        "fps_definition": "1000 / serialized command-buffer GPU milliseconds",
        "mandelbulber_root": str(root),
        "renderer_binary": str(binary),
        "cache_directory": str(cache_dir),
        "process_temporary_directory": str(process_temp_dir),
    }
    environment = os.environ.copy()
    environment.pop("FPT_MANDEL_DIAGNOSTIC_OPTIMIZED", None)
    environment["FPT_MANDEL_DIAGNOSTIC_CACHE_DIR"] = str(cache_dir)
    environment["FPT_MANDEL_DIAGNOSTIC_PRODUCTION_COMPILE"] = "1"
    environment["FPT_SERIALIZE_DIAGNOSTIC_GPU"] = "1"
    environment["TMPDIR"] = str(process_temp_dir) + os.sep

    started = time.perf_counter()
    started_at = datetime.now(timezone.utc).isoformat()
    jobs_path = output_dir / "active-jobs.json"
    batch_report_path = output_dir / "active-batch-report.json"
    for chunk_offset in range(0, len(pending), args.chunk_size):
        chunk = pending[chunk_offset : chunk_offset + args.chunk_size]
        jobs = [
            diagnostic_job(
                scene=root / audit_entry["path"],
                output=images_dir / f"{corpus_index:04d}",
                root=root,
                width=args.width,
                height=args.height,
            )
            for corpus_index, audit_entry in chunk
        ]
        write_json(jobs_path, jobs)
        command = [
            str(binary),
            "diagnostic-batch",
            str(jobs_path),
            "--workers",
            str(args.workers),
            "--report",
            str(batch_report_path),
        ]
        batch_report_path.unlink(missing_ok=True)
        batch_timeout = args.timeout * math.ceil(len(chunk) / args.workers)
        try:
            child = subprocess.run(
                command,
                env=environment,
                capture_output=True,
                text=True,
                timeout=batch_timeout,
                check=False,
            )
            batch_stderr = child.stderr[-4000:]
        except subprocess.TimeoutExpired as error:
            batch_stderr = f"batch timed out after {error.timeout:.1f} seconds"

        batch_entries: dict[int, dict[str, Any]] = {}
        if batch_report_path.is_file():
            batch_report = json.loads(batch_report_path.read_text(encoding="utf-8"))
            batch_entries = {
                int(entry["index"]) - 1: entry for entry in batch_report["entries"]
            }
        for local_index, (corpus_index, audit_entry) in enumerate(chunk):
            scene_dir = images_dir / f"{corpus_index:04d}"
            metadata_paths = sorted(
                scene_dir.glob("*.png.render.json"),
                key=lambda path: path.stat().st_mtime_ns,
                reverse=True,
            )
            batch_entry = batch_entries.get(local_index, {})
            try:
                # A compiler-service timeout can prevent diagnostic-batch from
                # writing its coordinator report after other jobs have already
                # completed. A render metadata file is the stronger success
                # signal because diagnostic publishes it only after the GPU
                # command buffer and PNG both succeed.
                if not metadata_paths:
                    raise RuntimeError(batch_entry.get("error") or batch_stderr)
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
                    "build_ms": 0.0,
                    "wall_ms": float(batch_entry.get("elapsed_ms") or 0.0),
                    "cache_status": metadata.get(
                        "sdf_stitch_cache_status", "unknown"
                    ),
                    "render_mode": "diffuse-normal",
                    "compile_mode": metadata.get("mandel_compile_mode"),
                    "formula_dispatch_mode": metadata.get(
                        "mandel_formula_dispatch_mode"
                    ),
                    "dispatch_mode": "serialized-diagnostic-batch",
                    "watchdog_retry": False,
                    "recovery_retry": False,
                    "metal_device": metadata.get("metal_device"),
                    "output": metadata.get("output"),
                    "metadata": str(metadata_path),
                    "stderr_tail": batch_stderr[-2000:],
                    "error": None,
                }
            except Exception as error:
                record = {
                    "corpus_index": corpus_index,
                    "path": audit_entry["path"],
                    "hybrid": audit_entry.get("hybrid"),
                    "formula_ids": audit_entry.get("formula_ids", []),
                    "status": "failed",
                    "wall_ms": float(batch_entry.get("elapsed_ms") or 0.0),
                    "error": str(error),
                }
            write_json(entries_dir / f"{corpus_index:04d}.json", record)
            completed[corpus_index] = record

        selected_entries = [
            completed[index] for index, _ in selected if index in completed
        ]
        report = aggregate_report(
            audit=audit,
            entries=selected_entries,
            settings=settings,
            started_at=started_at,
            elapsed_seconds=time.perf_counter() - started,
        )
        persist_outputs(output_dir, report)
        print(
            f"Diffuse batch {min(chunk_offset + len(chunk), len(pending))}/"
            f"{len(pending)}: {report['successful_scenes']} successful, "
            f"{report['failed_scenes']} failed",
            flush=True,
        )

    jobs_path.unlink(missing_ok=True)
    batch_report_path.unlink(missing_ok=True)
    final_entries = [completed[index] for index, _ in selected if index in completed]
    report = aggregate_report(
        audit=audit,
        entries=final_entries,
        settings=settings,
        started_at=started_at,
        elapsed_seconds=time.perf_counter() - started,
    )
    persist_outputs(output_dir, report)
    return 1 if report["failed_scenes"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
