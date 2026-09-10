#!/usr/bin/env python3
"""Review Perlin geometry separately from full authored appearance."""
import argparse
import json
from pathlib import Path

from PIL import Image, ImageDraw
from run_release_canaries import difference, sha256


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('reference', 'controls', 'baseline', 'candidate', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    reports = {name: json.loads(getattr(args, name).read_text())
               for name in ('reference', 'controls', 'baseline', 'candidate')}
    rows = {name: next(row for row in reports[name]['rows'] if row['id'] == '380')
            for name in ('reference', 'baseline', 'candidate')}
    control = next(row for row in reports['controls']['rows'] if row['variant'] == 'both')
    source_hash = rows['reference']['sha256']
    if reports['controls']['source_sha256'] != source_hash or any(
            row['sha256'] != source_hash for row in rows.values()):
        raise ValueError('source identity mismatch')
    if not reports['controls']['neutral_perlin_color'] or not control['color_control']:
        raise ValueError('expected explicit white Perlin colour control')
    size = rows['reference']['size']
    if any(row['size'] != size for row in rows.values()):
        raise ValueError('render dimensions mismatch')
    assets = [control['results']['native'], rows['baseline']['modes']['geometry'],
              rows['candidate']['modes']['geometry'], rows['reference']['modes']['mandel'],
              rows['baseline']['modes']['authored'], rows['candidate']['modes']['authored']]
    for asset in assets:
        capture = asset['capture']
        if asset['status'] != 'ok' or sha256(Path(capture['path'])) != capture['sha256']:
            raise ValueError('invalid or modified capture')
        with Image.open(capture['path']) as im:
            if list(im.size) != size:
                raise ValueError('capture dimensions mismatch')
    for name in ('baseline', 'candidate'):
        settings = reports[name]['identity']['settings']
        if settings['samples'] != 32 or settings['max_axis'] != 300:
            raise ValueError('expected 300px max edge, 32 FPT SPP')
    path = lambda i: assets[i]['capture']['path']
    metrics = dict(neutral_before=difference(path(0), path(1)),
                   neutral_after=difference(path(0), path(2)),
                   authored_before=difference(path(3), path(4)),
                   authored_after=difference(path(3), path(5)))
    unchanged = []
    for before in reports['baseline']['rows']:
        after = next(row for row in reports['candidate']['rows'] if row['id'] == before['id'])
        if before['sha256'] != after['sha256']:
            raise ValueError('regression source mismatch')
        for mode in ('geometry', 'authored'):
            a, b = before['modes'][mode], after['modes'][mode]
            if a['status'] != 'ok' or b['status'] != 'ok':
                raise ValueError('regression capture failed')
            if before['id'] != '380':
                if sha256(Path(a['capture']['path'])) != sha256(Path(b['capture']['path'])):
                    raise ValueError(f'unaffected capture changed: {before["id"]} {mode}')
                unchanged.append([before['id'], mode])
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    sheet = Image.new('RGB', (900, 540), '#202326')
    draw = ImageDraw.Draw(sheet)
    for row, title in enumerate(('White diffuse: colour bypass; displacement ON',
                                 'Authored reference: all original effects retained')):
        top = row * 260
        for col, label in enumerate(('Mandelbulber CPU', 'FPT before displacement', 'FPT after displacement')):
            draw.text((col * 300 + 8, top + 10), label, fill='white')
            with Image.open(path(row * 3 + col)) as im:
                sheet.paste(im.convert('RGB'), (col * 300, top + 36))
        draw.text((8, top + 216), title, fill='#ffce86')
        prefix = 'neutral' if row == 0 else 'authored'
        draw.text((8, top + 234),
                  f'RGB MAE {metrics[prefix + "_before"]["mae"]:.5f} -> '
                  f'{metrics[prefix + "_after"]["mae"]:.5f} | appearance diagnostic, not geometry IoU',
                  fill='white')
    draw.text((8, 524), 'Scene 380 | 300x169 | FPT 32 SPP | fixed camera | no image registration', fill='white')
    sheet.save(out / 'comparison.png')
    record = dict(scene='380', source_sha256=source_hash, metrics=metrics,
                  unaffected_byte_exact=unchanged, native_control=control,
                  assets=assets, input_reports={name: sha256(getattr(args, name)) for name in reports},
                  harness_sha256=sha256(Path(__file__)), sheet_sha256=sha256(out / 'comparison.png'),
                  limitations=['Native and FPT sampling/normal/shading differ.',
                               'Authored fog/clouds and material/lighting differences remain.',
                               'Continuous FPT only; no NAADF/CVOX or performance validation.'])
    (out / 'summary.json').write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps(dict(metrics=metrics, unaffected_byte_exact=len(unchanged)), indent=2))


if __name__ == '__main__':
    main()
