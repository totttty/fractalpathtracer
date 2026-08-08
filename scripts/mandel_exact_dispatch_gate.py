#!/usr/bin/env python3
"""Repeatedly gate exact Mandel dispatch specializations and confirm winners natively."""

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


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("mandelbulber_root", type=Path)
    parser.add_argument("--production-binary", type=Path, required=True)
    parser.add_argument("--candidate-binary", type=Path, required=True)
    parser.add_argument(
        "--candidate-env",
        action="append",
        default=[],
        metavar="NAME=VALUE",
        help="environment override applied only to candidate renders",
    )
    parser.add_argument("--screening-comparison", type=Path, required=True)
    parser.add_argument("--compiler-source", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--width", type=int, default=120)
    parser.add_argument("--height", type=int, default=68)
    parser.add_argument("--samples", type=int, default=1)
    parser.add_argument("--screen-runs", type=int, default=3)
    parser.add_argument("--minimum-screen-speedup", type=float, default=1.25)
    parser.add_argument("--minimum-native-speedup", type=float, default=1.10)
    parser.add_argument("--native-confirm-count", type=int, default=0)
    parser.add_argument("--native-runs", type=int, default=1)
    parser.add_argument("--confirmation-width", type=int)
    parser.add_argument("--confirmation-height", type=int)
    parser.add_argument("--timeout", type=float, default=300.0)
    parser.add_argument("--native-timeout", type=float, default=1200.0)
    parser.add_argument("--tile-rows", type=int, default=4)
    parser.add_argument("--resume", action="store_true")
    return parser.parse_args()


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")
    temporary.replace(path)


def environment_overrides(values: list[str]) -> dict[str, str]:
    overrides: dict[str, str] = {}
    for value in values:
        name, separator, setting = value.partition("=")
        if not separator or not name:
            raise ValueError(f"candidate environment must use NAME=VALUE: {value}")
        overrides[name] = setting
    return overrides


def safe_name(path: str) -> str:
    return re.sub(r"[^A-Za-z0-9_.-]+", "-", Path(path).stem).strip("-")


def direct_hashes(source: str) -> set[str]:
    marker = "const DIRECT_HYBRID_SCENES"
    if marker not in source:
        raise ValueError("compiler source has no DIRECT_HYBRID_SCENES table")
    block = source.split(marker, 1)[1].split("];", 1)[0]
    return set(re.findall(r'"([0-9a-f]{64})"', block))


def changed_pixels(first_path: Path, second_path: Path) -> int:
    with Image.open(first_path) as first_image, Image.open(second_path) as second_image:
        first = first_image.convert("RGB")
        second = second_image.convert("RGB")
        if first.size != second.size:
            raise ValueError(f"image sizes differ: {first.size} != {second.size}")
        difference = ImageChops.difference(first, second)
        return sum(1 for pixel in difference.getdata() if pixel != (0, 0, 0))


def render(
    *,
    binary: Path,
    root: Path,
    scene: Path,
    output: Path,
    width: int | None,
    height: int | None,
    samples: int,
    timeout: float,
    cache: Path,
    temporary: Path,
    tile_rows: int | None,
    extra_environment: dict[str, str] | None = None,
) -> tuple[Path, dict[str, Any]]:
    output.mkdir(parents=True, exist_ok=True)
    cache.mkdir(parents=True, exist_ok=True)
    temporary.mkdir(parents=True, exist_ok=True)
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
    environment = os.environ.copy()
    environment.update(extra_environment or {})
    environment["FPT_MANDEL_RENDER_CACHE_DIR"] = str(cache)
    environment["TMPDIR"] = str(temporary) + os.sep
    if tile_rows is not None:
        environment["FPT_MANDEL_TILED_DISPATCH"] = "1"
        environment["FPT_MANDEL_TILE_ROWS"] = str(tile_rows)
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
    metadata_paths = sorted(output.glob("*.png.render.json"))
    if len(metadata_paths) != 1:
        raise RuntimeError(f"expected one render metadata file in {output}")
    metadata = json.loads(metadata_paths[0].read_text(encoding="utf-8"))
    metadata["wall_ms"] = (time.perf_counter() - started) * 1000.0
    image = metadata_paths[0].with_suffix("").with_suffix("")
    if not image.is_file():
        raise RuntimeError(f"render metadata references missing image: {image}")
    return image, metadata


def paired_runs(
    *,
    root: Path,
    scene: Path,
    output: Path,
    production_binary: Path,
    candidate_binary: Path,
    width: int | None,
    height: int | None,
    samples: int,
    runs: int,
    timeout: float,
    tile_rows: int | None,
    candidate_environment: dict[str, str] | None = None,
) -> dict[str, Any]:
    production_times: list[float] = []
    candidate_times: list[float] = []
    differences: list[int] = []
    failure: str | None = None
    for run in range(1, runs + 1):
        try:
            production_image, production_metadata = render(
                binary=production_binary,
                root=root,
                scene=scene,
                output=output / "production" / f"run-{run}",
                width=width,
                height=height,
                samples=samples,
                timeout=timeout,
                cache=output.parent / "cache-production",
                temporary=output.parent / "tmp-production",
                tile_rows=tile_rows,
                extra_environment={},
            )
            candidate_image, candidate_metadata = render(
                binary=candidate_binary,
                root=root,
                scene=scene,
                output=output / "direct" / f"run-{run}",
                width=width,
                height=height,
                samples=samples,
                timeout=timeout,
                cache=output.parent / "cache-direct",
                temporary=output.parent / "tmp-direct",
                tile_rows=tile_rows,
                extra_environment=candidate_environment,
            )
            production_times.append(float(production_metadata["elapsed_ms"]))
            candidate_times.append(float(candidate_metadata["elapsed_ms"]))
            differences.append(changed_pixels(production_image, candidate_image))
        except (RuntimeError, subprocess.TimeoutExpired) as error:
            failure = str(error)
            break
    result: dict[str, Any] = {
        "status": "failed" if failure else "ok",
        "failure": failure,
        "production_gpu_ms_runs": production_times,
        "direct_gpu_ms_runs": candidate_times,
        "changed_pixels_runs": differences,
    }
    if not failure:
        production_gpu = median(production_times)
        direct_gpu = median(candidate_times)
        result.update(
            {
                "production_gpu_ms": production_gpu,
                "direct_gpu_ms": direct_gpu,
                "speedup": production_gpu / direct_gpu,
                "pixel_exact": all(value == 0 for value in differences),
                "gpu_ms_saved": production_gpu - direct_gpu,
            }
        )
    return result


def main() -> int:
    args = parse_args()
    if (args.confirmation_width is None) != (args.confirmation_height is None):
        raise ValueError("confirmation width and height must be provided together")
    root = args.mandelbulber_root.resolve()
    production_binary = args.production_binary.resolve()
    candidate_binary = args.candidate_binary.resolve()
    candidate_environment = environment_overrides(args.candidate_env)
    comparison = json.loads(args.screening_comparison.read_text(encoding="utf-8"))
    production_hashes = direct_hashes(args.compiler_source.read_text(encoding="utf-8"))
    output = args.out.resolve()
    output.mkdir(parents=True, exist_ok=True)
    selected: list[dict[str, Any]] = []
    for entry in comparison["entries"]:
        path = root / entry["path"]
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        if digest in production_hashes:
            continue
        if entry.get("image", {}).get("changed_pixels") != 0:
            continue
        if float(entry.get("speedup", 0.0)) < args.minimum_screen_speedup:
            continue
        selected.append({**entry, "scene_sha256": digest})
    selected.sort(key=lambda entry: int(entry["corpus_index"]))
    report_path = output / "report.json"
    report: dict[str, Any] = {
        "schema_version": 1,
        "mandelbulber_root": str(root),
        "production_binary": str(production_binary),
        "production_binary_sha256": hashlib.sha256(production_binary.read_bytes()).hexdigest(),
        "candidate_binary": str(candidate_binary),
        "candidate_binary_sha256": hashlib.sha256(candidate_binary.read_bytes()).hexdigest(),
        "candidate_environment": candidate_environment,
        "screening_comparison": str(args.screening_comparison.resolve()),
        "settings": {
            "width": args.width,
            "height": args.height,
            "samples": args.samples,
            "screen_runs": args.screen_runs,
            "minimum_screen_speedup": args.minimum_screen_speedup,
            "minimum_native_speedup": args.minimum_native_speedup,
            "native_confirm_count": args.native_confirm_count,
            "native_runs": args.native_runs,
            "confirmation_width": args.confirmation_width,
            "confirmation_height": args.confirmation_height,
            "tile_rows": args.tile_rows,
        },
        "screened_scenes": len(comparison["entries"]),
        "repeated_candidates": len(selected),
        "scenes": [],
        "accepted_hashes": [],
    }
    if args.resume and report_path.is_file():
        report = json.loads(report_path.read_text(encoding="utf-8"))
        report["settings"]["native_confirm_count"] = args.native_confirm_count
        report["settings"]["native_runs"] = args.native_runs
        report["settings"]["confirmation_width"] = args.confirmation_width
        report["settings"]["confirmation_height"] = args.confirmation_height
        report["settings"]["minimum_native_speedup"] = args.minimum_native_speedup
        report["accepted_hashes"] = []
    completed_screens = {
        int(scene["corpus_index"]) for scene in report["scenes"] if scene.get("screen")
    }
    for position, entry in enumerate(selected, start=1):
        index = int(entry["corpus_index"])
        if index in completed_screens:
            print(
                f"[{position:02d}/{len(selected):02d}] resume repeated gate {index:04d} {entry['path']}",
                flush=True,
            )
            continue
        print(f"[{position:02d}/{len(selected):02d}] repeated gate {index:04d} {entry['path']}", flush=True)
        screen = paired_runs(
            root=root,
            scene=root / entry["path"],
            output=output / "screen" / f"{index:04d}-{safe_name(entry['path'])}",
            production_binary=production_binary,
            candidate_binary=candidate_binary,
            width=args.width,
            height=args.height,
            samples=args.samples,
            runs=args.screen_runs,
            timeout=args.timeout,
            tile_rows=None,
            candidate_environment=candidate_environment,
        )
        screen["qualifies"] = bool(
            screen.get("pixel_exact")
            and float(screen.get("speedup", 0.0)) >= args.minimum_screen_speedup
        )
        report["scenes"].append(
            {
                "corpus_index": index,
                "path": entry["path"],
                "formula_ids": entry.get("formula_ids", []),
                "scene_sha256": entry["scene_sha256"],
                "initial_screen_speedup": entry["speedup"],
                "screen": screen,
                "native": None,
            }
        )
        write_json(report_path, report)

    finalists = [scene for scene in report["scenes"] if scene["screen"]["qualifies"]]
    finalists.sort(
        key=lambda scene: float(scene["screen"]["gpu_ms_saved"]), reverse=True
    )
    for scene_record in finalists[: args.native_confirm_count]:
        index = int(scene_record["corpus_index"])
        if scene_record.get("native") is not None:
            if scene_record["native"].get("qualifies"):
                report["accepted_hashes"].append(scene_record["scene_sha256"])
            continue
        print(f"native gate {index:04d} {scene_record['path']}", flush=True)
        native = paired_runs(
            root=root,
            scene=root / scene_record["path"],
            output=output / "native" / f"{index:04d}-{safe_name(scene_record['path'])}",
            production_binary=production_binary,
            candidate_binary=candidate_binary,
            width=args.confirmation_width,
            height=args.confirmation_height,
            samples=args.samples,
            runs=args.native_runs,
            timeout=args.native_timeout,
            tile_rows=args.tile_rows,
            candidate_environment=candidate_environment,
        )
        native["qualifies"] = bool(
            native.get("pixel_exact")
            and float(native.get("speedup", 0.0)) >= args.minimum_native_speedup
        )
        scene_record["native"] = native
        if native["qualifies"]:
            report["accepted_hashes"].append(scene_record["scene_sha256"])
        write_json(report_path, report)

    write_json(report_path, report)
    print(
        f"accepted {len(report['accepted_hashes'])}/{min(len(finalists), args.native_confirm_count)} native candidates",
        flush=True,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
