#!/usr/bin/env python3
"""Separate normal evaluation from hit location using immutable diagnostic dumps."""
import argparse
import json
from pathlib import Path

import numpy as np
from run_mandel_support_suite import parameters
from run_release_canaries import execute, sha256


def angles(a, b):
    dot = np.sum(a * b, axis=-1)
    length = np.linalg.norm(a, axis=-1) * np.linalg.norm(b, axis=-1)
    return np.degrees(np.arccos(np.clip(dot / np.maximum(length, 1e-30), -1, 1)))


def distribution(values):
    return {'count': int(values.size), 'median': float(np.median(values)),
            'p95': float(np.quantile(values, .95))}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--diagnostic', type=Path, required=True)
    parser.add_argument('--probe', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    d = args.diagnostic
    cmd = json.loads((d / 'fpt/command.json').read_text())
    metadata = json.loads((d / 'fpt/hits.bin.json').read_text())
    size = (metadata['height'], metadata['width'])
    native = np.fromfile(d / 'native/channels.bin', '<f4').reshape(*size, 4).astype(float)
    fpt = np.fromfile(d / 'fpt/hits.bin', '<f4').reshape(*size, 20).astype(float)
    scale = metadata['world_scale']
    settings = parameters(Path(cmd[2]).read_text())
    camera = np.array([float(x.replace(',', '.')) for x in settings['camera'].split()])[[0, 2, 1]] * scale
    # Use actual diagnostic incoming rays, never independently reconstructed UVs.
    # Native depth plus these rays is approximate, not an authoritative native hit.
    valid = (native[..., 0] > 0) & (native[..., 0] < 1e10) & (fpt[..., 7] > .5)
    valid &= np.all(np.isfinite(native), axis=-1)
    native_normals = native[..., [1, 3, 2]][valid]
    fpt_normals = fpt[..., 4:7][valid]
    points = fpt[..., :3][valid]
    directions = fpt[..., 16:19][valid]
    distances = native[..., 0][valid] * scale
    rounded_camera = np.array(metadata['camera']['position'])
    inputs = {
        'actual-fpt-hit': points,
        'native-depth-fpt-origin': rounded_camera + directions * distances[:, None],
        'native-depth-source-origin': camera + directions * distances[:, None],
    }
    stacked = np.concatenate(list(inputs.values())).astype('f4')
    payload = np.column_stack([stacked, np.zeros(len(stacked))])
    point_file = out / 'points.json'
    point_file.write_text(json.dumps(payload.tolist()))
    output = out / 'samples.json'
    execute([str(args.probe.resolve()), cmd[2], cmd[cmd.index('--mandelbulber-root') + 1],
             str(size[1]), str(size[0]), str(point_file), str(output)], out / 'run', 240)
    probe = json.loads(output.read_text())
    samples = np.array(probe['samples']).reshape(len(inputs), -1, 4)
    report = {'scope': 'Re-evaluate production normal function at actual FPT hits and approximate native-depth positions. Native-depth positions use FPT rays; not an exact native point/evaluator comparison.',
              'inputs': {str(p): sha256(p) for p in [d / 'native/channels.bin', d / 'fpt/hits.bin', Path(cmd[2])]},
              'probe_binary_sha256': probe['binary_sha256'], 'rows': []}
    for index, (label, positions) in enumerate(inputs.items()):
        normals, epsilon = samples[index, :, :3], samples[index, :, 3]
        spacing = np.abs(np.spacing(positions.astype('f4'))).astype(float)
        ratios = np.max(spacing, axis=-1) / np.maximum(epsilon, 1e-30)
        row = {'mode': label, 'normal_vs_native': distribution(angles(normals, native_normals)),
               'normal_vs_production_dump': distribution(angles(normals, fpt_normals)),
               'normal_epsilon': distribution(epsilon),
               'max_coordinate_ulp_over_epsilon': distribution(ratios),
               'ulp_larger_than_epsilon_pixels': int(np.sum(ratios > 1)),
               'position_shift': distribution(np.linalg.norm(positions - points, axis=-1))}
        report['rows'].append(row)
        print(label, row, flush=True)
    (out / 'summary.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
