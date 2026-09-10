#!/usr/bin/env python3
"""Native controls for omitted primitives, camera precision and stereo framing."""
import argparse
import json
import struct
from pathlib import Path

from PIL import Image, ImageDraw
from run_mandel_geometry_controls import headlight_command
from run_mandel_support_suite import capture, parameters
from run_release_canaries import difference, sha256


def precision_control(source):
    values = parameters(source)
    camera = [float(x.replace(',', '.')) for x in values['camera'].split()]
    target = [float(x.replace(',', '.')) for x in values['target'].split()]
    rounded = [struct.unpack('<f', struct.pack('<f', x))[0] for x in camera]
    shifted = [r + t - c for r, t, c in zip(rounded, target, camera)]
    return {'camera': ' '.join(format(x, '.17g') for x in rounded),
            'target': ' '.join(format(x, '.17g') for x in shifted)}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--audit', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--scenes', nargs='+', default=['379', '380', '567', '572', '602'])
    p.add_argument('--controls-file', type=Path,
                   help='JSON mapping scene IDs to [label, native post-load overrides] lists.')
    args = p.parse_args()
    audit = json.loads((args.audit/'summary.json').read_text())
    for path, digest in audit['identity']['executables'].items():
        if sha256(Path(path)) != digest:
            # FPT is not executed: baseline captures are verified below.
            if Path(path).name != 'fpt-metal':
                raise ValueError('native binary changed')
    args.output.mkdir(parents=True, exist_ok=False)
    report = {'scope': 'Native one-variable controls. Camera rounding is diagnostic, never a production framing fix.',
              'audit_sha256': sha256(args.audit/'summary.json'), 'rows': []}
    controls = json.loads(args.controls_file.read_text()) if args.controls_file else None
    if controls is not None:
        report['controls_sha256'] = sha256(args.controls_file)
    for scene_id in args.scenes:
        row = next(r for r in audit['rows'] if r['id'] == scene_id)
        template = json.loads((args.audit/scene_id/'mandel/command.json').read_text())
        source = Path(template[-1]).read_text()
        if sha256(Path(template[-1])) != row['sha256']:
            raise ValueError('source changed')
        fpt = row['modes']['geometry']['capture']
        if sha256(Path(fpt['path'])) != fpt['sha256']:
            raise ValueError('baseline capture changed')
        variants = [('neutral', {})]
        if scene_id in ('379', '380', '602'):
            variants.append(('no-water', {'primitive_water_1_enabled': 'false'}))
        if scene_id == '380':
            variants.append(('no-box', {'primitive_box_2_enabled': 'false'}))
        if scene_id in ('380', '567'):
            variants.append(('fp32-camera', precision_control(source)))
        if scene_id == '567':
            variants.append(('stale-de-function', {'delta_DE_function': '2'}))
        if scene_id == '572':
            variants.append(('forced-delta', {'delta_DE_method': '1'}))
        if controls is not None:
            variants = controls[scene_id]
        labels = [label for label, _ in variants]
        if not labels or len(labels) != len(set(labels)) or any(not s.replace('-', '').isalnum() for s in labels):
            raise ValueError('control labels must be unique safe directory names')
        for label, overrides in variants:
            folder = args.output/scene_id/label
            command, neutral = headlight_command(template, source, folder)
            if scene_id == '572':
                overrides = dict(overrides, stereo_enabled='false')
            command[command.index('-O')+1] += '#' + '#'.join(f'{k}={v}' for k,v in overrides.items())
            result = capture(command, folder, tuple(row['size']), 240)
            if "doesn't exists" in (folder/'stderr.log').read_text(errors='replace'):
                raise ValueError('unrecognized native override')
            if 'opencl - rendering' in (folder/'stdout.log').read_text(errors='replace').lower():
                raise ValueError('expected native CPU')
            record = dict(id=scene_id, variant=label, source_sha256=row['sha256'],
                          neutral_overrides=neutral, overrides=overrides, result=result)
            if result['status'] != 'ok':
                raise RuntimeError(result)
            record['rgb_diff_not_geometry_metric'] = difference(result['capture']['path'], fpt['path'])
            report['rows'].append(record)
            (args.output/'summary.json').write_text(json.dumps(report, indent=2)+'\n')
            print(scene_id, label, record['rgb_diff_not_geometry_metric'], flush=True)
    sheet = Image.new('RGB', (600, 40+len(report['rows'])*330), '#202326')
    draw = ImageDraw.Draw(sheet)
    draw.text((8, 10), 'Native controlled geometry / unchanged FPT geometry', fill='white')
    for i, item in enumerate(report['rows']):
        row = next(r for r in audit['rows'] if r['id']==item['id'])
        for col, path in enumerate([item['result']['capture']['path'], row['modes']['geometry']['capture']['path']]):
            with Image.open(path) as image:
                sheet.paste(image.convert('RGB'), (col*300, 40+i*330))
        draw.text((8, 40+i*330+305), item['id']+' '+item['variant'], fill='white')
    sheet.save(args.output/'comparison.png')


if __name__ == '__main__':
    main()
