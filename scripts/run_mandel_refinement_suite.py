#!/usr/bin/env python3
"""Cross-scene diagnostic refinement/sampling gates using hash-checked references."""
import argparse
import json
import os
from pathlib import Path

from PIL import Image, ImageDraw

from run_release_canaries import difference, execute, sha256


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('legacy-report', 'recovered-report', 'scene572-report', 'source-root', 'probe', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--sampling-seed', type=int)
    args = parser.parse_args()
    if args.sampling_seed is not None and not 0 <= args.sampling_seed <= 0xffffffff:
        parser.error('sampling seed must fit uint32')
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    legacy = json.loads((args.legacy_report / 'summary.json').read_text())
    recovered = {r['index']: r for r in json.loads((args.recovered_report / 'summary.json').read_text())['rows']}
    scenes = []
    for row in legacy['rows']:
        source = args.legacy_report / f"{row['index']:02d}" / 'scene.fract'
        if sha256(source) != row['control_sha256']:
            raise ValueError('changed source control')
        ref = row['captures']['native']
        if row.get('native_single_color'):
            recovery = recovered[row['index']]
            if sha256(source) != recovery['source_sha256']:
                raise ValueError('recovery source differs')
            ref = dict(path=recovery['capture'], sha256=recovery['capture_sha256'])
        scenes.append(dict(index=row['index'], name=Path(row['source']).stem, source=source, mode=row['mode'], size=row['size'],
                           native=ref, accepted=row['captures']['candidate']))
    special = json.loads((args.scene572_report / 'summary.json').read_text())
    authored = next(r for r in special['rows'] if r['mode'] == 'authored')
    scenes.append(dict(index=9, name='hybrid77-stereo', source=args.legacy_report / '09/scene.fract', mode='authored',
                       size=[300, 158], native=authored['captures']['native'], accepted=authored['captures']['candidate']))
    variants = [('unchanged', {}), ('dynamic', {'FPT_BEAUTY_PROBE_DYNAMIC_THRESHOLD': '1'})]
    if args.sampling_seed is not None:
        sample = {'FPT_BEAUTY_PROBE_SAMPLING_SEED': str(args.sampling_seed)}
        variants += [('sampled', sample), ('sampled-dynamic', {**sample, 'FPT_BEAUTY_PROBE_DYNAMIC_THRESHOLD': '1'})]
    report = dict(complete=False, rows=[], probe_sha256=sha256(args.probe),
                  harness_sha256=sha256(Path(__file__)), sampling_seed=args.sampling_seed,
                  scope='Diagnostic only; native CPU sampling differs. No production or performance gate.',
                  omissions=['Authored hybrid25 watchdog case is deliberately excluded.',
                             'Authored checks limited to the two existing safe native references.'])
    for scene in scenes:
        for key in ('native', 'accepted'):
            capture = scene[key]
            if sha256(Path(capture['path'])) != capture['sha256']:
                raise ValueError('reference hash changed')
        with Image.open(scene['native']['path']) as image:
            if list(image.size) != scene['size'] or all(a == b for a, b in image.convert('RGB').getextrema()):
                raise ValueError('unusable reference')
        mode = 'geometry' if scene['mode'] == 'geometry' else 'authored-path'
        row = dict(index=scene['index'], name=scene['name'], source=str(scene['source'].resolve()),
                   source_sha256=sha256(scene['source']), mode=mode, size=scene['size'],
                   native=scene['native'], accepted=scene['accepted'], variants={})
        for label, controls in variants:
            folder = out / f"{scene['index']:02d}" / mode / label
            env = {k: v for k, v in os.environ.items() if not k.startswith((
                'FPT_BEAUTY_PROBE_', 'FPT_NORMAL_PROBE_', 'FPT_MARCH_PROBE_', 'FPT_IFS_PRECISION_'))}
            env.update(controls)
            command = [str(args.probe.resolve()), 'beauty', str(scene['source'].resolve()),
                       '--mandelbulber-root', str(args.source_root.resolve()), '--width', str(scene['size'][0]),
                       '--height', str(scene['size'][1]), '--samples', '32', '--sdf-accumulation', 'chunked',
                       '--sdf-chunk-samples', '1', '--mandel-appearance', mode, '--out', str(folder)]
            execute(command, folder.parent / (label + '-run'), 180, env=env)
            metadata = json.loads((folder / 'summary.json').read_text())
            expected_seed = args.sampling_seed if label.startswith('sampled') else None
            if (metadata.get('dimensioned_step_sampling_seed') != expected_seed or
                    metadata['dynamic_refinement_threshold'] != ('dynamic' in label)):
                raise ValueError('wrong compiled experiment')
            capture = folder / 'render.png'
            accepted_delta = difference(scene['accepted']['path'], capture)
            if label == 'unchanged' and accepted_delta['changed_pixels']:
                raise ValueError(f'unchanged probe differs from accepted capture: {scene["index"]} {mode}')
            row['variants'][label] = dict(path=str(capture), sha256=sha256(capture), controls=controls,
                native_difference=difference(scene['native']['path'], capture), accepted_difference=accepted_delta)
        report['rows'].append(row)
        (out / 'summary.json').write_text(json.dumps(report, indent=2) + '\n')
        print(row['index'], mode, {k: v['native_difference']['mae'] for k, v in row['variants'].items()}, flush=True)
    for mode in ('geometry', 'authored-path'):
        subset = [r for r in report['rows'] if r['mode'] == mode]
        for start in range(0, len(subset), 5):
            rows = subset[start:start + 5]
            sheet = Image.new('RGB', (300 * (len(variants) + 1), 40 + len(rows) * 280), '#202326')
            draw = ImageDraw.Draw(sheet)
            for c, label in enumerate(['Mandel CPU'] + [v[0] for v in variants]):
                draw.text((c * 300 + 8, 10), label, fill='white')
            for r, row in enumerate(rows):
                for c, capture in enumerate([row['native']] + list(row['variants'].values())):
                    with Image.open(capture['path']) as image:
                        sheet.paste(image.convert('RGB'), (c * 300, 40 + r * 280))
                draw.text((8, 40 + r * 280 + 235), f"{row['index']:02d} {row['name']} | {mode} | {row['size'][0]}x{row['size'][1]} | FPT 32 SPP", fill='white')
                draw.text((8, 40 + r * 280 + 254), 'MAE: ' + ' / '.join(f"{k} {v['native_difference']['mae']:.5f}" for k, v in row['variants'].items()), fill='white')
            sheet.save(out / f'{mode}-{start // 5 + 1:02d}.png')
    report['complete'] = True
    (out / 'summary.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
