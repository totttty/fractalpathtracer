#!/usr/bin/env python3
"""Compare original native and Metal primary marchers on identical float rays."""
import argparse
import json
import os
from pathlib import Path

import numpy as np

from run_mandel_shadow_controls import replace_main_parameters
from run_release_canaries import execute, sha256, scene_dimensions


def probe_framing(source, max_axis=None):
    if max_axis is None:
        return 300, 158, .94
    if not 16 <= max_axis <= 4096:
        raise ValueError('max axis must be 16..4096')
    width, height = scene_dimensions(source, max_axis)
    return width, height, .48 * width / height


def compare_hits(native, metal, camera, scale):
    native, metal, camera = (np.asarray(v, dtype=np.float64) for v in (native, metal, camera))
    if (native.ndim != 2 or native.shape[1] != 8 or
            metal.shape != (len(native), 4) or camera.shape != (3,)):
        raise ValueError('expected native Nx8, Metal Nx4 and camera XYZ')
    if not np.isfinite(scale) or scale <= 0 or not all(np.isfinite(v).all() for v in (native, metal, camera)):
        raise ValueError('nonfinite input or invalid world scale')
    native_hit, metal_hit = native[:, 0] > .5, metal[:, 3] > .5
    common = native_hit & metal_hit
    native_world = native[:, [1, 3, 2]] * scale
    error = np.linalg.norm(native_world - metal[:, :3], axis=1) / scale
    threshold = native[:, 6]
    normalized = error / np.maximum(threshold, 1e-15)
    records = []
    for i in range(len(native)):
        records.append(dict(index=i, native_hit=bool(native_hit[i]), metal_hit=bool(metal_hit[i]),
                            native_point=native_world[i].tolist(), metal_point=metal[i, :3].tolist(),
                            position_error=float(error[i]), threshold=float(threshold[i]),
                            error_in_thresholds=float(normalized[i]),
                            native_steps=int(native[i, 7]),
                            native_travel=float(np.linalg.norm(native_world[i] - camera) / scale)))
    return dict(common_hits=int(common.sum()), native_only=int((native_hit & ~metal_hit).sum()),
                metal_only=int((metal_hit & ~native_hit).sum()), neither=int((~native_hit & ~metal_hit).sum()),
                median_common_error_in_thresholds=float(np.median(normalized[common])) if common.any() else None,
                common_over_one_threshold=int((common & (normalized > 1)).sum()), rows=records)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('scene', 'source-root', 'metal-probe', 'native-probe', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--grid-width', type=int, default=21)
    parser.add_argument('--grid-height', type=int, default=13)
    parser.add_argument('--seeds', type=int, nargs='+', default=[0, 1, 2])
    parser.add_argument('--native-mode', choices=('march', 'hybrid-march'), default='march')
    parser.add_argument('--max-axis', type=int, help='Preserve authored aspect at this maximum edge; default is legacy 300x158.')
    args = parser.parse_args()
    if not (2 <= args.grid_width <= 64 and 2 <= args.grid_height <= 64):
        parser.error('grid dimensions must be 2..64')
    if not args.seeds or len(set(args.seeds)) != len(args.seeds) or any(s < 0 or s > 2147483646 for s in args.seeds):
        parser.error('seeds must be distinct integers in 0..2147483646')
    if args.max_axis is not None and not 16 <= args.max_axis <= 4096:
        parser.error('max axis must be 16..4096')
    source = args.scene.read_text()
    width, height, x_extent = probe_framing(source, args.max_axis)
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    scene = out / 'scene.fract'
    scene.write_text(replace_main_parameters(source, {
        'stereo_enabled': 'false', 'image_width': str(width), 'image_height': str(height)}))

    def metal(points, name):
        inputs, output = out / (name + '-inputs.json'), out / (name + '.json')
        inputs.write_text(json.dumps(np.asarray(points, dtype=np.float32).tolist()))
        execute([str(args.metal_probe.resolve()), str(scene), str(args.source_root.resolve()),
                 str(width), str(height), str(inputs), str(output)], out / (name + '-run'), 120)
        result = json.loads(output.read_text())
        return np.asarray(result['samples'], dtype=np.float64), result

    xy = np.asarray([(x, y) for y in np.linspace(-.48, .48, args.grid_height)
                     for x in np.linspace(-x_extent, x_extent, args.grid_width)], dtype=np.float32)
    rays, metadata = metal(np.column_stack([xy, np.zeros(len(xy)), np.full(len(xy), -3)]), 'rays')
    scale = float(metadata['world_scale'])
    camera = np.asarray(metadata['camera_world'], dtype=np.float64)
    overrides = out / 'native-camera.override'
    overrides.write_text('camera=' + ' '.join(format(x, '.17g') for x in camera[[0, 2, 1]] / scale) + '\n')
    refined, _ = metal(np.column_stack([rays[:, :3], np.full(len(xy), -5)]), 'refined')
    raw, _ = metal(np.column_stack([rays[:, :3], np.full(len(xy), -6)]), 'unrefined')
    generated, _ = metal(np.column_stack([xy, np.zeros(len(xy)), np.full(len(xy), -4)]), 'generated-rays-hit')
    if not np.array_equal(refined[:, 3], raw[:, 3]):
        raise ValueError('instrumented/raw Metal hit flags differ')
    report = dict(source_sha256=sha256(args.scene), control_sha256=sha256(scene),
                  dimensions=[width, height], native_mode=args.native_mode,
                  binaries={str(p.resolve()): sha256(p) for p in (args.metal_probe, args.native_probe)},
                  camera_world=camera.tolist(), world_scale=scale, ray_count=len(xy),
                  generated_vs_supplied_hit_flag_changes=int((generated[:, 3] != refined[:, 3]).sum()),
                  generated_vs_supplied_position_changes=int(np.any(generated[:, :3] != refined[:, :3], axis=1).sum()),
                  variants={}, limitations=[
                      'Isolated kernels/native worker, not an image or production compiler parity gate.',
                      'Native origin rounded to the same Metal float32 world origin.',
                      'Native original double marcher; no copied native traversal.',
                      'Seed 0 disables native step jitter; positive seeds retain it.',
                      'No pixel jitter, shading, stereo, primitive or displacement paths.'])
    for seed in args.seeds:
        inputs, output = out / f'native-{seed}-inputs.tsv', out / f'native-{seed}.tsv'
        np.savetxt(inputs, np.column_stack([rays[:, [0, 2, 1]], np.full(len(xy), seed)]), fmt='%.17g', delimiter='\t')
        execute([str(args.native_probe.resolve()), str(scene), str(inputs), str(output), args.native_mode],
                out / f'native-{seed}-run', 120,
                env=dict(os.environ, FPT_NATIVE_PROBE_OVERRIDES_FILE=str(overrides)))
        values = np.loadtxt(output, ndmin=2)
        if values.shape != (len(xy), 16) or not np.isfinite(values).all():
            raise ValueError('invalid native marcher output')
        report['variants'][str(seed)] = dict(raw=compare_hits(values[:, :8], raw, camera, scale),
                                           refined=compare_hits(values[:, 8:], refined, camera, scale))
    report['harness_sha256'] = sha256(Path(__file__))
    (out / 'summary.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({**{k: v for k, v in report.items() if k != 'variants'}, 'variants': {
        seed: {mode: {k: v for k, v in values.items() if k != 'rows'} for mode, values in modes.items()}
        for seed, modes in report['variants'].items()}}, indent=2))


if __name__ == '__main__':
    main()
