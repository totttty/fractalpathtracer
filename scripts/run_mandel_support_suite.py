#!/usr/bin/env python3
"""Resumable support audit; execution, geometry and appearance are separate claims."""
import argparse
import csv
import json
import os
import re
import shutil
import subprocess
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw, ImageStat
from run_release_canaries import (ROOT, difference, execute, image_result,
    lightmap_asset, mandel_reference_command, scene_dimensions, sha256,
    validate_manifest, verify_lightmap)

MODES = ('geometry', 'authored', 'mandel')


def parameters(text):
    section = None
    values = {}
    for line in text.splitlines():
        line = line.strip()
        if line.startswith('[') and line.endswith(']'):
            section = line[1:-1]
        elif section == 'main_parameters' and line.endswith(';'):
            parts = line[:-1].split(None, 1)
            if parts:
                values[parts[0]] = parts[1].strip() if len(parts) == 2 else ''
    return values


def resolve_lightmap(scene, source_root, default):
    value = parameters(scene.read_text()).get('file_lightmap')
    if value is None:
        return lightmap_asset(default)
    normalized = value.replace('\\', '/')
    if normalized.startswith('$SHARED_DIR/'):
        relative = normalized[len('$SHARED_DIR/'):]
        for parent in (scene.parent, *scene.parent.parents):
            if (parent / relative).is_file():
                return lightmap_asset(parent / relative)
    path = Path(normalized)
    if path.is_absolute() and path.is_file():
        return lightmap_asset(path)
    if (scene.parent / path).is_file():
        return lightmap_asset(scene.parent / path)
    for parent in (scene.parent, *scene.parent.parents):
        candidate = parent / 'textures' / path.name
        if candidate.is_file():
            return lightmap_asset(candidate)
    raise ValueError('missing authored lightmap: '+value)


def failure_kind(error, stderr):
    if isinstance(error, subprocess.TimeoutExpired):
        return 'timeout'
    text = (str(error)+'\n'+stderr).lower()
    if any(s in text for s in ('metal compilation failed', 'metal library link failed')):
        return 'compile_failed'
    if any(s in text for s in ('does not yet support', 'unsupported', 'requires --mandelbulber-root')):
        return 'unsupported_contract'
    if any(s in text for s in ('missing authored', 'no such file', 'missing lightmap')):
        return 'missing_asset'
    if 'blank or single-colour' in text or 'blank or single-color' in text:
        return 'image_validation_failed'
    return 'execution_failed'


def capture(command, folder, size, timeout, runner=execute):
    try:
        seconds = runner(command, folder, timeout)
        result = dict(status='ok', capture=dict(image_result(folder, size), wall_seconds=seconds))
        with Image.open(result['capture']['path']) as image:
            rgb = image.convert('RGB')
            luminance = rgb.convert('L')
            histogram = luminance.histogram()
            extrema = rgb.getextrema()
            result['screening'] = dict(mean_luminance_255=ImageStat.Stat(luminance).mean[0],
                dark_fraction=sum(histogram[:9])/(rgb.width*rgb.height),
                single_color=all(low == high for low,high in extrema),
                review_flags=['single_color'] if all(low == high for low,high in extrema) else
                    ['mostly_dark'] if sum(histogram[:9])/(rgb.width*rgb.height) > 0.95 else [],
                note='Advisory only; cannot establish geometry completeness or reference parity.')
        metadata = list(folder.glob('*.render.json'))
        if len(metadata) == 1:
            data = json.loads(metadata[0].read_text())
            result['metadata'] = dict(path=str(metadata[0]), sha256=sha256(metadata[0]))
            result['pipeline'] = {k: data.get(k) for k in (
                'mandel_kernel_mode', 'mandel_compile_mode', 'mandel_cache_status',
                'mandel_ambient', 'elapsed_ms', 'samples', 'sdf_bounce_cap')}
        return result
    except Exception as error:
        stderr = (folder/'stderr.log').read_text(errors='replace') if (folder/'stderr.log').exists() else ''
        result = dict(status=failure_kind(error, stderr), error=str(error), stderr_tail=stderr[-4000:])
        try:
            result['diagnostic_capture'] = image_result(folder, size)
        except (ValueError, OSError):
            pass
        return result


def validate_resume(summary, identity):
    if summary['identity'] != identity:
        raise ValueError('resume input hashes/settings/environment differ')
    for row in summary['rows']:
        for result in row['modes'].values():
            for key in ('capture', 'diagnostic_capture', 'metadata'):
                asset = result.get(key)
                if asset and sha256(Path(asset['path'])) != asset['sha256']:
                    raise ValueError('resume artifact hash mismatch: '+asset['path'])


