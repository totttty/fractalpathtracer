#!/usr/bin/env python3
"""Compare production Metal rays/DE with native CPU at identical float-rounded points."""
import argparse
import json
from pathlib import Path

import numpy as np
from run_mandel_shadow_controls import replace_main_parameters
from run_release_canaries import execute, sha256


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('scene', 'source-root', 'metal-probe', 'native-probe', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    scene = out / 'scene.fract'
    scene.write_text(replace_main_parameters(args.scene.read_text(), {
        'stereo_enabled': 'false', 'image_width': '300', 'image_height': '158'}))

    def metal(points, name):
        path = out / (name + '-inputs.json')
        path.write_text(json.dumps(np.asarray(points, dtype=np.float32).tolist()))
        result = out / (name + '.json')
        execute([str(args.metal_probe.resolve()), str(scene), str(args.source_root.resolve()),
                 '300', '158', str(path), str(result)], out / (name + '-run'), 240)
        return np.asarray(json.loads(result.read_text())['samples'], dtype=np.float64)

    xy = np.asarray([(x, y) for y in np.linspace(-0.48, 0.48, 27)
                     for x in np.linspace(-0.94, 0.94, 51)], dtype=np.float32)
    rays = metal(np.column_stack([xy, np.zeros(len(xy)), np.full(len(xy), -3)]), 'rays')[:, :3]
    hits = metal(np.column_stack([xy, np.zeros(len(xy)), np.full(len(xy), -4)]), 'hits')
    ray_input = out / 'native-rays-input.tsv'
    np.savetxt(ray_input, np.column_stack([xy, np.zeros(len(xy)), np.ones(len(xy))]), delimiter='\t', fmt='%.17g')
    native_rays_path = out / 'native-rays.tsv'
    execute([str(args.native_probe.resolve()), str(scene), str(ray_input), str(native_rays_path), 'rays'], out / 'native-rays-run', 240)
    native_rays = np.loadtxt(native_rays_path)[:, [0, 2, 1]]
    rays /= np.linalg.norm(rays, axis=1)[:, None]
    native_rays /= np.linalg.norm(native_rays, axis=1)[:, None]
    angles = np.degrees(np.arccos(np.clip((rays * native_rays).sum(axis=1), -1, 1)))
    indexes = np.flatnonzero(hits[:, 3] > 0.5)
    if len(indexes) == 0:
        raise ValueError('no hit points to diagnose')
    indexes = indexes[np.linspace(0, len(indexes)-1, min(128, len(indexes))).astype(int)]
    points = hits[indexes, :3].astype(np.float32)
    primary = metal(np.column_stack([points, np.full(len(points), -1)]), 'primary')
    normals = metal(np.column_stack([points, np.zeros(len(points))]), 'normals')
    field = metal(np.column_stack([points, np.full(len(points), -2)]), 'field')
    # The same representable points, not the pre-rounding doubles, go to native.
    scale = 1024.0
    native_points = points[:, [0, 2, 1]].astype(np.float64) / scale
    inputs = out / 'native-points.tsv'
    np.savetxt(inputs, np.column_stack([native_points, primary[:, 1] / scale]), delimiter='\t', fmt='%.17g')
    report = dict(source_sha256=sha256(args.scene), scene_sha256=sha256(scene),
                  binaries={str(p.resolve()): sha256(p) for p in (args.metal_probe, args.native_probe)},
                  ray_count=len(xy), max_ray_angle_degrees=float(angles.max()),
                  median_ray_angle_degrees=float(np.median(angles)),
                  sampled_hit_count=int((hits[:, 3] > .5).sum()), point_count=len(points), variants={})
    for label, overrides in [('authored', {}), ('automatic', {'delta_DE_method': '0'}),
                             ('forced-delta', {'delta_DE_method': '1'})]:
        control = out / (label + '.fract')
        control.write_text(replace_main_parameters(scene.read_text(), overrides))
        result = out / (label + '-primary.tsv')
        execute([str(args.native_probe.resolve()), str(control), str(inputs), str(result), 'primary'], out / (label + '-run'), 240)
        values = np.loadtxt(result)
        threshold = primary[:, 1] / scale
        fpt_distance = primary[:, 0] / scale
        native_distance = values[:, 0]
        relative = np.abs(native_distance-fpt_distance)/np.maximum(np.abs(native_distance), threshold*.01)
        report['variants'][label] = dict(
            median_relative_distance_error=float(np.median(relative)),
            p95_relative_distance_error=float(np.percentile(relative, 95)),
            native_accepts=int((native_distance < threshold).sum()),
            metal_accepts=int((fpt_distance < threshold).sum()),
            iteration_count_matches=int((values[:, 1] == np.abs(field[:, 3])).sum()),
            points=[dict(pixel_sample_index=int(indexes[i]), xyz=native_points[i].tolist(),
                         threshold=float(threshold[i]), metal_distance=float(fpt_distance[i]),
                         native_distance=float(native_distance[i]), metal_iterations=float(abs(field[i,3])),
                         native_iterations=float(values[i,1])) for i in range(len(points))])
    report['harness_sha256'] = sha256(Path(__file__))
    report['limitations'] = ['Diagnostic kernel, not a production compiler-parity gate.',
                            'FPT hit points only; no native-first-hit completeness claim.',
                            'Normals captured for subsequent stencil comparison; no normal-error claim yet.']
    (out / 'summary.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({**{k:v for k,v in report.items() if k not in ('variants','binaries')},
                      'variants': {k:{a:b for a,b in v.items() if a!='points'} for k,v in report['variants'].items()}},indent=2))


if __name__ == '__main__':
    main()
