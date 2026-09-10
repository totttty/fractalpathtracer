#!/usr/bin/env python3
"""Native/FPT light2-only white directional controls with authored light rotation."""
import argparse
import json
import os
from pathlib import Path

from PIL import Image, ImageDraw
from run_mandel_shadow_controls import control_overrides, replace_main_parameters
from run_mandel_support_suite import capture, parameters
from run_release_canaries import difference, execute, scene_dimensions, sha256


def auxiliary_overrides(source, variant):
    values = control_overrides(source, True, 5 if variant == 'soft' else 0)
    # Transfer the complete isolated directional setup, then turn off light1.
    values.update({key.replace('light1_', 'light2_'): value for key, value in list(values.items()) if key.startswith('light1_')})
    values['light1_enabled'] = 'false'
    values['light2_rotation'] = parameters(source).get('light2_rotation', '0 0 0')
    values['light2_cast_shadows'] = 'false' if variant == 'no-shadows' else 'true'
    values['all_lights_intensity'] = '1'
    return values


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--audit', type=Path, required=True)
    parser.add_argument('--baseline', type=Path, required=True)
    parser.add_argument('--candidate', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--scenes', nargs='+', default=['21', '42', '50'])
    parser.add_argument('--variants', nargs='+', default=['no-shadows', 'hard', 'soft'], choices=['no-shadows', 'hard', 'soft'])
    args = parser.parse_args()
    out = args.output.resolve(); out.mkdir(parents=True, exist_ok=False)
    report = {'scope': 'Light2 only; authored rotation, white diffuse, intensity 1, no AO/specular/fog/fake lights. Native CPU vs FPT 32 SPP; sampling differs.',
              'binaries': {str(p.resolve()): sha256(p) for p in (args.baseline,args.candidate)}, 'rows': []}
    for identifier in args.scenes:
        original = json.loads((args.audit / identifier / 'mandel/command.json').read_text())
        source = Path(original[-1]); text = source.read_text()
        version = next(line.split()[-1] for line in text.splitlines() if line.startswith('# version'))
        if tuple(map(int, version.split('.')[:2])) < (2,25):
            raise ValueError('auxiliary control requires a modern source')
        size = scene_dimensions(text,200)
        for variant in args.variants:
            folder = out / identifier / variant; folder.mkdir(parents=True)
            overrides = auxiliary_overrides(text, variant)
            derived = folder / 'control.fract'; derived.write_text(replace_main_parameters(text, overrides))
            native = original.copy(); native[-1] = str(derived)
            native[native.index('-r')+1] = f'{size[0]}x{size[1]}'
            native[native.index('-o')+1] = str(folder/'native/scene.png')
            report['binaries'][native[0]] = sha256(Path(native[0]))
            row = {'id':identifier,'variant':variant,'size':size,'source_sha256':sha256(source),'control_sha256':sha256(derived),'overrides':overrides,'modes':{}}
            report['rows'].append(row)
            row['modes']['native'] = capture(native,folder/'native',size,240)
            if 'OpenCl - rendering' in (folder/'native/stdout.log').read_text():
                raise RuntimeError('expected native CPU capture')
            for label,binary in [('baseline',args.baseline),('candidate',args.candidate)]:
                cmd = json.loads((args.audit/identifier/'authored/command.json').read_text())
                cmd[0],cmd[2] = str(binary.resolve()),str(derived)
                for flag,value in [('--width',size[0]),('--height',size[1]),('--samples',32),('--out',folder/label)]:
                    cmd[cmd.index(flag)+1] = str(value)
                cmd += ['--sdf-bounce-cap','1']
                result = capture(cmd,folder/label,size,240,runner=lambda c,d,t:execute(c,d,t,env=dict(os.environ,FPT_MANDEL_TILED_DISPATCH='1',FPT_MANDEL_TILE_ROWS='16')))
                # Missing auxiliary support may trigger the renderer's black-image
                # guard. Preserve that failure; use its real PNG only for diagnosis.
                image = result.get('capture',result.get('diagnostic_capture'))
                if image and row['modes']['native'].get('capture'):
                    row[label+'_diff'] = difference(folder/'native/scene.png',image['path'])
                row['modes'][label] = result
            print(identifier,variant,{k:v['status'] for k,v in row['modes'].items()}, {k:v for k,v in row.items() if k.endswith('_diff')},flush=True)
            (out/'summary.json').write_text(json.dumps(report,indent=2)+'\n')
    sheet=Image.new('RGB',(900,40+250*len(report['rows'])),'#191c1e');draw=ImageDraw.Draw(sheet)
    for c,title in enumerate(['Native light2-only diffuse','FPT previous (auxiliary absent)','FPT auxiliary directional']):draw.text((c*300+7,10),title,fill='white')
    for r,row in enumerate(report['rows']):
        for c,label in enumerate(['native','baseline','candidate']):
            mode=row['modes'][label]; im=mode.get('capture',mode.get('diagnostic_capture'))
            if im:
                with Image.open(im['path']) as image:sheet.paste(image.convert('RGB'),(c*300+(300-image.width)//2,40+r*250))
            else:draw.text((c*300+7,100+r*250),mode['status'],fill='white')
        draw.text((8,245+r*250),row['id']+' | '+row['variant'],fill='white')
    sheet.save(out/'comparison.png')
    if any(r['modes'][m]['status'] != 'ok' for r in report['rows'] for m in ['native','candidate']):
        raise SystemExit('incomplete native/candidate control gate')


if __name__ == '__main__':main()
