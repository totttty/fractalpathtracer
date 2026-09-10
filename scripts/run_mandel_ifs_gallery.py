#!/usr/bin/env python3
"""Matched-size scene567 precision review, including bounded camera translations."""
import argparse
import json
import os
from pathlib import Path
import sys

import numpy as np
from PIL import Image, ImageDraw, ImageFont
from run_mandel_geometry_controls import geometry_overrides
from run_mandel_ifs_path import shifted_camera
from run_mandel_ifs_precision import config_values, SCENE_SHA, FORMULA_SHA
from run_release_canaries import ROOT, execute, mandel_reference_command, sha256

VIEWS = [('center',(0.,0.)),('left',(-.25,0.)),('right',(.25,0.)),
         ('up',(0.,.25)),('down',(0.,-.25))]


def sheets(report, out):
    width,height=report['size']
    font=ImageFont.truetype('/System/Library/Fonts/Supplemental/Arial.ttf',14)
    small=ImageFont.truetype('/System/Library/Fonts/Supplemental/Arial.ttf',12)
    for name,controls in [('comparison',['headlight','authored']),('geometry',['headlight']),('authored',['authored'])]:
        columns=len(controls)*2
        canvas=Image.new('RGB',(columns*width,76+len(report['views'])*(height+51)),'#202326')
        draw=ImageDraw.Draw(canvas)
        draw.text((8,8),f"Scene 567 | {width} x {height} | FPT {report['samples']} SPP | authored: 4 bounces",font=font,fill='white')
        draw.text((8,28),'No AO / fog / clouds / glow. Same camera; native and FPT sampling/shading differ.',font=small,fill='#d1d4d5')
        for j,control in enumerate(controls):
            draw.text((j*2*width+8,54),'Mandelbulber: '+control,font=small,fill='white')
            draw.text(((j*2+1)*width+8,54),'FPT two-term: '+control,font=small,fill='white')
        for i,view in enumerate(report['views']):
            y=76+i*(height+51)
            for j,control in enumerate(controls):
                for k,renderer in enumerate(('native','fpt')):
                    result=view.get(control,{})
                    path=result.get(renderer+'_image')
                    if path:
                        image=Image.open(path).convert('RGB')
                        if image.size!=(width,height): raise ValueError('image dimensions differ')
                        canvas.paste(image,((j*2+k)*width,y))
            draw.text((8,y+height+6),view['name']+' | eye and target shift: '+str(view['shift'])+' focus-distance units',font=small,fill='white')
        canvas.save(out/(name+'.png'))


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--scene',type=Path,required=True)
    p.add_argument('--mandelbulber-root',type=Path,required=True)
    p.add_argument('--mandelbulber-bin',type=Path,required=True)
    p.add_argument('--fpt-inputs',type=Path,required=True)
    p.add_argument('--out',type=Path,required=True)
    p.add_argument('--views',nargs='+',choices=[x[0] for x in VIEWS],default=[x[0] for x in VIEWS])
    p.add_argument('--size',default='300x169')
    p.add_argument('--samples',type=int,default=32)
    args=p.parse_args()
    size=tuple(map(int,args.size.split('x')))
    formula=args.mandelbulber_root/'formula/definition/fractal_kaleidoscopic_ifs.cpp'
    inputs=json.loads((args.fpt_inputs/'summary.json').read_text())
    if sha256(args.scene)!=SCENE_SHA or sha256(formula)!=FORMULA_SHA or inputs['scene_sha256']!=SCENE_SHA:
        raise ValueError('unsupported scene/formula')
    if sha256(args.fpt_inputs/'config.tsv')!=inputs['config_sha256']:
        raise ValueError('input config mismatch')
    if len(size)!=2 or not(0<size[0]<=320 and 0<size[1]<=240 and 0<args.samples<=32):
        raise ValueError('invalid bounded gallery size/samples')
    source=args.scene.read_text(); cfg=config_values(args.fpt_inputs/'config.tsv')
    out=args.out.resolve();out.mkdir(parents=True,exist_ok=False)
    report=dict(scene_sha256=SCENE_SHA,formula_sha256=FORMULA_SHA,size=size,samples=args.samples,
        precision='two',bounces=4,production_unchanged=True,visual_review='pending',views=[],
        acceptance='Visual structure, no invalid rays or missing surfaces; byte equality between arithmetic variants is not required.',
        limitations=['Continuous FPT, not NAADF/CVOX','Only the source-pinned IFS scene','Translation tests, not continuous-motion certification',
            'AO/fog/clouds/glow excluded','Native CPU sampling and integrator differ','Times are wall time, not GPU intervals'],
        executable_hashes={str(p.resolve()):sha256(p) for p in [args.mandelbulber_bin,ROOT/'target/release/examples/mandel_derivative_render']})
    env={**os.environ,'QT_QPA_PLATFORM':'offscreen'}
    lightmap=args.mandelbulber_root/'deploy/share/mandelbulber2/textures/lightmap.jpg'
    for name,shift in VIEWS:
        if name not in args.views: continue
        _,camera=shifted_camera(cfg,shift)
        row=dict(name=name,shift=shift,camera=camera);report['views'].append(row)
        for control in ('headlight','authored'):
            folder=out/name/control
            native=folder/'native'
            command=mandel_reference_command(args.mandelbulber_bin,args.scene,size,native/'scene.png',lightmap)
            overrides=geometry_overrides(source) if control=='headlight' else {
                'basic_fog_enabled':'0','volumetric_fog_enabled':'0','iteration_fog_enable':'0','clouds_enable':'0',
                'glow_enabled':'0','ambient_occlusion_enabled':'0','DOF_enabled':'0','DOF_monte_carlo':'0'}
            overrides.update({key:' '.join(format(x,'.17g') for x in camera[key]) for key in ('camera','target')})
            command[command.index('-O')+1]+='#'+'#'.join(f'{k}={v}' for k,v in overrides.items())
            native_wall=execute(command,native,600,env=env)
            if "doesn't exists" in (native/'stderr.log').read_text(): raise RuntimeError('unrecognized native override')
            fpt=folder/'fpt'
            command=[sys.executable,str(ROOT/'scripts/run_mandel_ifs_path.py'),'--scene',str(args.scene),
                '--mandelbulber-root',str(args.mandelbulber_root),'--fpt-inputs',str(args.fpt_inputs),
                '--out',str(fpt),'--precision','two','--control',control,'--size',args.size,
                '--samples',str(args.samples),'--bounces',str(1 if control=='headlight' else 4),
                '--camera-shift='+','.join(map(str,shift))]
            execute(command,folder/'driver',900,env=env)
            summary=json.loads((fpt/'render/summary.json').read_text())
            run=json.loads((fpt/'run.json').read_text())
            if run['camera']!=camera: raise ValueError('native/FPT camera metadata mismatch')
            raw=np.fromfile(fpt/'render/linear.f32',dtype='<f4')
            if len(raw)!=size[0]*size[1]*4 or not np.isfinite(raw).all(): raise ValueError('invalid radiance')
            row[control]=dict(native_image=str(native/'scene.png'),fpt_image=str(fpt/'render/render.png'),
                native_image_sha256=sha256(native/'scene.png'),fpt_image_sha256=sha256(fpt/'render/render.png'),
                fpt_linear_sha256=sha256(fpt/'render/linear.f32'),native_wall_s=native_wall,
                fpt_render_wall_s=summary['beauty_render_wall_ms']/1000,
                finite_linear=True,native_overrides=overrides)
            (out/'summary.json').write_text(json.dumps(report,indent=2)+'\n')
            sheets(report,out)
            print(name,control,'complete',flush=True)
    report['renders_complete']=True
    (out/'summary.json').write_text(json.dumps(report,indent=2)+'\n')


if __name__=='__main__':
    main()
