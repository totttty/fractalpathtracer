#!/usr/bin/env python3
"""Prepare deterministic Metal-FPT jobs from a Mandelbulber render manifest."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("batch_dir", type=Path)
    parser.add_argument("--index-base", type=int, required=True)
    parser.add_argument("--mandelbulber-root", type=Path, required=True)
    args = parser.parse_args()

    batch_dir = args.batch_dir.resolve()
    manifest_path = batch_dir / "upstream" / "manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    width = int(manifest["settings"]["width"])
    height = int(manifest["settings"]["height"])
    records = manifest["scenes"]
    failed = [record for record in records if record["status"] != "ok"]
    if failed:
        names = ", ".join(record["relative_path"] for record in failed[:5])
        raise RuntimeError(f"upstream manifest contains failed scenes: {names}")

    scenes: list[dict[str, object]] = []
    jobs: list[list[str]] = []
    for local_index, record in enumerate(records):
        index = args.index_base + local_index
        scene_id = record["id"]
        scenes.append(
            {
                "index": index,
                "id": scene_id,
                "path": record["relative_path"],
            }
        )
        jobs.append(
            [
                record["source"],
                "--out",
                str(batch_dir / "fpt" / scene_id),
                "--mode",
                "diffuse-normal",
                "--mandelbulber-root",
                str(args.mandelbulber_root.resolve()),
                "--width",
                str(width),
                "--height",
                str(height),
            ]
        )

    (batch_dir / "scenes.json").write_text(
        json.dumps(scenes, indent=2) + "\n", encoding="utf-8"
    )
    (batch_dir / "fpt-jobs.json").write_text(
        json.dumps(jobs, indent=2) + "\n", encoding="utf-8"
    )
    print(f"prepared {len(jobs)} scenes in {batch_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
