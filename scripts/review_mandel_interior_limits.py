#!/usr/bin/env python3
"""Matched-camera interior/clipping ablation, with native CPU controls."""
import argparse
import json
from pathlib import Path

from PIL import Image, ImageDraw
from run_mandel_geometry_controls import headlight_command
from run_mandel_shadow_controls import replace_main_parameters
from run_release_canaries import difference, execute, image_result, mandel_reference_command, sha256


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('scene', 'baseline', 'candidate', 'native', 'source-root', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    source = args.scene.read_text()
    report = dict(source=str(args.scene.resolve()), source_sha256=sha256(args.scene),
                  binaries={str(p.resolve()): sha256(p) for p in (args.baseline, args.candidate, args.native)},
                  dimensions=[300, 158], samples=32, harness_sha256=sha256(Path(__file__)), rows=[],
                  limitations=['Continuous FPT only, not NAADF/CVOX validation.',
                               'Native sampling and lighting differ from FPT. RGB MAE is not geometry IoU.',
                               'Stereo is explicitly disabled. No camera, exposure or image registration changes.'])
    lightmap = args.source_root / 'deploy/share/mandelbulber2/textures/lightmap.jpg'
    for label, interior, limits in [('neither', False, False), ('limits', False, True),
                                    ('interior', True, False), ('both', True, True)]:
        overrides = dict(stereo_enabled='false', interior_mode=str(interior).lower(),
                         limits_enabled=str(limits).lower())
        control = out / label / 'scene.fract'
        control.parent.mkdir()
        control.write_text(replace_main_parameters(source, overrides))
        for mode in (['geometry', 'authored'] if label == 'both' else ['geometry']):
            row = dict(variant=label, mode=mode, overrides=overrides, control_sha256=sha256(control), captures={})
            for renderer, binary in [('native', args.native), ('baseline', args.baseline), ('candidate', args.candidate)]:
                folder = out / label / mode / renderer
                if renderer == 'native':
                    command = mandel_reference_command(binary.resolve(), control, (300, 158), folder / 'scene.png', lightmap)
                    if mode == 'geometry':
                        command, _ = headlight_command(command, control.read_text(), folder)
                else:
                    command = [str(binary.resolve()), 'render', str(control), '--mandelbulber-root',
                               str(args.source_root.resolve()), '--width', '300', '--height', '158',
                               '--samples', '32', '--sdf-accumulation', 'chunked', '--sdf-chunk-samples', '1',
                               '--mandel-appearance', 'authored-path' if mode == 'authored' else mode,
                               '--out', str(folder)]
                wall = execute(command, folder, 300)
                row['captures'][renderer] = dict(image_result(folder, (300, 158)), wall_seconds=wall)
            for renderer in ('baseline', 'candidate'):
                row[renderer + '_difference'] = difference(row['captures']['native']['path'], row['captures'][renderer]['path'])
            report['rows'].append(row)
            (out / 'summary.json').write_text(json.dumps(report, indent=2) + '\n')
            print(label, mode, row['baseline_difference']['mae'], row['candidate_difference']['mae'], flush=True)
    sheet = Image.new('RGB', (900, 40 + len(report['rows']) * 204), '#202326')
    draw = ImageDraw.Draw(sheet)
    for col, label in enumerate(('Mandelbulber CPU', 'FPT before', 'FPT corrected')):
        draw.text((col * 300 + 8, 12), label, fill='white')
    for i, row in enumerate(report['rows']):
        top = 40 + i * 204
        for col, renderer in enumerate(('native', 'baseline', 'candidate')):
            with Image.open(row['captures'][renderer]['path']) as image:
                sheet.paste(image.convert('RGB'), (col * 300, top))
        draw.text((8, top + 164), f"{row['variant']} | {row['mode']} | clipping/interior controls; identical camera", fill='white')
        draw.text((8, top + 181), f"RGB MAE {row['baseline_difference']['mae']:.4f} -> {row['candidate_difference']['mae']:.4f} (appearance diagnostic only)", fill='#cccccc')
    sheet.save(out / 'comparison.png')
    report['sheet_sha256'] = sha256(out / 'comparison.png')
    selected = [row for row in report['rows'] if row['variant'] == 'both']
    compact = Image.new('RGB', (900, 40 + len(selected) * 204), '#202326')
    compact.paste(sheet.crop((0, 0, 900, 40)), (0, 0))
    for index, row in enumerate(selected):
        start = 40 + report['rows'].index(row) * 204
        compact.paste(sheet.crop((0, start, 900, start + 204)), (0, 40 + index * 204))
    compact.save(out / 'authored-settings-comparison.png')
    report['compact_sheet_sha256'] = sha256(out / 'authored-settings-comparison.png')
    (out / 'summary.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
