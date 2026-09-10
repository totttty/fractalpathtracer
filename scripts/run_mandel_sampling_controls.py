#!/usr/bin/env python3
"""Bounded diagnostic sampling A/B; never changes production sampling defaults."""
import argparse
import json
from pathlib import Path

from PIL import Image, ImageDraw
from run_release_canaries import difference, execute, sha256


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--controls', type=Path, required=True)
    parser.add_argument('--probe', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--scenes', nargs='+', default=['14', '30'])
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    report = {'scope': 'Diagnostic kernel comparison; native pixel positions versus jittered production renderPath. Not byte-equivalent backends, not a recommendation to remove anti-aliasing.',
              'probe_sha256': sha256(args.probe), 'jitter_probe_max_production_mae': .001, 'rows': []}
    for identifier in args.scenes:
        base = args.controls.resolve() / identifier / 'no-shadows'
        original = json.loads((base / 'candidate/command.json').read_text())
        width, height = [original[original.index(flag) + 1] for flag in ['--width', '--height']]
        row = {'id': identifier, 'size': [int(width), int(height)], 'modes': {},
               'source_sha256': sha256(base / 'control.fract')}
        report['rows'].append(row)
        for mode, samples in [('jittered', 32), ('native', 32), ('native-single', 1)]:
            dest = out / identifier / mode
            cmd = [str(args.probe.resolve()), str(base / 'control.fract'),
                   original[original.index('--mandelbulber-root') + 1], width, height,
                   str(samples), 'native' if mode == 'native-single' else mode, str(dest / 'capture')]
            execute(cmd, dest, 240)
            png = dest / 'capture/probe.png'
            row['modes'][mode] = {'path': str(png), 'png_sha256': sha256(png),
                                 'native_diff': difference(base / 'native/scene.png', png),
                                 'production_diff': difference(base / 'candidate/control.png', png)}
            print(identifier, mode, row['modes'][mode], flush=True)
            (out / 'summary.json').write_text(json.dumps(report, indent=2) + '\n')
        row['native_sample_count_diff'] = difference(row['modes']['native']['path'], row['modes']['native-single']['path'])
        row['probe_gate_passed'] = (row['modes']['jittered']['production_diff']['mae'] <= .001
                                    and row['native_sample_count_diff']['changed_pixels'] == 0)
        (out / 'summary.json').write_text(json.dumps(report, indent=2) + '\n')
    sheet = Image.new('RGB', (1200, 40 + len(report['rows']) * 250), '#191c1e')
    draw = ImageDraw.Draw(sheet)
    for col, title in enumerate(['Native single-ray diffuse', 'Production FPT 32 SPP', 'Probe jittered 32 SPP', 'Probe native pixels / no jitter']):
        draw.text((col * 300 + 7, 10), title, fill='white')
    for index, row in enumerate(report['rows']):
        base = args.controls.resolve() / row['id'] / 'no-shadows'
        paths = [base / 'native/scene.png', base / 'candidate/control.png',
                 Path(row['modes']['jittered']['path']), Path(row['modes']['native']['path'])]
        for col, path in enumerate(paths):
            with Image.open(path) as im:
                sheet.paste(im.convert('RGB'), (col * 300 + (300 - im.width) // 2, 40 + index * 250))
        draw.text((8, 245 + index * 250), f"{row['id']} | same white direct light, no AO/shadows/specular; sampling experiment only", fill='white')
    sheet.save(out / 'comparison.png')
    if not all(row['probe_gate_passed'] for row in report['rows']):
        raise SystemExit('sampling probe failed its production/single-sample sanity gate')


if __name__ == '__main__':
    main()
