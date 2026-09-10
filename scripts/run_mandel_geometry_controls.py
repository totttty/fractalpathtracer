#!/usr/bin/env python3
"""Supplement an immutable support audit with unshadowed headlight controls."""
import argparse
import json
import re
from pathlib import Path

from PIL import Image, ImageDraw
from run_mandel_support_suite import capture, parameters
from run_release_canaries import difference, sha256, verify_lightmap


def geometry_overrides(source):
    """Match FPT geometry mode's white Lambert material and directional key.

    These are post-load CLI overrides: do not change scene hashes or formula
    specialization. Only address lights that the native loader instantiates.
    """
    values = parameters(source)
    materials = {'1'} | {m.group(1) for key in values if (m := re.match(r'mat(\d+)_', key))}
    lights = {'1'} | {m.group(1) for key in values if (m := re.match(r'light(\d+)_', key))}
    overrides = {
        'basic_fog_enabled': '0', 'volumetric_fog_enabled': '0',
        'iteration_fog_enable': '0', 'clouds_enable': '0', 'glow_enabled': '0',
        'ambient_occlusion_enabled': '0', 'raytraced_reflections': '0',
        'DOF_enabled': '0', 'DOF_monte_carlo': '0',
        'post_chromatic_aberration_enabled': '0',
        'DOF_MC_global_illumination': '0', 'textured_background': '0',
        'background_color_1': '0000 0000 0000',
        'background_color_2': '0000 0000 0000',
        'background_color_3': '0000 0000 0000',
        'brightness': '1', 'contrast': '1', 'gamma': '1',
        'random_lights_group': '0', 'fake_lights_enabled': '0',
        'light1_enabled': '1', 'light1_type': '0',
        'light1_relative_position': '1', 'light1_rotation': '0 0 0',
        'light1_use_target_point': '0', 'light1_intensity': '1',
        'light1_color': 'ffff ffff ffff', 'light1_cast_shadows': '0',
        'light1_volumetric': '0',
    }
    for light in sorted(lights - {'1'}):
        overrides[f'light{light}_enabled'] = '0'
    for material in sorted(materials):
        for key, value in {
            'use_colors_from_palette': '0', 'surface_gradient_enable': '0',
            'surface_color': 'ffff ffff ffff', 'specular': '0',
            'specular_plastic_enable': '0', 'specular_metallic': '0',
            'reflectance': '0', 'iridescence_enabled': '0', 'luminosity': '0',
            'transparency_of_surface': '0', 'shading': '1',
            'use_color_texture': '0', 'use_normal_map_texture': '0',
        }.items():
            overrides[f'mat{material}_{key}'] = value
    return overrides


def headlight_command(command, source, folder):
    command = list(command)
    overrides = geometry_overrides(source)
    command[command.index('-O') + 1] += '#' + '#'.join(f'{k}={v}' for k, v in overrides.items())
    command[command.index('-o') + 1] = str(folder / 'scene.png')
    return command, overrides


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--audit', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--scenes', nargs='+', default=['25', '38'])
    parser.add_argument('--timeout', type=int, default=900)
    args = parser.parse_args()
    audit = args.audit.resolve()
    output = args.output.resolve()
    if output.exists():
        parser.error('use a new output directory; completed controls are immutable')
    summary = json.loads((audit / 'summary.json').read_text())
    rows = []
    sources = {}
    for scene_id in args.scenes:
        row = next((r for r in summary['rows'] if r['id'] == scene_id), None)
        if row is None:
            parser.error(f'unknown audit scene {scene_id}')
        rows.append(row)
        sources[scene_id] = Path(json.loads((audit / scene_id / 'mandel/command.json').read_text())[-1])
    for path, digest in summary['identity']['executables'].items():
        if sha256(Path(path)) != digest:
            parser.error(f'executable differs from audited checkpoint: {path}')
    for row in rows:
        if sha256(sources[row['id']]) != row['sha256']:
            parser.error(f'source differs from audit: {row["id"]}')
        verify_lightmap(summary['identity']['lightmaps'][row['id']])
        image = row['modes']['geometry']['capture']
        if sha256(Path(image['path'])) != image['sha256']:
            parser.error(f'geometry capture differs from audit: {row["id"]}')
    output.mkdir(parents=True)
    result = {
        'audit_sha256': sha256(audit / 'summary.json'),
        'renderer_changed': False,
        'lighting': 'white Lambert; unit camera-facing directional light; no shadows/AO',
        'limitations': ['FPT uses stored 32-SPP captures; native CPU raster sampling is different.',
                        'Normal estimators and floating-point precision differ; RGB error is not geometric error.',
                        'Controls remove authored appearance; they do not certify authored parity.'],
        'rows': [],
    }
    canvas = Image.new('RGB', (900, 40 + len(rows)*350), '#202326')
    draw = ImageDraw.Draw(canvas)
    for i, label in enumerate(('Mandel CPU authored', 'Mandel CPU geometry headlight', 'FPT Metal geometry (unchanged)')):
        draw.text((i*300+8, 12), label, fill='white')
    for index, row in enumerate(rows):
        scene_id = row['id']
        folder = output / scene_id
        command, overrides = headlight_command(
            json.loads((audit / scene_id / 'mandel/command.json').read_text()),
            sources[scene_id].read_text(), folder)
        print(scene_id, 'native headlight control', flush=True)
        outcome = capture(command, folder, tuple(row['size']), args.timeout)
        if sha256(sources[scene_id]) != row['sha256']:
            raise RuntimeError(f'{scene_id}: source changed during control capture')
        log = (folder / 'stderr.log').read_text(errors='replace')
        if 'doesn\'t exists' in log:
            raise RuntimeError(f'{scene_id}: native control contains unrecognized overrides; inspect stderr')
        if 'OpenCl - rendering' in (folder / 'stdout.log').read_text(errors='replace'):
            raise RuntimeError('expected CPU reference, not OpenCL')
        record = dict(id=scene_id, source_sha256=row['sha256'], overrides=overrides, result=outcome)
        if outcome['status'] == 'ok':
            record['appearance_difference_not_geometry_metric'] = difference(
                outcome['capture']['path'], row['modes']['geometry']['capture']['path'])
        result['rows'].append(record)
        (output / 'summary.json').write_text(json.dumps(result, indent=2)+'\n')
        for col, item in enumerate((row['modes']['mandel'], outcome, row['modes']['geometry'])):
            top = 40 + index*350
            if item.get('capture'):
                with Image.open(item['capture']['path']) as image:
                    image = image.convert('RGB')
                    image.thumbnail((300, 300))
                    canvas.paste(image, (col*300+(300-image.width)//2, top+(300-image.height)//2))
            else:
                draw.text((col*300+8, top+140), item['status'], fill='white')
        draw.text((8, 40+index*350+305), scene_id+' '+Path(row['path']).stem, fill='white')
        draw.text((8, 40+index*350+324), 'Same camera/formulas. No image flips, crops or registration. Sampling/normals differ.', fill='white')
        canvas.save(output / 'comparison.png')
        print(scene_id, outcome['status'], flush=True)
    if any(row['result']['status'] != 'ok' for row in result['rows']):
        raise SystemExit('one or more native controls failed; inspect summary.json')


if __name__ == '__main__':
    main()
