#!/usr/bin/env python3
"""Gate production sampling against the reviewed 32-SPP diagnostic captures."""
import argparse
import json
import os
from pathlib import Path

from run_release_canaries import difference, execute, image_result, sha256


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('reference', 'binary', 'source-root', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    reference = json.loads((args.reference / 'summary.json').read_text())
    if not reference['complete'] or reference['sampling_seed'] != 1:
        raise ValueError('expected complete seed-1 sampling suite')
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    report = dict(complete=False, binary_sha256=sha256(args.binary),
                  reference_sha256=sha256(args.reference / 'summary.json'),
                  harness_sha256=sha256(Path(__file__)), rows=[],
                  scope='32-SPP production-vs-reviewed-sampling gate. Higher SPP explicitly waived.')
    (out / 'summary.json').write_text(json.dumps(report, indent=2) + '\n')
    env = {k: v for k, v in os.environ.items() if not k.startswith((
        'FPT_BEAUTY_PROBE_', 'FPT_NORMAL_PROBE_', 'FPT_MARCH_PROBE_', 'FPT_IFS_PRECISION_'))}
    for row in reference['rows']:
        source = Path(row['source'])
        expected = row['variants']['sampled']
        if sha256(source) != row['source_sha256'] or sha256(Path(expected['path'])) != expected['sha256']:
            raise ValueError('source or expected image changed')
        folder = out / f"{row['index']:02d}" / row['mode']
        command = [str(args.binary.resolve()), 'render', str(source), '--mandelbulber-root', str(args.source_root.resolve()),
                   '--width', str(row['size'][0]), '--height', str(row['size'][1]), '--samples', '32',
                   '--sdf-accumulation', 'chunked', '--sdf-chunk-samples', '1',
                   '--mandel-appearance', row['mode'], '--out', str(folder)]
        execute(command, folder, 180, env=env)
        capture = image_result(folder, tuple(row['size']))
        delta = difference(expected['path'], capture['path'])
        report['rows'].append(dict(index=row['index'], name=row['name'], mode=row['mode'],
                                   capture=capture, expected=expected, difference=delta))
        (out / 'summary.json').write_text(json.dumps(report, indent=2) + '\n')
        print(row['index'], row['name'], row['mode'], delta, flush=True)
        if delta['changed_pixels']:
            raise ValueError('production differs from reviewed sampling output')
    report['complete'] = True
    (out / 'summary.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
