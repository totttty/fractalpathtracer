#!/usr/bin/env python3
"""Summarize diagnostic ray-march trials without promoting them."""
import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw
from run_release_canaries import difference, sha256
from run_mandel_normal_controls import compare


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--experiment', type=Path, required=True)
    parser.add_argument('--native-diffuse', type=Path, required=True)
    parser.add_argument('--native-mask', type=Path, required=True)
    parser.add_argument('--native-channels', type=Path,
                        help='decoded native EXR: depth and signed XYZ normal, float4 per pixel')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    size = (300, 169)
    native_channels = None
    if args.native_channels:
        if args.native_channels.stat().st_size != size[0] * size[1] * 16:
            raise ValueError('invalid native EXR channel buffer')
        native_channels = np.fromfile(args.native_channels, dtype='<f4').reshape(size[1], size[0], 4)
    with Image.open(args.native_mask) as image:
        if image.size != size:
            raise ValueError('native mask dimensions differ')
        native_mask = np.asarray(image.convert('RGB')).max(axis=2) > 127
    with Image.open(args.native_diffuse) as image:
        if image.size != size:
            raise ValueError('native diffuse dimensions differ')
    variants = [('baseline', 'Current FPT (diagnostic)'),
                ('parametric', 'Ray-origin + accumulated distance'),
                ('preferred-ifs', 'Native-migrated IFS setting'),
                ('preferred-ifs-parametric', 'Both changes')]
    rows = []
    sheet = Image.new('RGB', (900, 4 * 221 + 36), '#202326')
    draw = ImageDraw.Draw(sheet)
    for col, label in enumerate(('Mandelbulber white diffuse', 'FPT diagnostic trial', 'FPT hit mask')):
        draw.text((col * 300 + 8, 10), label, fill='white')
    for index, (variant, label) in enumerate(variants):
        folder = args.experiment / variant
        report = json.loads((folder / 'summary.json').read_text())
        if (report['width'], report['height']) != size or report['diagnostic_mode'] != 8:
            raise ValueError('unexpected diagnostic configuration')
        path = folder / 'hits.bin'
        if path.stat().st_size != size[0] * size[1] * 80:
            raise ValueError('invalid structural dump')
        records = np.fromfile(path, dtype='<f4').reshape(size[1], size[0], 20)
        mask = records[:, :, 7] > 0.5
        miss = native_mask & ~mask
        extra = ~native_mask & mask
        row = dict(variant=variant, label=label, hit_count=int(mask.sum()),
                   coverage_pct=float(mask.mean() * 100),
                   native_visible_miss_pct=float(miss.sum() / max(1, native_mask.sum()) * 100),
                   extra_pixels=int(extra.sum()),
                   rgb_difference_not_geometry_score=difference(str(args.native_diffuse), str(folder / 'render.png')),
                   diagnostic=report, input_sha256={name: sha256(folder / name)
                       for name in ('summary.json', 'hits.bin', 'render.png')})
        rows.append(row)
        if native_channels is not None:
            row['native_depth_comparison'] = compare(native_channels, records, report['world_scale'])[0]
        top = 36 + index * 221
        for col, path in enumerate((args.native_diffuse, folder / 'render.png')):
            with Image.open(path) as image:
                if image.size != size:
                    raise ValueError('image dimensions differ')
                sheet.paste(image.convert('RGB'), (col * 300, top))
        sheet.paste(Image.fromarray((mask * 255).astype('uint8')).convert('RGB'), (600, top))
        draw.text((8, top + 176), label, fill='white')
        detail = f'Coverage {row["coverage_pct"]:.2f}% | native-visible miss {row["native_visible_miss_pct"]:.2f}%'
        if native_channels is not None:
            depth = row['native_depth_comparison']['depth_relative']
            detail += f' | depth error median {depth["median"] * 100:.2f}%'
        draw.text((8, top + 193), detail + ' | not promoted', fill='#ffce86')
    args.output.mkdir(parents=True, exist_ok=False)
    sheet.save(args.output / 'comparison.png')
    result = dict(rows=rows, native_hit_count=int(native_mask.sum()),
                  reference_sha256={str(path): sha256(path) for path in (args.native_diffuse, args.native_mask)},
                  limitations=['One center ray per FPT pixel; native sampling differs.',
                               'Coverage is diagnostic, not an exact parity gate.',
                               'All candidates rejected; no production change or performance claim.'],
                  harness_sha256=sha256(Path(__file__)), sheet_sha256=sha256(args.output / 'comparison.png'))
    if args.native_channels:
        result['native_channels_sha256'] = sha256(args.native_channels)
    (args.output / 'summary.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps([{key: row[key] for key in ('variant', 'coverage_pct', 'native_visible_miss_pct', 'extra_pixels')} for row in rows], indent=2))


if __name__ == '__main__':
    main()
