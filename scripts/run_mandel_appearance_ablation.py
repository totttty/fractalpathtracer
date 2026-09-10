#!/usr/bin/env python3
"""One-setting-at-a-time authored appearance controls; no renderer modifications."""
import argparse
import json
import os
from pathlib import Path

from PIL import Image, ImageDraw
from run_mandel_shadow_controls import replace_main_parameters
from run_mandel_support_suite import capture
from run_release_canaries import difference, execute, scene_dimensions, sha256


def variants():
    no_post = {'DOF_enabled': 'false', 'post_chromatic_aberration_enabled': 'false'}
    no_specular = dict(no_post, mat1_specular='0', mat1_specular_plastic_enable='false',
                      mat1_specular_metallic='0', mat1_reflectance='0', raytraced_reflections='false')
    linear = dict(no_specular, brightness='1', contrast='1', gamma='1')
    return [
        ('original', None, {}, 0),
        ('no-chromatic', 'original', {'post_chromatic_aberration_enabled': 'false'}, 0),
        ('no-dof', 'original', {'DOF_enabled': 'false'}, 0),
        ('no-post', 'no-chromatic', no_post, 0),
        ('one-bounce', 'no-post', no_post, 1),
        ('diffuse', 'one-bounce', no_specular, 1),
        ('linear', 'diffuse', linear, 1),
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--audit', type=Path, required=True)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--scene', default='50')
    parser.add_argument('--max-axis', type=int, default=160)
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    native_template = json.loads((args.audit / args.scene / 'mandel/command.json').read_text())
    fpt_template = json.loads((args.audit / args.scene / 'authored/command.json').read_text())
    source = Path(native_template[-1])
    original = source.read_text()
    size = scene_dimensions(original, args.max_axis)
    report = {'scene': args.scene, 'size': size, 'samples': 32,
              'source_sha256': sha256(source),
              'binaries': {str(args.binary.resolve()): sha256(args.binary),
                           native_template[0]: sha256(Path(native_template[0]))},
              'scope': 'Authored settings with explicit ablations. Native CPU and FPT pixel sampling differ. No image registration.',
              'rows': []}
    for label, parent, overrides, bounces in variants():
        folder = out / label
        folder.mkdir()
        derived = folder / 'control.fract'
        derived.write_text(replace_main_parameters(original, overrides))
        row = {'variant': label, 'parent': parent, 'overrides': overrides,
               'bounce_cap': bounces, 'source_sha256': sha256(derived), 'modes': {}}
        native = native_template.copy()
        native[-1] = str(derived)
        native[native.index('-r') + 1] = f'{size[0]}x{size[1]}'
        native[native.index('-o') + 1] = str(folder / 'native/scene.png')
        row['modes']['native'] = capture(native, folder / 'native', size, 240)
        fpt = fpt_template.copy()
        fpt[0], fpt[2] = str(args.binary.resolve()), str(derived)
        for flag, value in [('--width', size[0]), ('--height', size[1]),
                            ('--samples', 32), ('--out', folder / 'fpt')]:
            fpt[fpt.index(flag) + 1] = str(value)
        if bounces:
            fpt += ['--sdf-bounce-cap', str(bounces)]
        row['modes']['fpt'] = capture(fpt, folder / 'fpt', size, 240,
            runner=lambda c, d, t: execute(c, d, t, env=dict(os.environ,
                FPT_MANDEL_TILED_DISPATCH='1', FPT_MANDEL_TILE_ROWS='16')))
        stderr = (folder / 'native/stderr.log').read_text(errors='replace')
        if "doesn't exists" in stderr:
            raise RuntimeError('native rejected a control parameter')
        if 'OpenCl - rendering' in (folder / 'native/stdout.log').read_text():
            raise RuntimeError('expected native CPU')
        if all(v['status'] == 'ok' for v in row['modes'].values()):
            row['diff'] = difference(row['modes']['native']['capture']['path'],
                                     row['modes']['fpt']['capture']['path'])
            if parent:
                previous = next(r for r in report['rows'] if r['variant'] == parent)
                row['parent_diff'] = {mode: difference(previous['modes'][mode]['capture']['path'],
                    row['modes'][mode]['capture']['path']) for mode in ['native', 'fpt']}
        report['rows'].append(row)
        (out / 'summary.json').write_text(json.dumps(report, indent=2) + '\n')
        print(label, {k: v['status'] for k, v in row['modes'].items()}, row.get('diff'), flush=True)
        if any(v['status'] != 'ok' for v in row['modes'].values()):
            raise RuntimeError('capture failed; inspect summary')
    sheet = Image.new('RGB', (600, 40 + len(report['rows']) * 210), '#191c1e')
    draw = ImageDraw.Draw(sheet)
    draw.text((10, 10), 'Native Mandel CPU', fill='white')
    draw.text((310, 10), 'FPT Metal authored', fill='white')
    for y, row in enumerate(report['rows']):
        for x, mode in enumerate(['native', 'fpt']):
            with Image.open(row['modes'][mode]['capture']['path']) as im:
                sheet.paste(im.convert('RGB'), (x * 300 + (300 - im.width) // 2, 40 + y * 210))
        draw.text((10, 215 + y * 210), f'{row["variant"]} | MAE {row["diff"]["mae"]:.6f}', fill='white')
    sheet.save(out / 'comparison.png')


if __name__ == '__main__':
    main()
