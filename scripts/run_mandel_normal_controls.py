#!/usr/bin/env python3
"""Compare native world normals and FPT structural records at matched pixels."""
import argparse
import json
import os
from pathlib import Path
import subprocess

import numpy as np
from PIL import Image, ImageDraw
from run_release_canaries import execute, sha256


def compare(native, fpt, scale):
    if native.shape[:2] != fpt.shape[:2] or native.shape[2] != 4 or fpt.shape[2] != 20:
        raise ValueError('expected matching native float4 and FPT float20 images')
    if not np.isfinite(scale) or scale <= 0:
        raise ValueError('world scale must be positive and finite')
    a, b = native[..., 0], fpt[..., 3] / scale
    hn = np.isfinite(a) & (a > 0) & (a < 1e10)
    hf = fpt[..., 7] > 0.5
    both = hn & hf
    relative = np.abs(a - b) / np.maximum(np.abs(a), 1e-30)
    # Native CPU writes signed, normalized XYZ directly to nW.X/Y/Z.
    # FPT's world mapping swaps Y and Z. No image flip or registration.
    n, f = native[..., [1, 3, 2]], fpt[..., 4:7]
    nl, fl = np.linalg.norm(n, axis=2), np.linalg.norm(f, axis=2)
    valid = both & np.isfinite(nl) & np.isfinite(fl) & (nl > 0.5) & (fl > 0.5)
    cosine = np.sum(n * f, axis=2) / np.maximum(nl * fl, 1e-30)
    angles = np.degrees(np.arccos(np.clip(cosine, -1, 1)))
    close = valid & (relative <= 0.001)
    result = {'native_hits': int(hn.sum()), 'fpt_hits': int(hf.sum()),
              'visible_misses': int((hn & ~hf).sum()), 'visible_extras': int((hf & ~hn).sum()),
              'matched_depth_tolerance': 0.001}
    for label, values in [('depth_relative', relative[both]),
                          ('normal_degrees_all', angles[valid]),
                          ('normal_degrees_depth_matched', angles[close])]:
        result[label] = {'count': int(values.size),
                         'median': float(np.median(values)) if values.size else None,
                         'p95': float(np.quantile(values, .95)) if values.size else None}
    return result, n, f, angles, valid, close


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--controls', type=Path, required=True)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--exr-decoder', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--scenes', nargs='+', default=['14', '30'])
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    report = {'scope': 'Unjittered native CPU versus FPT production-compiled diagnostic; signed world normals, fixed coordinates, no registration. Depth agreement does not prove identical hit positions.',
              'binaries': {str(p.resolve()): sha256(p) for p in [args.binary, args.exr_decoder]}, 'rows': []}
    for scene in args.scenes:
        source = args.controls.resolve() / scene / 'no-shadows'
        dest = out / scene
        native = json.loads((source / 'native/command.json').read_text())
        native[native.index('-f') + 1] = 'exr'
        native[native.index('-o') + 1] = str(dest / 'native/scene.exr')
        native[native.index('-O') + 1] += '#zbuffer_enabled=1#normalWorld_enabled=1#normalWorld_quality=32'
        report['binaries'][native[0]] = sha256(Path(native[0]))
        execute(native, dest / 'native', 240)
        if 'OpenCl - rendering' in (dest / 'native/stdout.log').read_text():
            raise RuntimeError('native CPU control required')
        subprocess.run([str(args.exr_decoder.resolve()), str(dest / 'native/scene.exr'),
                        str(dest / 'native/channels.bin')], check=True, timeout=30)
        fpt = json.loads((source / 'candidate/command.json').read_text())
        fpt[0], fpt[1] = str(args.binary.resolve()), 'diagnostic'
        fpt[fpt.index('--out') + 1] = str(dest / 'fpt')
        fpt[fpt.index('--samples') + 1] = '1'
        fpt += ['--mode', 'normal', '--structural-dump', str(dest / 'fpt/hits.bin')]
        execute(fpt, dest / 'fpt', 240, env=dict(os.environ, FPT_MANDEL_DIAGNOSTIC_PRODUCTION_COMPILE='1'))
        width, height = int(fpt[fpt.index('--width') + 1]), int(fpt[fpt.index('--height') + 1])
        manifest = json.loads((dest / 'fpt/hits.bin.json').read_text())
        n = np.fromfile(dest / 'native/channels.bin', dtype='<f4').reshape(height, width, 4)
        f = np.fromfile(dest / 'fpt/hits.bin', dtype='<f4').reshape(height, width, 20)
        metrics, nn, fn, angles, valid, close = compare(n, f, manifest['world_scale'])
        report['rows'].append({'id': scene, 'size': [width, height],
                               'source_sha256': sha256(Path(fpt[2])), **metrics})
        for label, normal in [('native', nn), ('fpt', fn)]:
            pixels = np.uint8(np.clip((normal + 1) * 127.5, 0, 255))
            pixels[~valid] = 0
            Image.fromarray(pixels).save(dest / f'{label}-normal.png')
        heat = np.zeros((height, width, 3), dtype=np.uint8)
        heat[..., 0] = np.uint8(np.clip(angles / 90 * 255, 0, 255))
        heat[~valid] = [0, 255, 255]
        heat[valid & ~close] = [255, 255, 0]
        Image.fromarray(heat).save(dest / 'difference.png')
        print(scene, metrics, flush=True)
        (out / 'summary.json').write_text(json.dumps(report, indent=2) + '\n')
    sheet = Image.new('RGB', (900, 40 + 250 * len(report['rows'])), '#191c1e')
    draw = ImageDraw.Draw(sheet)
    for c, label in enumerate(['Native world normal (mapped Y/Z)', 'FPT world normal', 'Red: angle; yellow: depth mismatch']):
        draw.text((c * 300 + 5, 10), label, fill='white')
    for r, row in enumerate(report['rows']):
        for c, name in enumerate(['native-normal', 'fpt-normal', 'difference']):
            with Image.open(out / row['id'] / f'{name}.png') as im:
                sheet.paste(im, (c * 300 + (300 - im.width) // 2, 40 + r * 250))
        draw.text((8, 245 + r * 250), row['id'] + ' | red 0..90 degrees, cyan invalid/hit mismatch', fill='white')
    sheet.save(out / 'comparison.png')


if __name__ == '__main__':
    main()
