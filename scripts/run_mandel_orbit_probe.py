#!/usr/bin/env python3
"""Decompose formula-85 delta-DE error into orbit counts, rounding and subtraction."""
import argparse
import json
from pathlib import Path
import numpy as np
from run_mandel_normal_position_probe import distribution
from run_release_canaries import execute, sha256


def completed_iterations(records):
    """Native i+1 includes an extra count only when the loop was exhausted."""
    return records[..., 1] - records[..., 2]


def shifted_points(points, delta):
    exact = points[:, None, :] + np.eye(3)[None, :, :] * delta[:, None, None]
    return exact, exact.astype('f4').astype(float)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--stencil', type=Path, required=True)
    parser.add_argument('--diagnostic', type=Path, required=True)
    parser.add_argument('--metal-probe', type=Path, required=True)
    parser.add_argument('--native-probe', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve(); out.mkdir(parents=True, exist_ok=False)
    cmd = json.loads((args.diagnostic / 'fpt/command.json').read_text())
    meta = json.loads((args.diagnostic / 'fpt/hits.bin.json').read_text())
    points = np.array(json.loads((args.stencil / 'field-points.json').read_text()), dtype='f4')[:, :3]
    if len(points) > 25000:
        raise ValueError('probe input too large')
    scale = meta['world_scale']
    def metal(label, points, mode):
        inputs, result = out / f'{label}.json', out / f'{label}-result.json'
        inputs.write_text(json.dumps(np.column_stack([points, mode]).tolist()))
        execute([str(args.metal_probe.resolve()), cmd[2], cmd[cmd.index('--mandelbulber-root')+1],
                 str(meta['width']), str(meta['height']), str(inputs), str(result)], out / f'{label}-run', 180)
        return np.array(json.loads(result.read_text())['samples'])
    def native(label, points, forced):
        inputs, result = out / f'{label}.tsv', out / f'{label}-result.tsv'
        np.savetxt(inputs, np.column_stack([points[:, [0,2,1]], forced]), fmt='%.17g', delimiter='\t')
        execute([str(args.native_probe.resolve()), cmd[2], str(inputs), str(result), 'orbit85'],
                out / f'{label}-run', 180)
        return np.loadtxt(result)
    base_m = metal('metal-base', points, np.full(len(points), 2))
    scaled = points.astype(float) / scale
    base_n = native('native-base', scaled, np.full(len(points), -1))
    delta = base_m[:, 2]
    exact_shifted, shifted = shifted_points(scaled, delta)
    forced = np.repeat(base_m[:, 1], 3)
    shift_m = metal('metal-shifted', shifted.reshape(-1, 3) * scale, forced + 3).reshape(-1, 3, 4)
    shift_n = native('native-shifted', shifted.reshape(-1, 3), forced).reshape(-1, 3, 6)
    exact_n = native('native-unrounded-shifted', exact_shifted.reshape(-1, 3), forced).reshape(-1, 3, 6)
    # Native reports i+1 even after exhausting the loop. maxiter distinguishes
    # that bookkeeping case from a bailout at iteration i.
    base_n_count = completed_iterations(base_n)
    shift_n_count = completed_iterations(shift_n)
    matching = base_n_count == base_m[:, 1]
    radial_m = shift_m[..., 0] - base_m[:, None, 0]
    radial_n = shift_n[..., 0] - base_n[:, None, 0]
    magnitude = np.maximum(np.abs(radial_n), 1e-30)
    report = {
        'scope': 'Formula 85. Native Compute<calcModeDeltaDE1> versus production Metal orbit helper. Same float32 base/shifted points, forced shifted budget from Metal base; native unrounded shifted points are a separate rounding control.',
        'count': len(points), 'matching_base_iteration_count': int(matching.sum()),
        'base_radius_relative_error': distribution(np.abs(base_m[:,0]-base_n[:,0])/np.maximum(np.abs(base_n[:,0]),1e-30)),
        'shifted_radius_relative_error': distribution((np.abs(shift_m[...,0]-shift_n[...,0])/np.maximum(np.abs(shift_n[...,0]),1e-30)).flatten()),
        'native_shifted_early_exit_count': int(np.sum(shift_n_count < base_m[:, None, 1])),
        'native_input_rounding_radius_relative_change': distribution((np.abs(shift_n[...,0]-exact_n[...,0])/np.maximum(np.abs(exact_n[...,0]),1e-30)).flatten()),
        'radial_difference_relative_error_matching_base': distribution((np.abs(radial_m-radial_n)/magnitude)[matching].flatten()),
        'radius_error_over_radial_difference_matching_base': distribution((np.abs(shift_m[...,0]-shift_n[...,0])/magnitude)[matching].flatten()),
        'radial_difference_over_radius_matching_base': distribution((np.abs(radial_n)/np.maximum(np.abs(base_n[:,None,0]),1e-30))[matching].flatten()),
        'identity': {str(p):sha256(p) for p in [Path(cmd[2]),args.metal_probe,args.native_probe,args.stencil / 'field-points.json']},
    }
    (out / 'summary.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(report, indent=2), flush=True)


if __name__ == '__main__':
    main()
