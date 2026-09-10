#!/usr/bin/env python3
"""Three-camera authored/white-diffuse/normal gates for diagnostic formula 85."""
import argparse
import json
import os
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw
from run_mandel_geometry_controls import geometry_overrides
from run_mandel_normal_controls import compare
from run_mandel_shadow_controls import replace_main_parameters
from run_mandel_support_suite import parameters
from run_release_canaries import difference, execute, sha256


def camera_offsets(text):
    values = parameters(text)
    vector = lambda name: np.array([float(v.replace(',', '.')) for v in values[name].split()])
    camera, target, top = (vector(v) for v in ('camera', 'target', 'camera_top'))
    if any(v.shape != (3,) or not np.isfinite(v).all() for v in (camera, target, top)):
        raise ValueError('invalid camera vectors')
    forward = target - camera
    distance = np.linalg.norm(forward)
    if not np.isfinite(distance) or distance <= 0:
        raise ValueError('invalid authored camera')
    right = np.cross(forward / distance, top)
    right_length = np.linalg.norm(right)
    if not np.isfinite(right_length) or right_length <= 0:
        raise ValueError('invalid camera up vector')
    right /= right_length
    shifts = {'authored': np.zeros(3), 'lateral': right * distance * .15,
              'forward': forward * .15}
    return {name: {key: ' '.join(f'{x:.17g}' for x in point + shift)
                  for key, point in [('camera', camera), ('target', target)]}
            for name, shift in shifts.items()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--probe', type=Path, required=True)
    parser.add_argument('--native-command', type=Path, required=True)
    parser.add_argument('--mandel-root', type=Path, required=True)
    parser.add_argument('--decoder', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve(); out.mkdir(parents=True, exist_ok=False)
    source = args.source.read_text()
    template = json.loads(args.native_command.read_text())
    width, height, spp, bounces = 400, 224, 32, 4
    report = {'scope': 'Fixed cameras shared through source-space camera/target translations. Authored 32-SPP four-bounce FPT versus native authored renderer, not identical integrators. Separate white headlight and unjittered normal/depth controls. No image registration.',
              'size': [width, height], 'spp': spp, 'bounce_cap': bounces,
              'identity': {str(p.resolve()): sha256(p) for p in [args.source, args.probe, args.decoder, Path(template[0])]}, 'rows': []}
    def save():
        (out / 'summary.json').write_text(json.dumps(report, indent=2) + '\n')
    for name, changes in camera_offsets(source).items():
        dest = out / name; dest.mkdir()
        scene = dest / 'scene.fract'
        # Authored camera keeps the original scene bytes, including decimal syntax.
        scene.write_text(source if name == 'authored' else replace_main_parameters(source, changes))
        row = {'view': name, 'camera_overrides': {} if name == 'authored' else changes,
               'source_sha256': sha256(scene), 'modes': {}, 'normal': {}}
        report['rows'].append(row); save()
        for mode in ['authored', 'geometry', 'normal']:
            native = list(template); native[-1] = str(scene)
            native[native.index('-r')+1] = f'{width}x{height}'
            folder = dest / f'native-{mode}'
            file = folder / ('scene.exr' if mode == 'normal' else 'scene.png')
            native[native.index('-o')+1] = str(file)
            if mode != 'authored':
                overrides = geometry_overrides(scene.read_text())
                if mode == 'normal':
                    overrides.update(zbuffer_enabled='1', normalWorld_enabled='1', normalWorld_quality='32')
                    native[native.index('-f')+1] = 'exr'
                native[native.index('-O')+1] += '#' + '#'.join(f'{k}={v}' for k,v in overrides.items())
            print(name, 'native', mode, flush=True)
            execute(native, folder, 360)
            if 'OpenCl - rendering' in (folder/'stdout.log').read_text():
                raise ValueError('native CPU required')
            if mode == 'normal':
                execute([str(args.decoder.resolve()), str(file), str(folder/'channels.bin')], folder/'decode', 30)
            for candidate in (False, True):
                label = 'analytic' if candidate else 'baseline'
                target = dest / f'{label}-{mode}'
                render = [str(args.probe.resolve())] + ([] if mode == 'normal' else ['beauty'])
                render += [str(scene), '--mandelbulber-root', str(args.mandel_root.resolve()),
                           '--width', str(width), '--height', str(height), '--samples', str(1 if mode == 'normal' else spp),
                           '--sdf-accumulation', 'chunked', '--sdf-chunk-samples', '1',
                           '--mandel-appearance', 'geometry' if mode == 'geometry' else 'authored-path',
                           '--sdf-bounce-cap', str(bounces), '--out', str(target)]
                if mode == 'normal': render += ['--mode', 'normal']
                print(name, label, mode, flush=True)
                execute(render, dest/f'{label}-{mode}-run', 360,
                        env=dict(os.environ, FPT_NORMAL_PROBE_ANALYTIC85='1' if candidate else '0'))
                if mode == 'normal':
                    n = np.fromfile(folder/'channels.bin', '<f4').reshape(height,width,4)
                    f = np.fromfile(target/'hits.bin', '<f4').reshape(height,width,20)
                    meta = json.loads((target/'summary.json').read_text())
                    row['normal'][label] = compare(n, f, meta['world_scale'])[0]
                else:
                    row['modes'].setdefault(mode, {})[label] = difference(file, target/'render.png')
                save()
        a,b = row['normal']['analytic'],row['normal']['baseline']
        row['checks'] = {
            'normal_median_improves': a['normal_degrees_all']['median'] < b['normal_degrees_all']['median'],
            'normal_p95_improves': a['normal_degrees_all']['p95'] < b['normal_degrees_all']['p95'],
            'depth_median_improves': a['depth_relative']['median'] < b['depth_relative']['median'],
            'no_new_native_visible_misses': a['visible_misses'] <= b['visible_misses'],
        }
        save()
    for mode in ['authored', 'geometry', 'normal']:
        sheet = Image.new('RGB', (width*3, 36+(height+36)*len(report['rows'])), '#1b1d20')
        draw = ImageDraw.Draw(sheet)
        for col, title in enumerate(['Mandel native', 'FPT baseline', 'FPT analytic derivative']):
            draw.text((col*width+8, 12), title, fill='white')
        for i,row in enumerate(report['rows']):
            dest = out/row['view']
            for col,label in enumerate(['native','baseline','analytic']):
                folder = dest/f'{label}-{mode}'
                if mode == 'normal' and label == 'native':
                    n = np.fromfile(folder/'channels.bin','<f4').reshape(height,width,4)
                    pixels = np.uint8(np.clip((n[..., [1,3,2]]+1)*127.5,0,255))
                    pixels[(~np.isfinite(n[...,0])) | (n[...,0] <= 0) | (n[...,0] >= 1e10)] = 0
                    image = Image.fromarray(pixels)
                else:
                    image = Image.open(folder/('scene.png' if label=='native' else 'render.png')).convert('RGB')
                sheet.paste(image, (col*width,36+i*(height+36)))
            draw.text((8,36+i*(height+36)+height+8), row['view']+' | '+mode, fill='white')
        sheet.save(out/f'{mode}-comparison.png')
    report['normal_depth_gate_passed'] = all(all(r['checks'].values()) for r in report['rows'])
    save()
    print(json.dumps(report,indent=2),flush=True)
    if not report['normal_depth_gate_passed']: raise SystemExit('normal/depth gate failed; retained reports are diagnostic only')


if __name__ == '__main__': main()
