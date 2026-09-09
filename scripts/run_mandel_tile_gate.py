#!/usr/bin/env python3
"""Byte-exact gate for experimental spatial tiling within Mandel sample chunks."""
import argparse
import json
import os
from pathlib import Path

from run_release_canaries import ROOT, difference, execute, image_result, sha256, validate_manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline-fpt', type=Path, required=True)
    parser.add_argument('--fpt', type=Path, required=True)
    parser.add_argument('--scene-root', type=Path, required=True)
    parser.add_argument('--mandelbulber-root', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--manifest', type=Path, default=ROOT/'tests/fixtures/mandel-release-canaries.json')
    parser.add_argument('--tile-rows', type=int, nargs='+', default=[1, 7, 32])
    parser.add_argument('--width', type=int, default=97)
    parser.add_argument('--height', type=int, default=83)
    parser.add_argument('--samples', type=int, default=5)
    parser.add_argument('--chunk-samples', type=int, default=2)
    parser.add_argument('--timeout', type=int, default=300)
    args = parser.parse_args()
    if not (16 <= args.width <= 2048 and 16 <= args.height <= 2048 and
            1 <= args.samples <= 512 and 1 <= args.chunk_samples <= 64 and args.timeout > 0 and
            len(set(args.tile_rows)) == len(args.tile_rows) and all(1 <= r <= 32 for r in args.tile_rows)):
        parser.error('invalid dimensions, samples, timeout or tile rows')
    manifest = json.loads(args.manifest.read_text())
    validate_manifest(manifest, args.scene_root)
    for path in (args.fpt, args.baseline_fpt):
        if not path.is_file():
            parser.error('missing executable: '+str(path))
    out = args.output.resolve()
    if out.exists() and any(out.iterdir()):
        parser.error('output must be empty')
    out.mkdir(parents=True, exist_ok=True)
    environment = dict(os.environ)
    environment.pop('FPT_MANDEL_TILED_DISPATCH', None)
    environment.pop('FPT_MANDEL_TILE_ROWS', None)
    summary = dict(manifest=manifest, settings=vars(args).copy(), rows=[],
                   baseline_sha256=sha256(args.baseline_fpt), candidate_sha256=sha256(args.fpt),
                   fpt_environment={k:v for k,v in environment.items() if k.startswith('FPT_')})
    for scene in manifest['scenes']:
        for mode in ('geometry', 'authored-path'):
            row = dict(id=scene['id'], mode=mode, captures={}, status='running')
            common = ['render', str((args.scene_root/scene['path']).resolve()),
                      '--mandelbulber-root', str(args.mandelbulber_root.resolve()),
                      '--width', str(args.width), '--height', str(args.height),
                      '--samples', str(args.samples), '--sdf-accumulation', 'chunked',
                      '--sdf-chunk-samples', str(args.chunk_samples), '--mandel-appearance', mode]
            try:
                # The candidate-off control detects unrelated compilation/output drift.
                variants = [('baseline', args.baseline_fpt, None), ('candidate-off', args.fpt, None)]
                variants += [(f'tile-{r}', args.fpt, r) for r in args.tile_rows]
                for label, binary, tile_rows in variants:
                    folder = out/scene['id']/mode/label
                    env = dict(environment)
                    if tile_rows is not None:
                        env.update(FPT_MANDEL_TILED_DISPATCH='1', FPT_MANDEL_TILE_ROWS=str(tile_rows))
                    command = [str(binary.resolve()), *common, '--out', str(folder)]
                    seconds = execute(command, folder, args.timeout, env=env)
                    capture = dict(image_result(folder, (args.width, args.height)), seconds=seconds)
                    row['captures'][label] = capture
                    if label != 'baseline':
                        capture['difference'] = difference(row['captures']['baseline']['path'], capture['path'])
                        if capture['difference']['changed_pixels']:
                            raise ValueError(label+' changed pixels')
                    print(scene['id'], mode, label, f'{seconds:.2f}s', flush=True)
                row['status'] = 'ok'
            except Exception as error:
                row.update(status='failed', error=str(error))
                print(scene['id'], mode, str(error), flush=True)
            summary['rows'].append(row)
            summary['passed'] = all(r['status']=='ok' for r in summary['rows'])
            (out/'summary.json').write_text(json.dumps(summary, indent=2, default=str))
    return int(not summary['passed'])


if __name__ == '__main__':
    raise SystemExit(main())
