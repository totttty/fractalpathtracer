#!/usr/bin/env python3
"""Review a box repair without hiding the still-missing displacement."""
import argparse
import json
from pathlib import Path

from PIL import Image, ImageDraw
from run_release_canaries import difference, sha256


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for name in ('reference', 'controls', 'baseline', 'candidate', 'output'):
        p.add_argument('--' + name, type=Path, required=True)
    args = p.parse_args()
    reports = {name: json.loads(getattr(args, name).read_text())
               for name in ('reference', 'controls', 'baseline', 'candidate')}
    rows = {name: next(r for r in reports[name]['rows'] if r['id'] == '380')
            for name in ('reference', 'baseline', 'candidate')}
    neutral = next(r for r in reports['controls']['rows']
                   if r['id'] == '380' and r['variant'] == 'no-displacement')
    source_hash = rows['reference']['sha256']
    if neutral['source_sha256'] != source_hash or any(r['sha256'] != source_hash for r in rows.values()):
        raise ValueError('scene source changed')
    size = rows['reference']['size']
    if any(r['size'] != size for r in rows.values()):
        raise ValueError('dimensions changed')
    assets = [rows['reference']['modes']['mandel'], neutral['result'],
              rows['baseline']['modes']['geometry'], rows['candidate']['modes']['geometry'],
              rows['baseline']['modes']['authored'], rows['candidate']['modes']['authored']]
    for asset in assets:
        c = asset['capture']
        if asset['status'] != 'ok' or sha256(Path(c['path'])) != c['sha256']:
            raise ValueError('invalid capture')
        with Image.open(c['path']) as im:
            if list(im.size) != size:
                raise ValueError('capture size differs')
    for name in ('baseline', 'candidate'):
        settings = reports[name]['identity']['settings']
        if settings['samples'] != 32 or settings['max_axis'] != 300:
            raise ValueError('expected 300px max edge and 32 samples')
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    def path(i):
        return assets[i]['capture']['path']
    metrics = dict(neutral_before=difference(path(1), path(2)), neutral_after=difference(path(1), path(3)),
                   authored_before=difference(path(0), path(4)), authored_after=difference(path(0), path(5)))
    sheet = Image.new('RGB', (900, 520), '#202326')
    d = ImageDraw.Draw(sheet)
    for col, title in enumerate(('Native: displacement OFF', 'FPT before box fix', 'FPT after box fix')):
        d.text((col*300+8, 12), title, fill='white')
    for col, i in enumerate((1, 2, 3)):
        with Image.open(path(i)) as im:
            sheet.paste(im.convert('RGB'), (col*300, 38))
    d.text((8, 216), 'White diffuse control, fixed camera. Remaining normal/water differences are not hidden.', fill='#ffce86')
    for col, title in enumerate(('Native: FULL authored scene', 'FPT before box fix', 'FPT after box fix')):
        d.text((col*300+8, 263), title, fill='white')
    for col, i in enumerate((0, 4, 5)):
        with Image.open(path(i)) as im:
            sheet.paste(im.convert('RGB'), (col*300, 289))
    d.text((8, 470), '380 GeneralizedFoldBox03_2 | 300px max edge | FPT 32 SPP | no crops or registration', fill='white')
    d.text((8, 489), 'Box restored. Perlin displacement and authored lighting remain incomplete. Not gallery-ready.', fill='#ffce86')
    sheet.save(out/'comparison.png')
    record = dict(scene='380', source_sha256=source_hash, native_geometry_overrides=neutral,
                  assets=assets, metrics=metrics,
                  input_reports={name: sha256(getattr(args, name)) for name in reports},
                  harness_sha256=sha256(Path(__file__)), sheet_sha256=sha256(out/'comparison.png'))
    (out/'summary.json').write_text(json.dumps(record, indent=2)+'\n')
    print(json.dumps(metrics, indent=2))


if __name__ == '__main__':
    main()
