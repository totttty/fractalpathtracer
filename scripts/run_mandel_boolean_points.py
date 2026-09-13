#!/usr/bin/env python3
"""Audit unsigned boolean DE against linked native code at identical points."""
import argparse
import json
from pathlib import Path

import numpy as np

from run_mandel_shadow_controls import replace_main_parameters
from run_release_canaries import execute, scene_dimensions, sha256


def distance_errors(native_distance, metal_distance, thresholds):
    arrays = [np.asarray(v, dtype=np.float64) for v in (native_distance, metal_distance, thresholds)]
    if (any(v.ndim != 1 or len(v) == 0 or not np.isfinite(v).all() for v in arrays)
            or any(v.shape != arrays[0].shape for v in arrays)
            or np.any(arrays[2] <= 0)):
        raise ValueError('expected equal nonempty finite arrays and positive thresholds')
    error = np.abs(arrays[0] - arrays[1]) / arrays[2]
    return dict(median_error_in_thresholds=float(np.median(error)),
                p95_error_in_thresholds=float(np.percentile(error, 95)),
                over_one_threshold=int((error > 1).sum()))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('scene', 'source-root', 'metal-probe', 'native-probe', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    size = scene_dimensions(args.scene.read_text(), 300)
    scene = out / 'scene.fract'
    scene.write_text(replace_main_parameters(args.scene.read_text(), {
        'image_width': str(size[0]), 'image_height': str(size[1])}))

    def metal(points, name):
        inputs, output = out / (name + '-inputs.json'), out / (name + '.json')
        inputs.write_text(json.dumps(np.asarray(points, dtype=np.float32).tolist()))
        execute([str(args.metal_probe.resolve()), str(scene), str(args.source_root.resolve()),
                 *map(str, size), str(inputs), str(output)], out / (name + '-run'), 120)
        metadata = json.loads(output.read_text())
        values = np.asarray(metadata['samples'], dtype=np.float64)
        if values.shape != (len(points), 4) or not np.isfinite(values).all():
            raise ValueError('invalid Metal point output')
        return values, metadata

    xy = [(x, y) for y in np.linspace(-.45, .45, 13)
          for x in np.linspace(-.45 * size[0] / size[1], .45 * size[0] / size[1], 21)]
    hits, metadata = metal([[x, y, 0, -4] for x, y in xy], 'hits')
    points = hits[hits[:, 3] > .5, :3].astype(np.float32)
    if len(points) == 0:
        raise ValueError('no common point candidates')
    scale = float(metadata['world_scale'])
    primary, _ = metal(np.column_stack([points, np.full(len(points), -1)]), 'primary')
    normal, _ = metal(np.column_stack([points, np.ones(len(points))]), 'normal')
    native_points = points[:, [0, 2, 1]].astype(np.float64) / scale
    inputs = out / 'native-inputs.tsv'
    np.savetxt(inputs, np.column_stack([native_points, primary[:, 1] / scale]), fmt='%.17g', delimiter='\t')
    report = dict(source_sha256=sha256(args.scene), control_sha256=sha256(scene), size=size,
                  binaries={str(p.resolve()): sha256(p) for p in (args.metal_probe, args.native_probe)},
                  point_count=len(points), world_scale=scale, variants={}, limitations=[
                      'Candidate hit points only; not a whole-scene completeness score.',
                      'Same float-rounded positions and detail thresholds; native orbit remains double.',
                      'No primitive, texture displacement, perlin displacement or object-tree data.',
                      'Point probe is not a production image compiler parity gate.'])
    for mode, values in [('primary', primary), ('normal', normal)]:
        output = out / (mode + '-native.tsv')
        execute([str(args.native_probe.resolve()), str(scene), str(inputs), str(output),
                 'boolean-' + mode], out / (mode + '-native-run'), 120)
        native = np.loadtxt(output, ndmin=2)
        if native.shape != (len(points), 2) or not np.isfinite(native).all():
            raise ValueError('invalid native output')
        threshold = primary[:, 1] / scale
        distance = values[:, 0] / scale
        report['variants'][mode] = dict(
            **distance_errors(native[:, 0], distance, threshold),
            rows=[dict(point=p.tolist(), threshold=float(t), metal_distance=float(m), native_distance=float(n))
                  for p, t, m, n in zip(native_points, threshold, distance, native[:, 0])])
    report['harness_sha256'] = sha256(Path(__file__))
    (out / 'summary.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({k: {a: b for a, b in v.items() if a != 'rows'}
                      for k, v in report['variants'].items()}, indent=2))


if __name__ == '__main__':
    main()
