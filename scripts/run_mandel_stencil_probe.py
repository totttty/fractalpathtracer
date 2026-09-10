#!/usr/bin/env python3
"""Compare native/Metal normal-distance stencils at identical explicit points."""
import argparse
import json
import os
from pathlib import Path
import numpy as np
from run_mandel_normal_position_probe import angles, distribution
from run_release_canaries import execute, sha256


def stencil(points, epsilon):
    offsets = np.array([[1,0,0],[-1,0,0],[0,1,0],[0,-1,0],[0,0,1],[0,0,-1]])
    return points[:, None, :] + epsilon[:, None, None] * offsets[None, :, :]


def normals(distances):
    gradient = distances[:, ::2] - distances[:, 1::2]
    return gradient / np.maximum(np.linalg.norm(gradient, axis=1)[:, None], 1e-300)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--diagnostic', type=Path, required=True)
    parser.add_argument('--metal-probe', type=Path, required=True)
    parser.add_argument('--native-probe', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--count', type=int, default=1024)
    parser.add_argument('--native-metal85-delta', action='store_true')
    parser.add_argument('--max-normal-median', type=float)
    args = parser.parse_args()
    if not 1 <= args.count <= 4096:
        parser.error('count must be 1..4096')
    if args.max_normal_median is not None and (not np.isfinite(args.max_normal_median) or args.max_normal_median < 0):
        parser.error('max-normal-median must be finite and nonnegative')
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    d = args.diagnostic.resolve()
    cmd = json.loads((d / 'fpt/command.json').read_text())
    meta = json.loads((d / 'fpt/hits.bin.json').read_text())
    fpt = np.fromfile(d / 'fpt/hits.bin', '<f4').reshape(-1, 20).astype(float)
    valid = np.flatnonzero(fpt[:, 7] > .5)
    ids = valid[np.linspace(0, len(valid)-1, min(args.count, len(valid)), dtype=int)]
    points = fpt[ids, :3]
    width, height, scale = meta['width'], meta['height'], meta['world_scale']
    root = cmd[cmd.index('--mandelbulber-root') + 1]

    def metal(label, payload):
        inputs, output = out / f'{label}-points.json', out / f'{label}.json'
        inputs.write_text(json.dumps(payload.tolist()))
        execute([str(args.metal_probe.resolve()), cmd[2], root, str(width), str(height),
                 str(inputs), str(output)], out / f'{label}-run', 180)
        report = json.loads(output.read_text())
        return np.array(report['samples'])

    point_result = metal('normal', np.column_stack([points, np.zeros(len(points))]))
    epsilon = point_result[:, 3]
    exact = stencil(points, epsilon)
    rounded = exact.astype('f4').astype(float)
    flat = rounded.reshape(-1, 3)
    field = metal('field', np.column_stack([flat, np.ones(len(flat))]))[:, 0].reshape(-1, 6) / scale
    # Native receives source-space XYZ, converted once from FPT world XYZ.
    native_points = np.concatenate([exact.reshape(-1, 3), flat])[:, [0, 2, 1]] / scale
    thresholds = np.tile(np.repeat(epsilon / scale, 6), 2)
    inputs, output = out / 'native-points.tsv', out / 'native-distances.tsv'
    np.savetxt(inputs, np.column_stack([native_points, thresholds]), fmt='%.17g', delimiter='\t')
    execute([str(args.native_probe.resolve()), cmd[2], str(inputs), str(output)], out / 'native-run', 180)
    native = np.loadtxt(output).reshape(2, len(points), 6, 2)
    ideal, quantized = native[0, ..., 0], native[1, ..., 0]
    ni, nq, ng = normals(ideal), normals(quantized), normals(field)
    rel = np.abs(field - quantized) / np.maximum(np.abs(quantized), 1e-30)
    report = {
        'scope': 'At actual FPT hit positions: native double coordinate stencil vs native float32-rounded stencil vs Metal at the exact same rounded stencil. Native objects retain original compiler flags; no camera reconstruction.',
        'count': len(points), 'world_scale': scale, 'pixel_indices': ids.tolist(),
        'diagnostic_delta_scale': float(os.environ.get('FPT_NORMAL_PROBE_DELTA_SCALE', '1')),
        'diagnostic_analytic85': os.environ.get('FPT_NORMAL_PROBE_ANALYTIC85', '0') == '1',
        'identity': {str(p): sha256(p) for p in [Path(cmd[2]), d / 'fpt/hits.bin', args.native_probe, args.metal_probe]},
        'native_rounding_normal_degrees': distribution(angles(ni, nq)),
        'metal_vs_native_same_points_normal_degrees': distribution(angles(ng, nq)),
        'metal_vs_native_ideal_stencil_normal_degrees': distribution(angles(ng, ni)),
        'metal_stencil_vs_normal_probe_degrees': distribution(angles(ng, point_result[:, :3])),
        'normal_probe_vs_production_degrees': distribution(angles(point_result[:, :3], fpt[ids, 4:7])),
        'distance_relative_error': distribution(rel.flatten()),
        'distance_error_over_epsilon': distribution((np.abs(field-quantized)/(epsilon[:,None]/scale)).flatten()),
        'collapsed_native_gradients': int(np.sum(np.linalg.norm(ni, axis=1) < .5)),
        'collapsed_metal_gradients': int(np.sum(np.linalg.norm(ng, axis=1) < .5)),
    }
    if args.native_metal85_delta:
        output_delta = out / 'native-metal-delta.tsv'
        execute([str(args.native_probe.resolve()), cmd[2], str(inputs), str(output_delta),
                 'metal85-delta'], out / 'native-metal-delta-run', 180)
        coarse = np.loadtxt(output_delta).reshape(2, len(points), 6, 2)[1, ..., 0]
        nc = normals(coarse)
        report['native_metal_delta_vs_native_default_degrees'] = distribution(angles(nc, nq))
        report['metal_vs_native_metal_delta_degrees'] = distribution(angles(ng, nc))
        report['metal_vs_native_metal_delta_distance_relative'] = distribution(
            (np.abs(field - coarse) / np.maximum(np.abs(coarse), 1e-30)).flatten())
    if args.max_normal_median is not None:
        report['gate'] = {'maximum_median_degrees': args.max_normal_median,
                          'passed': report['metal_vs_native_same_points_normal_degrees']['median'] <= args.max_normal_median}
    (out / 'summary.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({k:v for k,v in report.items() if k not in ['identity','pixel_indices']},indent=2), flush=True)
    if report.get('gate', {}).get('passed') is False:
        raise SystemExit('native/Metal normal median exceeds diagnostic gate')


if __name__ == '__main__':
    main()
