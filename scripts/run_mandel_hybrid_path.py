#!/usr/bin/env python3
"""Bounded scene-578 integration gate; no gallery promotion or native rendering."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

import numpy as np
from PIL import Image, ImageDraw

REFERENCE_HASHES = {
    'geometry': '9be86c344ffff15423c959305831ce9dcda125f8fbb378ce459985df2f039b7c',
    'authored': '2a0cf83ee5f6407247bca42a122cc2f58058e70c7b93c43d69f95d9002332da4',
}


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def metrics(a, b):
    a = np.array(Image.open(a).convert('RGB'), dtype=float)
    b = np.array(Image.open(b).convert('RGB'), dtype=float)
    if a.shape != b.shape:
        raise ValueError('comparison dimensions differ')
    d = a-b
    return dict(mae_255=float(abs(d).mean()), rmse_255=float(np.sqrt((d*d).mean())),
                changed_pixels=int((d != 0).any(axis=2).sum()))


def verify_reference(path, appearance):
    if sha(path) != REFERENCE_HASHES[appearance]:
        raise ValueError(f'{appearance}: not the validated scene-578 native reference')
    if Image.open(path).size != (300, 225):
        raise ValueError('native reference dimensions differ')


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for flag in ('binary', 'scene', 'mandelbulber-root', 'native-geometry', 'native-authored', 'out'):
        p.add_argument('--'+flag, type=Path, required=True)
    args = p.parse_args()
    verify_reference(args.native_geometry, 'geometry')
    verify_reference(args.native_authored, 'authored')
    args.out.mkdir(parents=True, exist_ok=False)
    out = args.out.resolve()
    env = dict(os.environ, FPT_MANDEL_TILED_DISPATCH='1', FPT_MANDEL_TILE_ROWS='1')
    rows = []
    for appearance, control, repeat in [
        ('geometry', 'float', False), ('geometry', 'safe-float', False),
        ('geometry', 'two-term', False), ('authored-path', 'float', False),
        ('authored-path', 'two-term', False), ('authored-path', 'two-term', True),
    ]:
        label = appearance+'-'+control+('-repeat' if repeat else '')
        directory = out/label
        command = [str(args.binary.resolve()), str(args.scene.resolve()),
                   '--mandelbulber-root', str(args.mandelbulber_root.resolve()),
                   '--width', '300', '--height', '225', '--samples', '4',
                   '--sdf-bounce-cap', '4' if appearance == 'authored-path' else '1',
                   '--mandel-appearance', appearance, '--precision-control', control,
                   '--out', str(directory)]
        (out/(label+'-command.json')).write_text(json.dumps(command, indent=2)+'\n')
        print('capture', label, flush=True)
        with (out/(label+'.stdout')).open('w') as stdout, (out/(label+'.stderr')).open('w') as stderr:
            result = subprocess.run(command, env=env, stdout=stdout, stderr=stderr, timeout=240)
        if result.returncode:
            raise RuntimeError(f'{label} failed; inspect logs')
        report = json.loads((directory/'summary.json').read_text())
        if not report['healthy'] or report['samples'] != 4:
            raise RuntimeError('unhealthy capture')
        ref = args.native_geometry if appearance == 'geometry' else args.native_authored
        rows.append(dict(label=label, report=report, native=metrics(directory/'render.png', ref)))
    first, repeat = out/'authored-path-two-term', out/'authored-path-two-term-repeat'
    exact = {name: (first/name).read_bytes() == (repeat/name).read_bytes()
             for name in ('render.png', 'linear.f32')}
    if not all(exact.values()):
        raise RuntimeError('compensated repeat not byte exact')
    references = []
    sheet = Image.new('RGB', (924, 546), '#1c2024')
    draw = ImageDraw.Draw(sheet)
    for row, (appearance, ref) in enumerate((('geometry', args.native_geometry),
                                            ('authored-path', args.native_authored))):
        draw.text((8, row*273+7), 'White diffuse / geometry' if row == 0 else 'Authored appearance / 4 bounces', fill='white')
        tiles = [('Mandelbulber reference', ref), ('FPT float / 4 SPP', out/(appearance+'-float')/'render.png'),
                 ('FPT two-term / 4 SPP', out/(appearance+'-two-term')/'render.png')]
        for column, (label, path) in enumerate(tiles):
            image = Image.open(path).convert('RGB')
            if image.size != (300, 225):
                raise ValueError('reference/capture must be 300x225')
            sheet.paste(image, (column*308+4, row*273+26))
            draw.text((column*308+4, row*273+256), label, fill='white')
        references.append(dict(appearance=appearance, path=str(ref.resolve()), sha256=sha(ref)))
    sheet.save(out/'comparison.png')
    geometry = {r['report']['control']: r['native']['mae_255'] for r in rows if r['report']['appearance'] == 'geometry'}
    result = dict(scope='Bounded opt-in path integration; not production or gallery promotion',
                  production_changed=False, gallery_changed=False, references=references, results=rows,
                  repeat_byte_exact=exact, geometry_error_reduction_pct=100*(1-geometry['two-term']/geometry['float']),
                  settings='All FPT: 300x225, 4 SPP, sampled march; geometry 1 bounce, authored 4 bounces. Native sampling differs.',
                  timing='Reported render wall time includes tiled CPU/Metal bridge work, not isolated GPU ms or FPS.',
                  limits='Authored color/lighting compatibility is not native shading parity; generic ambient, no new SSAO.',
                  binary_sha256=sha(args.binary), script_sha256=sha(Path(__file__)))
    (out/'summary.json').write_text(json.dumps(result, indent=2)+'\n')
    if not geometry['two-term'] < min(20, geometry['float']*.2):
        raise RuntimeError('geometry recovery gate failed')
    print(json.dumps({'geometry_mae': geometry, 'repeat_byte_exact': exact}, indent=2))


if __name__ == '__main__':
    main()
