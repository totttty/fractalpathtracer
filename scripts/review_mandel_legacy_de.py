#!/usr/bin/env python3
"""Audit every bundled 2.x scene affected by stale analityc_DE_mode inference."""
import argparse
import json
from pathlib import Path

from PIL import Image, ImageDraw
from run_mandel_geometry_controls import headlight_command
from run_mandel_shadow_controls import replace_main_parameters
from run_mandel_support_suite import parameters
from run_release_canaries import execute, image_result, mandel_reference_command, scene_dimensions, difference, sha256


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('scene-root', 'source-root', 'native', 'baseline', 'candidate', 'output'):
        parser.add_argument('--'+name, type=Path, required=True)
    parser.add_argument('--resume-geometry', action='store_true',
                        help='Hash-check completed captures and finish geometry only after an authored failure.')
    parser.add_argument('--bake-native-lights', action='store_true',
                        help='Load native geometry-control lights from an explicit derived scene file.')
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=args.resume_geometry)
    selected = []
    for path in sorted(args.scene_root.rglob('*.fract')):
        values = parameters(path.read_text())
        if 'analityc_DE_mode' in values and 'delta_DE_method' not in values:
            selected.append(path)
    if not selected:
        raise ValueError('no affected scenes')
    report = dict(harness_sha256=sha256(Path(__file__)), max_axis=300, fpt_samples=32,
                  native_light_control='file' if args.bake_native_lights else 'cli',
                  source_count=len(selected), binaries={str(p.resolve()):sha256(p) for p in (args.baseline,args.candidate,args.native)},
                  limitations=['Continuous FPT only. No NAADF gallery regeneration or timing gate.',
                               'Native CPU sampling and authored effects differ from FPT.',
                               'Stereo disabled explicitly; no registration or exposure adjustments.'], rows=[])
    if args.resume_geometry:
        previous=json.loads((out/'summary.json').read_text())
        if previous['binaries']!=report['binaries'] or previous['source_count']!=len(selected):
            raise ValueError('resume identity changed')
        if previous.get('native_light_control', 'cli') != report['native_light_control']:
            raise ValueError('resume native light control changed')
        for row in previous['rows']:
            if sha256(Path(row['source']))!=row['source_sha256']:
                raise ValueError('resume source changed')
            for capture in row['captures'].values():
                if sha256(Path(capture['path']))!=capture['sha256']:
                    raise ValueError('resume capture changed')
            if 'native_control' in row and sha256(Path(row['native_control']['path'])) != row['native_control']['sha256']:
                raise ValueError('resume native control changed')
        report=previous
        report['resume_harness_sha256']=sha256(Path(__file__))
        report['resume_scope']='Finish geometry only; incomplete authored runs are not retried or counted as passed.'
    for index, path in enumerate(selected, 1):
        control = out / f'{index:02d}' / 'scene.fract'
        control.parent.mkdir(exist_ok=args.resume_geometry)
        text=replace_main_parameters(path.read_text(), {'stereo_enabled':'false'})
        if control.exists():
            if control.read_text()!=text: raise ValueError('control changed')
        else:
            control.write_text(text)
        size = scene_dimensions(path.read_text(), 300)
        for mode in (('geometry',) if args.resume_geometry else ('geometry','authored')):
            if any(r['index']==index and r['mode']==mode for r in report['rows']):
                continue
            row = dict(index=index, source=str(path), source_sha256=sha256(path), control_sha256=sha256(control),
                       mode=mode, size=size, captures={})
            for renderer,binary in [('native',args.native),('baseline',args.baseline),('candidate',args.candidate)]:
                folder = control.parent / mode / renderer
                if renderer=='native':
                    command=mandel_reference_command(binary.resolve(),control,size,folder/'scene.png',
                        args.source_root/'deploy/share/mandelbulber2/textures/lightmap.jpg')
                    if mode=='geometry':
                        command,_=headlight_command(command,control.read_text(),folder,
                            bake_lights=args.bake_native_lights)
                        row['native_control']=dict(path=command[-1],sha256=sha256(Path(command[-1])))
                else:
                    command=[str(binary.resolve()),'render',str(control),'--mandelbulber-root',str(args.source_root.resolve()),
                             '--width',str(size[0]),'--height',str(size[1]),'--samples','32','--sdf-accumulation','chunked',
                             '--sdf-chunk-samples','1','--mandel-appearance','geometry' if mode=='geometry' else 'authored-path',
                             '--out',str(folder)]
                wall=execute(command,folder,300)
                row['captures'][renderer]=dict(image_result(folder,size),wall_seconds=wall)
            for renderer in ('baseline','candidate'):
                row[renderer+'_difference']=difference(row['captures']['native']['path'],row['captures'][renderer]['path'])
            row['output_byte_exact']=row['captures']['baseline']['rgb_sha256']==row['captures']['candidate']['rgb_sha256']
            report['rows'].append(row)
            (out/'summary.json').write_text(json.dumps(report,indent=2)+'\n')
            print(index,path.stem,mode,row['baseline_difference']['mae'],row['candidate_difference']['mae'],row['output_byte_exact'],flush=True)
    pages=[]
    for row in report['rows']:
        with Image.open(row['captures']['native']['path']) as im:
            single=all(lo==hi for lo,hi in im.convert('RGB').getextrema())
        row['native_single_color']=single
        row['reference_gate']='unusable_single_color' if single else 'requires_visual_review'
    for mode in ('geometry','authored'):
        rows=[r for r in report['rows'] if r['mode']==mode]
        for start in range(0,len(rows),5):
            subset=rows[start:start+5]
            canvas=Image.new('RGB',(900,40+len(subset)*338),'#202326');draw=ImageDraw.Draw(canvas)
            for col,label in enumerate(('Mandelbulber CPU','FPT accepted baseline','FPT preferred-DE correction')):
                draw.text((col*300+8,12),label,fill='white')
            for i,row in enumerate(subset):
                top=40+i*338
                for col,name in enumerate(('native','baseline','candidate')):
                    with Image.open(row['captures'][name]['path']) as image:
                        canvas.paste(image.convert('RGB'),(col*300+(300-image.width)//2,top+(300-image.height)//2))
                draw.text((8,top+304),f"{row['index']:02d} {Path(row['source']).stem} | {mode}",fill='white')
                note='REFERENCE IS SINGLE COLOR: no parity claim' if row['native_single_color'] else f"RGB MAE {row['baseline_difference']['mae']:.5f} -> {row['candidate_difference']['mae']:.5f}; appearance diagnostic only"
                draw.text((8,top+320),note,fill='#cccccc')
            name=f'{mode}-{start//5+1:02d}.png';canvas.save(out/name);pages.append(name)
    report['pages']={name:sha256(out/name) for name in pages}
    (out/'summary.json').write_text(json.dumps(report,indent=2)+'\n')


if __name__=='__main__':
    main()
