#!/usr/bin/env python3
"""Fresh production FPT ranked-50 review with hash-verified cached native references."""
import argparse
import json
import os
from pathlib import Path
import shutil
import textwrap

from PIL import Image, ImageDraw
from run_mandel_support_suite import capture
from run_release_canaries import difference, sha256, validate_manifest


SCENE_NOTES = {
    '07': 'Surface lighting restored; cloud volume remains unsupported.',
    '08': 'Generated point lights restored; native light placement is not bit-exact.',
    '32': 'Known illumination gap: authored output remains much darker than native.',
    '37': 'Known illumination gap: authored output remains darker than native.',
    '40': 'Orbit-trap surface lighting restored; volumetric halos remain unsupported.',
    '42': 'Known normal/lighting residual; rejected analytic derivative is NOT included.',
    '46': 'Known production float32 position-stall issue; inspect geometry carefully.',
    '48': 'Known production deep-zoom precision failure; offline precision prototype is NOT included.',
    '50': 'Known authored-lighting/indirect-transport mismatch.',
}


def sheets(report, out):
    columns = [('mandel', 'Mandelbulber CPU reference (cached)'),
               ('geometry', 'Current FPT - neutral geometry'),
               ('authored', 'Current FPT - authored path tracing')]
    for start in range(0, len(report['rows']), 10):
        rows = report['rows'][start:start+10]
        canvas = Image.new('RGB', (900, 70+len(rows)*352), '#1d2023')
        draw = ImageDraw.Draw(canvas)
        draw.text((8, 8), 'MERGE REVIEW | current production renderer | 32 SPP | no experimental derivative', fill='white')
        for col, (_, title) in enumerate(columns):
            draw.text((col*300+8, 36), title, fill='white')
        for i, row in enumerate(rows):
            top = 70+i*352
            for col, (mode, _) in enumerate(columns):
                result = row['modes'].get(mode, {})
                asset = result.get('capture', result.get('diagnostic_capture'))
                if asset:
                    with Image.open(asset['path']) as im:
                        im = im.convert('RGB')
                        if mode == 'mandel' and row.get('reference_reduced'):
                            im = im.resize(tuple(row['size']), Image.Resampling.NEAREST)
                        canvas.paste(im, (col*300+(300-im.width)//2, top+(300-im.height)//2))
                else:
                    draw.text((col*300+8, top+140), result.get('status', 'pending'), fill='#ffaaaa')
            name = row['id']+' '+Path(row['path']).stem
            draw.text((8, top+303), textwrap.shorten(name, width=130, placeholder='...'), fill='white')
            ref = '96px native enlarged; comparison is visual only' if row.get('reference_reduced') else 'native and FPT dimensions match'
            draw.text((8, top+319), f"FPT {row['size'][0]}x{row['size'][1]} | {ref}", fill='#c8ced4')
            if row.get('note'):
                draw.text((8, top+335), row['note'], fill='#ffce86')
        canvas.save(out/f'scenes-{start+1:02d}-{start+len(rows):02d}.png')
    # A compact navigation sheet; the detailed pages remain the visual gate.
    canvas = Image.new('RGB', (1500, 36+10*160), '#1d2023')
    draw = ImageDraw.Draw(canvas)
    draw.text((8, 10), 'Scenes 01-50 | Each pair: native reference / current FPT authored | detail in the five full sheets', fill='white')
    for i, row in enumerate(report['rows']):
        x,y = (i%5)*300,36+(i//5)*160
        for col,mode in enumerate(['mandel','authored']):
            result = row['modes'].get(mode,{})
            asset = result.get('capture',result.get('diagnostic_capture'))
            if asset:
                with Image.open(asset['path']) as im:
                    im = im.convert('RGB'); im.thumbnail((146,132))
                    canvas.paste(im,(x+col*150+(150-im.width)//2,y+(132-im.height)//2))
        draw.text((x+5,y+134),row['id']+' '+textwrap.shorten(Path(row['path']).stem,width=36,placeholder='...'),fill='white')
    canvas.save(out/'overview.png')


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--audit',type=Path,required=True)
    p.add_argument('--retries',type=Path,required=True)
    p.add_argument('--scene-root',type=Path,required=True)
    p.add_argument('--mandel-root',type=Path,required=True)
    p.add_argument('--binary',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True)
    p.add_argument('--tile-rows',type=int,default=8,
                   help='Rows per GPU dispatch; 8 avoids recovery errors in the ranked-50 suite.')
    args = p.parse_args()
    if args.tile_rows < 1:
        p.error('--tile-rows must be positive')
    old = json.loads((args.audit/'summary.json').read_text())
    retries = {r['id']:r for r in json.loads((args.retries/'summary.json').read_text())['rows']}
    validate_manifest(old['identity']['manifest'],args.scene_root)
    out=args.output.resolve();out.mkdir(parents=True,exist_ok=False)
    env=dict(os.environ,FPT_MANDEL_TILED_DISPATCH='1',FPT_MANDEL_TILE_ROWS=str(args.tile_rows))
    for key in list(env):
        if key.startswith('FPT_NORMAL_PROBE_') or key.startswith('FPT_DERIVATIVE_'):
            del env[key]
    report={'scope':'Fresh production continuous FPT, not NAADF voxel captures. Cached native references validated by source/image hashes. No experimental derivative or high-precision substitutions.',
            'production_sha256':sha256(args.binary),'audit_sha256':sha256(args.audit/'summary.json'),
            'samples':32,'max_axis':300,'bounces':'unchanged scene/config default',
            'environment':{k:v for k,v in env.items() if k.startswith('FPT_')},
            'merge_ready':False,'visual_parity_certified':False,'rows':[]}
    def save():
        report['counts']={m:sum(r['modes'].get(m,{}).get('status')=='ok' for r in report['rows']) for m in ('mandel','geometry','authored')}
        (out/'summary.json').write_text(json.dumps(report,indent=2)+'\n')
    for previous in old['rows']:
        row={k:previous[k] for k in ('id','path','sha256','size')}
        row['modes']={};row['note']=SCENE_NOTES.get(row['id'],'')
        reference=previous['modes']['mandel']
        row['reference_reduced']=reference['status']!='ok'
        if row['reference_reduced']:
            retry=retries[row['id']]
            if retry['source_sha256']!=row['sha256']:raise ValueError('retry source mismatch')
            reference=retry['native']
        if reference['status']!='ok':raise ValueError('native reference unavailable')
        asset=reference['capture']
        if sha256(Path(asset['path']))!=asset['sha256']:raise ValueError('native image hash changed')
        ref=out/row['id']/'native.png';ref.parent.mkdir(parents=True)
        shutil.copy2(asset['path'],ref)
        row['modes']['mandel']={'status':'ok','capture':dict(asset,path=str(ref)),
                                'provenance':'cached native reference; not rerendered','original_path':asset['path']}
        report['rows'].append(row);save()
        scene=(args.scene_root/row['path']).resolve()
        for mode in ('geometry','authored'):
            if shutil.disk_usage(out).free < 300*1024**2:raise RuntimeError('less than 300 MiB disk headroom')
            if sha256(args.binary)!=report['production_sha256'] or sha256(scene)!=row['sha256']:
                raise ValueError('renderer or source changed during capture')
            folder=out/row['id']/mode
            command=[str(args.binary.resolve()),'render',str(scene),'--mandelbulber-root',str(args.mandel_root.resolve()),
                     '--width',str(row['size'][0]),'--height',str(row['size'][1]),'--samples','32',
                     '--sdf-accumulation','chunked','--sdf-chunk-samples','1','--mandel-appearance',
                     'geometry' if mode=='geometry' else 'authored-path','--out',str(folder)]
            print(row['id'],mode,'starting',flush=True)
            runner=lambda c,d,t:__import__('run_release_canaries').execute(c,d,t,env=env)
            result=capture(command,folder,tuple(row['size']),900,runner=runner)
            if result['status']!='ok':
                original=result
                folder=folder.with_name(mode+'-retry');command[-1]=str(folder)
                result=capture(command,folder,tuple(row['size']),900,runner=runner)
                result['first_attempt']=original
            row['modes'][mode]=result
            if mode=='authored' and result['status']=='ok' and not row['reference_reduced']:
                row['appearance_difference']=difference(ref,result['capture']['path'])
            save();print(row['id'],mode,result['status'],flush=True)
        if len(report['rows'])%10==0:sheets(report,out)
    sheets(report,out);save()
    print(json.dumps(report['counts']),flush=True)
    if any(report['counts'][m]!=50 for m in ('mandel','geometry','authored')):
        raise SystemExit('incomplete capture suite; do not claim merge readiness')


if __name__=='__main__':main()