def classify(row, requested_modes=MODES):
    modes = row['modes']
    fpt = [modes.get(k, {}) for k in ('geometry', 'authored')]
    # A successful production image establishes compilation. A timeout alone
    # does not establish whether compilation or GPU execution exhausted time.
    row['compilation'] = ('passed' if any(r.get('status') == 'ok' for r in fpt) else
        'failed' if any(r.get('status') == 'compile_failed' for r in fpt) else 'not_established')
    row['geometry_fidelity'] = 'requires_visual_review'
    row['appearance_fidelity'] = 'requires_visual_review'
    row['status'] = ('running' if any(m not in modes for m in requested_modes) else
                     'ok' if all(modes[m]['status'] == 'ok' for m in requested_modes) else 'incomplete')
    if all(modes.get(k, {}).get('status') == 'ok' for k in ('mandel', 'authored')):
        row['appearance_difference'] = difference(modes['mandel']['capture']['path'], modes['authored']['capture']['path'])


def save(summary, output):
    requested_modes = summary['identity']['settings'].get('modes', MODES)
    summary['counts'] = {mode: sum(r['modes'].get(mode, {}).get('status') == 'ok' for r in summary['rows']) for mode in MODES}
    summary['counts']['compilation_passed'] = sum(r.get('compilation') == 'passed' for r in summary['rows'])
    summary['counts']['completed_scenes'] = sum(all(m in r['modes'] for m in requested_modes) for r in summary['rows'])
    temp = output/'summary.tmp'
    temp.write_text(json.dumps(summary, indent=2))
    temp.replace(output/'summary.json')
    with (output/'support.csv').open('w', newline='') as stream:
        writer = csv.writer(stream)
        writer.writerow(['id','scene','dimensions','compilation','geometry_render','authored_render','reference_render','appearance_mae','geometry_fidelity','appearance_fidelity'])
        for row in summary['rows']:
            writer.writerow([row['id'],row['path'],str(row.get('size')),row.get('compilation'),
                *[row['modes'].get(m,{}).get('status','pending') for m in MODES],
                row.get('appearance_difference',{}).get('mae'),row.get('geometry_fidelity'),row.get('appearance_fidelity')])


