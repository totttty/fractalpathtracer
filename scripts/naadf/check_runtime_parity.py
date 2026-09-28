#!/usr/bin/env python3
"""Compare the typed runtime against a preserved pre-extraction executable."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
from PIL import Image
import numpy as np


def digest(path):
    return hashlib.file_digest(path.open('rb'), 'sha256').hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for name in ('baseline', 'worker', 'catalog', 'mandel-root', 'output'):
        p.add_argument('--'+name, type=Path, required=True)
    p.add_argument('--ids', nargs='+', default=['04', '21', '475'])
    args = p.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    catalog = json.loads(args.catalog.read_text())
    rows = {s['id']: s for s in catalog['scenes']}
    env = {k: v for k, v in os.environ.items() if not k.startswith('FPT_')}
    env.update(FPT_MANDEL_TILED_DISPATCH='1', FPT_MANDEL_TILE_ROWS='8')
    report = dict(baseline_sha256=digest(args.baseline), worker_sha256=digest(args.worker),
                  catalog_sha256=digest(args.catalog), results=[])
    for sid in args.ids:
        row = rows[sid]
        source = args.mandel_root/'deploy/share/mandelbulber2/examples'/row['path']
        assert digest(source) == row['sha256']
        width, height = row['review_evidence']['dimensions']
        for mode in ('neutral', 'authored'):
            root = args.output/(sid+'-'+mode)
            root.mkdir()
            request = dict(source=dict(path=str(source.resolve()), sha256=row['sha256'],
                           mandelbulber_root=str(args.mandel_root.resolve())),
                           output_directory=str((root/'library').resolve()), maximum_axis=300,
                           samples=32, chunk_samples=1, bounces=None, appearance=mode)
            req = root/'request.json'
            req.write_text(json.dumps(request, indent=2))
            commands = [
                [str(args.baseline.resolve()), 'render', str(source), '--mandelbulber-root',
                 str(args.mandel_root), '--out', str(root/'baseline'), '--width', str(width),
                 '--height', str(height), '--samples', '32', '--sdf-chunk-samples', '1',
                 '--mandel-appearance', 'geometry' if mode == 'neutral' else 'authored-path'],
                [str(args.worker.resolve()), 'render', str(req), str(root/'receipt.json')]]
            for label, command in zip(('baseline', 'library'), commands):
                with (root/(label+'.log')).open('w') as log:
                    subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT,
                                   check=True, timeout=600)
            before = next((root/'baseline').glob('*.png'))
            after = Path(json.loads((root/'receipt.json').read_text())['path'])
            a, b = np.array(Image.open(before)), np.array(Image.open(after))
            same = a.shape == b.shape and np.array_equal(a, b)
            result = dict(id=sid, appearance=mode, pixels_identical=same,
                          baseline_sha256=digest(before), library_sha256=digest(after),
                          baseline=str(before.resolve()), library=str(after.resolve()))
            report['results'].append(result)
            (args.output/'report.json').write_text(json.dumps(report, indent=2))
            print(sid, mode, 'identical' if same else 'DIFFERENT', flush=True)
    if not all(r['pixels_identical'] for r in report['results']):
        raise SystemExit(1)


if __name__ == '__main__':
    main()
