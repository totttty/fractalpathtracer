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
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from PIL import Image, ImageDraw, ImageStat
from run_release_canaries import (ROOT, difference, execute, image_result,
    lightmap_asset, mandel_reference_command, scene_dimensions, sha256,
    validate_manifest, verify_lightmap)
from mandel_reference_cache import reference_contract, load_reference, store_reference

MODES = ('geometry', 'authored', 'mandel')


def run_modes(modes, render, record, overlap=False):
    """Only the main thread records results; at most one CPU and one GPU job."""
    if not overlap or 'mandel' not in modes:
        for mode in modes:
            record(mode, render(mode))
        return
    with ThreadPoolExecutor(max_workers=1) as pool:
        native = pool.submit(render, 'mandel')
        for mode in modes:
            if mode != 'mandel':
                record(mode, render(mode))
        record('mandel', native.result())


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


def native_mc_settings(text, cap=None):
    """Cap MC work without changing the authored integrator or enabling MC."""
    values=parameters(text)
    enabled=values.get('DOF_monte_carlo','false').lower() in ('true','1')
    if cap is not None and cap < 1:
        raise ValueError('native MC sample cap must be positive')
    maximum=int(values.get('DOF_samples','100'))
    minimum=int(values.get('DOF_min_samples','10'))
    if enabled and (maximum < 1 or minimum < 0):
        raise ValueError('invalid authored native MC sampling bounds')
    overrides={}
    effective=maximum
    if enabled and cap is not None and cap < maximum:
        effective=cap
        overrides['DOF_samples']=effective
        if minimum > effective:
            overrides['DOF_min_samples']=effective
    return dict(enabled=enabled,requested_cap=cap,authored_maximum=maximum,
                authored_minimum=minimum,effective_maximum=effective,
                effective_minimum=min(minimum,effective) if overrides else minimum,
                overrides=overrides,
                note='MC maximum, not guaranteed SPP; authored AA/adaptive sampling still applies.')