def pages(summary, output, rows_per_page=10):
    maximum = summary['identity']['settings']['max_axis']
    columns = [('mandel','Mandelbulber CPU authored'),('geometry','FPT neutral geometry'),('authored','FPT authored path')]
    requested_modes = summary['identity']['settings'].get('modes', MODES)
    columns = [(mode, label) for mode, label in columns if mode in requested_modes]
    for start in range(0,len(summary['rows']),rows_per_page):
        rows = summary['rows'][start:start+rows_per_page]
        canvas = Image.new('RGB',(maximum*len(columns),40+len(rows)*(maximum+58)),'#202326')
        draw = ImageDraw.Draw(canvas)
        for col,(_,label) in enumerate(columns):
            draw.text((col*maximum+7,12),label,fill='white')
        for i,row in enumerate(rows):
            top = 40+i*(maximum+58)
            for col,(mode,_) in enumerate(columns):
                result = row['modes'].get(mode,{})
                asset = result.get('capture', result.get('diagnostic_capture'))
                if asset:
                    with Image.open(asset['path']) as source:
                        source=source.convert('RGB')
                        canvas.paste(source,(col*maximum+(maximum-source.width)//2,top+(maximum-source.height)//2))
                else:
                    draw.text((col*maximum+8,top+maximum//2),result.get('status','pending'),fill='#ffaaaa')
            draw.text((8,top+maximum+6),row['id']+' '+Path(row['path']).stem,fill='white')
            samples=summary['identity']['settings']['samples']
            draw.text((8,top+maximum+22),f"{row.get('size')} | {samples} SPP FPT | " + ' / '.join(row['modes'].get(m,{}).get('status','pending') for m,_ in columns),fill='white')
            if 'appearance_difference' in row:
                draw.text((8,top+maximum+38),f"Appearance MAE {row['appearance_difference']['mae']:.4f}; not a geometry/parity certificate",fill='#cccccc')
        canvas.save(output/f'page-{start//rows_per_page+1:02d}.png')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest',type=Path,default=ROOT/'tests/fixtures/mandel-release-ranked50.json')
    parser.add_argument('--scene-root',type=Path,required=True)
    parser.add_argument('--mandelbulber-root',type=Path,required=True)
    parser.add_argument('--mandelbulber-bin',type=Path)
    parser.add_argument('--modes',nargs='+',choices=MODES,default=list(MODES))
    parser.add_argument('--fpt',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--max-axis',type=int,default=300)
    parser.add_argument('--samples',type=int,default=32)
    parser.add_argument('--timeout',type=int,default=900)
    parser.add_argument('--resume',action='store_true')
    parser.add_argument('--scene-limit',type=int,help='Stop after this many total scenes; resume can extend the limit.')
    parser.add_argument('--pages-every',type=int,default=10)
    args=parser.parse_args()
    if not 16<=args.max_axis<=2048 or not 1<=args.samples<=512 or args.timeout<=0:
        parser.error('invalid dimensions/samples/timeout')
    if len(set(args.modes)) != len(args.modes) or args.pages_every < 1 or (args.scene_limit is not None and args.scene_limit < 1):
        parser.error('duplicate modes or invalid scene/page limit')
    if 'mandel' in args.modes and args.mandelbulber_bin is None:
        parser.error('--mandelbulber-bin is required for Mandel references')
    manifest=json.loads(args.manifest.read_text())
    validate_manifest(manifest,args.scene_root)
    output=args.output.resolve()
    output.mkdir(parents=True,exist_ok=True)
    default_map=args.mandelbulber_root/'deploy/share/mandelbulber2/textures/lightmap.jpg'
    assets={}
    for row in manifest['scenes']:
        try: assets[row['id']]=resolve_lightmap(args.scene_root/row['path'],args.mandelbulber_root,default_map)
        except ValueError as error: assets[row['id']]=dict(error=str(error))
    binaries = [args.fpt] + ([args.mandelbulber_bin] if 'mandel' in args.modes else [])
    identity=dict(manifest=manifest,harnesses={p.name:sha256(p) for p in (Path(__file__),ROOT/'scripts/run_release_canaries.py')},executables={str(p.resolve()):sha256(p) for p in binaries},
        settings=dict(max_axis=args.max_axis,samples=args.samples,aspect='authored',chunk_samples=1,timeout=args.timeout,
                      modes=args.modes,orientation='native; no postprocessing flips/crops',reference_backend='CPU' if 'mandel' in args.modes else 'not_requested',
                      bounces='FPT scene/config default; metadata records effective configuration'),
        environment={k:v for k,v in os.environ.items() if k.startswith('FPT_')},lightmaps=assets)
    if args.resume:
        summary=json.loads((output/'summary.json').read_text())
        validate_resume(summary,identity)
    else:
        if any(output.iterdir()): parser.error('output must be empty without --resume')
        summary=dict(identity=identity,rows=[],visual_parity_certified=False,
            limitations=['MAE compares different authored integrators, not isolated geometry.',
                         'Only AO maps are fingerprinted; other external textures remain untracked.',
                         'Timeouts do not prove a scene unsupported.',
                         'No renderer changes or performance claims in this support audit.'])
    for scene in manifest['scenes'][:args.scene_limit]:
        row=next((r for r in summary['rows'] if r['id']==scene['id']),None)
        if row is None:
            row=dict(scene,modes={})
            summary['rows'].append(row)
        path=(args.scene_root/scene['path']).resolve()
        row['size']=scene_dimensions(path.read_text(),args.max_axis)
        row['authored_effect_flags']={k:v for k,v in parameters(path.read_text()).items()
            if (any(s in k for s in ('fog','cloud','texture','dof','ambient_occlusion')) and v not in ('false','0'))}
        classify(row,args.modes)
        save(summary,output)
        for mode in args.modes:
            if mode in row['modes']: continue
            if min(shutil.disk_usage(output).free, shutil.disk_usage(tempfile.gettempdir()).free) < 1024**3:
                raise RuntimeError('less than 1 GiB free on output or temporary volume; resume after restoring disk headroom')
            if sha256(path)!=scene['sha256']: raise ValueError('scene mutated: '+str(path))
            for binary,digest in identity['executables'].items():
                if sha256(Path(binary))!=digest: raise ValueError('binary changed during suite')
            folder=output/scene['id']/mode
            attempt=1
            while folder.exists():
                folder=output/scene['id']/f'{mode}-attempt-{attempt}'
                attempt+=1
            print(scene['id'],mode,'starting',flush=True)
            asset=assets[scene['id']]
            if mode=='mandel' and 'error' in asset:
                row['modes'][mode]=dict(status='missing_asset',error=asset['error'])
            else:
                if 'error' not in asset: verify_lightmap(asset)
                if mode=='mandel':
                    command=mandel_reference_command(args.mandelbulber_bin,path,row['size'],folder/'scene.png',Path(asset['path']))
                else:
                    command=[str(args.fpt.resolve()),'render',str(path),'--mandelbulber-root',str(args.mandelbulber_root.resolve()),
                        '--width',str(row['size'][0]),'--height',str(row['size'][1]),'--samples',str(args.samples),
                        '--sdf-accumulation','chunked','--sdf-chunk-samples','1',
                        '--mandel-appearance','geometry' if mode=='geometry' else 'authored-path','--out',str(folder)]
                result=capture(command,folder,tuple(row['size']),args.timeout)
                if 'error' not in asset: verify_lightmap(asset)
                if mode=='mandel' and (folder/'stdout.log').exists() and 'OpenCl - rendering' in (folder/'stdout.log').read_text():
                    result=dict(status='invalid_reference_backend',error='OpenCL used despite CPU override')
                if sha256(path)!=scene['sha256']: raise ValueError('scene changed while rendering')
                row['modes'][mode]=result
            classify(row,args.modes)
            save(summary,output)
            print(scene['id'],mode,row['modes'][mode]['status'],row['modes'][mode].get('capture',{}).get('wall_seconds',''),flush=True)
        if len(summary['rows']) % args.pages_every == 0:
            pages(summary,output)
    save(summary,output)
    pages(summary,output)
    print(json.dumps(summary['counts']),flush=True)
    return int(any(r['status']!='ok' for r in summary['rows']))


if __name__=='__main__':
    raise SystemExit(main())
