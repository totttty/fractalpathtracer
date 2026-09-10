#!/usr/bin/env python3
"""Replay first-hit rays with isolated step-jitter/refinement controls."""
import argparse
import json
import os
from pathlib import Path

import numpy as np

from run_mandel_first_hit_parity import compare_hits
from run_release_canaries import execute, sha256


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', type=Path, required=True)
    parser.add_argument('--source-root', type=Path, required=True)
    parser.add_argument('--metal-probe', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    baseline, out = args.baseline.resolve(), args.output.resolve()
    source = json.loads((baseline / 'summary.json').read_text())
    scene = baseline / 'scene.fract'
    if sha256(scene) != source['control_sha256']:
        raise ValueError('baseline control source changed')
    out.mkdir(parents=True, exist_ok=False)
    camera, scale = source['camera_world'], source['world_scale']
    report = dict(baseline=str(baseline), baseline_summary_sha256=sha256(baseline / 'summary.json'),
                  binary_sha256=sha256(args.metal_probe), harness_sha256=sha256(Path(__file__)),
                  ray_count=source['ray_count'], variants={}, complete=False,
                  scope='Diagnostic supplied primary rays only; no production or image parity claim.')
    references = {seed: np.loadtxt(baseline / f'native-{seed}.tsv', ndmin=2) for seed in (0, 1, 2)}
    accepted = {mode: np.asarray(json.loads((baseline / f'{name}.json').read_text())['samples'])
                for mode, name in [('raw', 'unrefined'), ('refined', 'refined')]}
    for seed, dynamic in [(None, False), (0, False), (1, False), (2, False),
                          (None, True), (1, True), (2, True)]:
        label = f'seed-{seed}-dynamic-{int(dynamic)}'
        env = {k: v for k, v in os.environ.items() if not k.startswith('FPT_NORMAL_PROBE_')}
        env['FPT_NORMAL_PROBE_DYNAMIC_THRESHOLD'] = str(int(dynamic))
        if seed is not None:
            env['FPT_NORMAL_PROBE_STEP_SEED'] = str(seed)
        results, arrays = {}, {}
        for mode, name in [('raw', 'unrefined'), ('refined', 'refined')]:
            output = out / f'{label}-{mode}.json'
            execute([str(args.metal_probe.resolve()), str(scene), str(args.source_root.resolve()),
                     '300', '158', str(baseline / f'{name}-inputs.json'), str(output)],
                    out / f'{label}-{mode}-run', 120, env=env)
            data = json.loads(output.read_text())
            if data['diagnostic_step_seed'] != seed or data['diagnostic_dynamic_threshold'] != dynamic:
                raise ValueError('wrong diagnostic configuration')
            values = arrays[mode] = np.asarray(data['samples'])
            native_seed = seed or 0
            native = references[native_seed][:, :8] if mode == 'raw' else references[native_seed][:, 8:]
            results[mode] = compare_hits(native, values, camera, scale)
            results[mode]['capture_sha256'] = sha256(output)
            results[mode]['vs_accepted_hit_changes'] = int((values[:, 3] != accepted[mode][:, 3]).sum())
            results[mode]['vs_accepted_position_changes'] = int(np.any(values[:, :3] != accepted[mode][:, :3], axis=1).sum())
            if seed in (None, 0) and not dynamic and not np.array_equal(values, accepted[mode]):
                raise ValueError('no-jitter control changed accepted output')
        if not np.array_equal(arrays['raw'][:, 3], arrays['refined'][:, 3]):
            raise ValueError('refinement changed visibility')
        report['variants'][label] = results
        (out / 'summary.json').write_text(json.dumps(report, indent=2) + '\n')
        print(label, json.dumps({mode: {k: v for k, v in r.items() if k != 'rows'}
                                 for mode, r in results.items()}), flush=True)
    report['complete'] = True
    (out / 'summary.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
