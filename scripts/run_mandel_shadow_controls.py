#!/usr/bin/env python3
"""Native CPU versus baseline/candidate FPT under isolated main lighting."""
import argparse
import json
import os
from pathlib import Path

from PIL import Image, ImageDraw
from run_mandel_geometry_controls import geometry_overrides
from run_mandel_support_suite import capture
from run_release_canaries import difference, execute, scene_dimensions, sha256


def replace_main_parameters(source, overrides):
    lines, seen, inside, found = [], set(), False, False
    for line in source.splitlines():
        stripped = line.strip()
        if stripped.startswith('[') and stripped.endswith(']'):
            if inside:
                lines.extend(f'{k} {v};' for k, v in overrides.items() if k not in seen)
            inside = stripped == '[main_parameters]'
            found |= inside
        if inside and stripped.endswith(';') and ' ' in stripped:
            key = stripped.split(None, 1)[0]
            if key in overrides:
                if key not in seen:
                    lines.append(f'{key} {overrides[key]};')
                    seen.add(key)
                continue
        lines.append(line)
    if inside:
        lines.extend(f'{k} {v};' for k, v in overrides.items() if k not in seen)
    if not found:
        raise ValueError('missing main_parameters section')
    return '\n'.join(lines) + '\n'


def control_overrides(source, penetrating, cone):
    values = geometry_overrides(source)
    values.update({
        'light1_rotation': '-45 45 0', 'light1_cast_shadows': '1',
        'light1_penetrating': str(int(penetrating)),
        'light1_soft_shadow_cone': str(cone), 'MC_soft_shadows_enable': '0',
        'iteration_threshold_mode': '0', 'interior_mode': '0',
    })
    for key, value in list(values.items()):
        if value in ('0', '1') and (
            'enable' in key or 'use_' in key or 'cast_shadows' in key
            or key.endswith(('_relative_position', '_volumetric', '_penetrating'))
            or key in ('raytraced_reflections', 'DOF_monte_carlo', 'DOF_MC_global_illumination',
                       'random_lights_group', 'textured_background', 'interior_mode', 'iteration_threshold_mode')
        ):
            values[key] = 'true' if value == '1' else 'false'
    return values


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--audit', type=Path, required=True)
    parser.add_argument('--baseline', type=Path, required=True)
    parser.add_argument('--candidate', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--scenes', nargs='+', default=['14', '30'])
    parser.add_argument('--max-axis', type=int, default=200)
    parser.add_argument('--samples', type=int, default=32)
    parser.add_argument('--candidate-label', default='FPT corrected shadows')
    parser.add_argument('--variants', nargs='+', default=['hard-penetrating', 'hard-opaque', 'soft-penetrating'],
                        choices=['hard-penetrating', 'hard-opaque', 'soft-penetrating', 'no-shadows'])
    args = parser.parse_args()
    audit, out = args.audit.resolve(), args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    report = {
        'scope': 'Main directional light only; white diffuse; no AO, specular, fog/clouds, glow, DOF, other lights, or secondary bounces. Native CPU sampling differs from FPT 32-SPP jitter.',
        'binaries': {str(p.resolve()): sha256(p) for p in (args.baseline, args.candidate)},
        'max_axis': args.max_axis, 'samples': args.samples, 'variants': args.variants,
        'candidate_label': args.candidate_label,
        'harness_sha256': sha256(Path(__file__)), 'rows': [],
    }
    for identifier in args.scenes:
        original = json.loads((audit / identifier / 'mandel/command.json').read_text())
        source = Path(original[-1])
        original_text = source.read_text()
        version = next(l.split()[-1] for l in original_text.splitlines() if l.startswith('# version'))
        if tuple(map(int, version.split('.')[:2])) < (2, 25):
            raise ValueError('This control requires modern light fields; do not silently reinterpret legacy scenes')
        size = scene_dimensions(original_text, args.max_axis)
        for variant in args.variants:
            penetrating = variant != 'hard-opaque'
            cone = 5 if variant == 'soft-penetrating' else 0
            folder = out / identifier / variant
            folder.mkdir(parents=True)
            overrides = control_overrides(original_text, penetrating, cone)
            if variant == 'no-shadows':
                overrides['light1_cast_shadows'] = 'false'
            derived = folder / 'control.fract'
            derived.write_text(replace_main_parameters(original_text, overrides))
            row = {'id': identifier, 'variant': variant, 'source_sha256': sha256(source),
                   'control_sha256': sha256(derived), 'overrides': overrides, 'size': size, 'modes': {}}
            report['rows'].append(row)
            native = original.copy()
            native[-1] = str(derived)
            native[native.index('-r') + 1] = f'{size[0]}x{size[1]}'
            native[native.index('-o') + 1] = str(folder / 'native/scene.png')
            report['binaries'][native[0]] = sha256(Path(native[0]))
            row['modes']['native'] = capture(native, folder / 'native', size, 240)
            if 'OpenCl - rendering' in (folder / 'native/stdout.log').read_text():
                raise RuntimeError('expected native CPU rendering')
            for label, binary in [('baseline', args.baseline), ('candidate', args.candidate)]:
                command = json.loads((audit / identifier / 'authored/command.json').read_text())
                command[0], command[2] = str(binary.resolve()), str(derived)
                for flag, value in [('--width', size[0]), ('--height', size[1]), ('--samples', args.samples), ('--out', folder / label)]:
                    command[command.index(flag) + 1] = str(value)
                command += ['--sdf-bounce-cap', '1']
                row['modes'][label] = capture(command, folder / label, size, 240, runner=lambda cmd, dest, timeout: execute(
                    cmd, dest, timeout, env=dict(os.environ, FPT_MANDEL_TILED_DISPATCH='1', FPT_MANDEL_TILE_ROWS='16')))
                if row['modes']['native']['status'] == row['modes'][label]['status'] == 'ok':
                    row[label + '_diff'] = difference(row['modes']['native']['capture']['path'], row['modes'][label]['capture']['path'])
            (out / 'summary.json').write_text(json.dumps(report, indent=2) + '\n')
            print(identifier, variant, {k:v['status'] for k,v in row['modes'].items()}, {k:v for k,v in row.items() if k.endswith('_diff')}, flush=True)
    sheet = Image.new('RGB', (900, 40 + 250 * len(report['rows'])), '#191c1e')
    draw = ImageDraw.Draw(sheet)
    for col, label in enumerate(['Native CPU direct diffuse', 'FPT before', args.candidate_label]):
        draw.text((col * 300 + 8, 12), label, fill='white')
    for index, row in enumerate(report['rows']):
        for col, label in enumerate(['native', 'baseline', 'candidate']):
            result = row['modes'][label]
            if result.get('capture'):
                with Image.open(result['capture']['path']) as im:
                    im = im.convert('RGB')
                    sheet.paste(im, (col*300+(300-im.width)//2, 40+index*250+(200-im.height)//2))
            else:
                draw.text((col*300+8, 100+index*250), result['status'], fill='white')
        draw.text((8, 250+index*250), f'{row["id"]} {row["variant"]} {row["size"]}', fill='white')
    sheet.save(out / 'comparison.png')
    if any(v['status'] != 'ok' for r in report['rows'] for v in r['modes'].values()):
        raise SystemExit('incomplete capture gate')


if __name__ == '__main__':
    main()