def apply_native_sampling(command, sampling):
    command=list(command)
    for key,value in sampling['overrides'].items():
        command[command.index('-O')+1]+=f'#{key}={value}'
    return command


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
        result = dict(status=failure_kind(error, stderr), error=str(error), stderr_tail=stderr[-4000:],
                      timeout_seconds=timeout)
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
    summary['counts']['native_cache_hits'] = sum(r['modes'].get('mandel',{}).get('reference_cache',{}).get('status')=='hit' for r in summary['rows'])
    summary['counts']['native_deferred'] = sum(r['modes'].get('mandel',{}).get('deferred',False) for r in summary['rows'])
    temp = output/'summary.tmp'
    temp.write_text(json.dumps(summary, indent=2))
    temp.replace(output/'summary.json')
    (output/'deferred-native.json').write_text(json.dumps(dict(
        scope='Incomplete references, not unsupported scenes; retry in the final outlier batch.',
        rows=[dict(id=r['id'],path=r['path'],sha256=r['sha256'],
                   reason=r['modes']['mandel']['status'],
                   timeout_seconds=r['modes']['mandel'].get('timeout_seconds'))
              for r in summary['rows'] if r['modes'].get('mandel',{}).get('deferred')]),indent=2))
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
    cap=summary['identity']['settings'].get('native_mc_samples')
    if cap is not None:
        columns[0]=('mandel',f'Mandel CPU screening (MC cap {cap})')
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
    parser.add_argument('--native-timeout',type=int,default=120,
                        help='Native CPU screening budget; timeout means deferred, not unsupported.')
    parser.add_argument('--native-mc-samples',type=int,
                        help='Screening-only cap for already-enabled native MC; does not alter effects or FPT SPP.')
    parser.add_argument('--reference-cache',type=Path,
                        help='Reuse native references only with a complete matching content contract.')
    parser.add_argument('--overlap-native',action='store_true',
                        help='Experimental: one native CPU capture alongside one FPT GPU capture.')
    parser.add_argument('--resume',action='store_true')
    parser.add_argument('--scene-limit',type=int,help='Stop after this many total scenes; resume can extend the limit.')
    parser.add_argument('--pages-every',type=int,default=10)
    args=parser.parse_args()
    if not 16<=args.max_axis<=2048 or not 1<=args.samples<=512 or min(args.timeout,args.native_timeout)<=0:
        parser.error('invalid dimensions/samples/timeout')
    if args.native_mc_samples is not None and not 1<=args.native_mc_samples<=10000:
        parser.error('native MC cap must be in 1..10000')
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
    identity=dict(manifest=manifest,harnesses={p.name:sha256(p) for p in (Path(__file__),ROOT/'scripts/run_release_canaries.py',ROOT/'scripts/mandel_reference_cache.py')},executables={str(p.resolve()):sha256(p) for p in binaries},
        settings=dict(max_axis=args.max_axis,samples=args.samples,aspect='authored',chunk_samples=1,timeout=args.timeout,
                      native_timeout=args.native_timeout,overlap_native=args.overlap_native,
                      native_mc_samples=args.native_mc_samples,
                      reference_cache=str(args.reference_cache.resolve()) if args.reference_cache else None,
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
                         'General FPT audit fingerprints AO maps only; eligible native cache entries additionally bind native resources/preferences.',
                         'Timeouts do not prove a scene unsupported.',
                         'No renderer changes or performance claims in this support audit.'])
    suite_start=time.monotonic()
    prior_wall=summary.get('suite_wall_seconds',0)
    for scene in manifest['scenes'][:args.scene_limit]:
        row=next((r for r in summary['rows'] if r['id']==scene['id']),None)
        if row is None:
            row=dict(scene,modes={})
            summary['rows'].append(row)
        path=(args.scene_root/scene['path']).resolve()
        row['size']=scene_dimensions(path.read_text(),args.max_axis)
        row['native_sampling']=native_mc_settings(path.read_text(),args.native_mc_samples)
        row['reference_overrides']=row['native_sampling']['overrides']
        row['authored_effect_flags']={k:v for k,v in parameters(path.read_text()).items()
            if (any(s in k for s in ('fog','cloud','texture','dof','ambient_occlusion')) and v not in ('false','0'))}
        classify(row,args.modes)
        save(summary,output)
        def render(mode):
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
                return dict(status='missing_asset',error=asset['error'])
            else:
                if 'error' not in asset: verify_lightmap(asset)
                if mode=='mandel':
                    command=mandel_reference_command(args.mandelbulber_bin,path,row['size'],folder/'scene.png',Path(asset['path']))
                    command=apply_native_sampling(command,row['native_sampling'])
                else:
                    command=[str(args.fpt.resolve()),'render',str(path),'--mandelbulber-root',str(args.mandelbulber_root.resolve()),
                        '--width',str(row['size'][0]),'--height',str(row['size'][1]),'--samples',str(args.samples),
                        '--sdf-accumulation','chunked','--sdf-chunk-samples','1',
                        '--mandel-appearance','geometry' if mode=='geometry' else 'authored-path','--out',str(folder)]
                contract=None
                cache_reason=None
                if mode=='mandel' and args.reference_cache:
                    contract,cache_reason=reference_contract(command,path,row['size'],asset,args.mandelbulber_root)
                    if contract:
                        cached=load_reference(args.reference_cache,contract,folder)
                        if cached:
                            return cached
                result=capture(command,folder,tuple(row['size']),
                               args.native_timeout if mode=='mandel' else args.timeout)
                if 'error' not in asset: verify_lightmap(asset)
                if mode=='mandel' and (folder/'stdout.log').exists() and 'OpenCl - rendering' in (folder/'stdout.log').read_text():
                    result=dict(status='invalid_reference_backend',error='OpenCL used despite CPU override')
                if sha256(path)!=scene['sha256']: raise ValueError('scene changed while rendering')
                if mode=='mandel':
                    result['deferred']=result['status']=='timeout'
                    if contract:
                        after,_=reference_contract(command,path,row['size'],asset,args.mandelbulber_root)
                        if after!=contract: raise ValueError('native reference inputs changed during capture')
                        try:
                            store_reference(args.reference_cache,contract,folder,result)
                            result['reference_cache']=dict(status='miss')
                        except ValueError as error:
                            result['reference_cache']=dict(status='ineligible',reason=str(error))
                    elif cache_reason:
                        result['reference_cache']=dict(status='ineligible',reason=cache_reason)
                return result

        def record(mode,result):
            row['modes'][mode]=result
            classify(row,args.modes)
            summary['suite_wall_seconds']=prior_wall+time.monotonic()-suite_start
            save(summary,output)
            print(scene['id'],mode,row['modes'][mode]['status'],row['modes'][mode].get('capture',{}).get('wall_seconds',''),flush=True)
        run_modes([m for m in args.modes if m not in row['modes']],render,record,args.overlap_native)
        if len(summary['rows']) % args.pages_every == 0:
            pages(summary,output)
    summary['suite_wall_seconds']=prior_wall+time.monotonic()-suite_start
    save(summary,output)
    pages(summary,output)
    print(json.dumps(summary['counts']),flush=True)
    return int(any(r['status']!='ok' for r in summary['rows']))


if __name__=='__main__':
    raise SystemExit(main())
