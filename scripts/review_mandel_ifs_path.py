#!/usr/bin/env python3
"""Review the scene567 full-path precision pilot; never promotes gallery images."""
import argparse
import csv
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont
from run_release_canaries import sha256


def comparison(a, b):
    x=np.array(Image.open(a/'render.png').convert('RGB')).astype(float)
    y=np.array(Image.open(b/'render.png').convert('RGB')).astype(float)
    u=np.fromfile(a/'linear.f32',dtype='<f4').reshape(-1,4)[:,:3]
    v=np.fromfile(b/'linear.f32',dtype='<f4').reshape(-1,4)[:,:3]
    if x.shape!=y.shape or u.shape!=v.shape or not np.isfinite(u).all() or not np.isfinite(v).all():
        raise ValueError('invalid matched render records')
    delta=np.abs(x-y)
    return dict(changed_pixels=int(np.any(delta!=0,axis=2).sum()),rgb_mae=float(delta.mean()),
        rgb_max=float(delta.max()),linear_mae=float(np.abs(u-v).mean()),
        linear_max=float(np.abs(u-v).max()),raw_repeat_exact=(a/'linear.f32').read_bytes()==(b/'linear.f32').read_bytes())


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reports',type=Path,default=Path('reports'))
    parser.add_argument('--stamp',default='20260910')
    parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args()
    def capture(arithmetic,control,size,samples,bounces,suffix=''):
        return args.reports/f'mandel-567-path-{arithmetic}-{control}-{size}-s{samples}-b{bounces}{suffix}-{args.stamp}'/'render'
    rows=[]
    inputs={}
    for control,bounces in [('headlight',1),('authored',2),('authored',3),('authored',4)]:
        a=capture('three',control,'32x18',4,bounces)
        b=capture('two',control,'32x18',4,bounces)
        row=dict(control=control,bounces=bounces,**comparison(a,b))
        for label,p in [('three',a),('two',b)]:
            summary=json.loads((p/'summary.json').read_text())
            row[label+'_render_wall_ms']=summary['beauty_render_wall_ms']
            inputs[str(p.resolve())]={name:sha256(p/name) for name in ['render.png','linear.f32','summary.json']}
        rows.append(row)
    repeat=comparison(capture('two','authored','32x18',4,4),capture('two','authored','32x18',4,4,'-repeat'))
    one=capture('two','authored','160x90',16,1)
    four=capture('two','authored','160x90',16,4)
    u=np.fromfile(one/'linear.f32',dtype='<f4').reshape(-1,4)[:,:3]
    v=np.fromfile(four/'linear.f32',dtype='<f4').reshape(-1,4)[:,:3]
    secondary=dict(changed_pixels=int(np.any(u!=v,axis=1).sum()),rgb_linear_mae=float(np.abs(u-v).mean()),
        minimum_added_radiance=float((v-u).min()),maximum_added_radiance=float((v-u).max()))
    args.out.mkdir(parents=True,exist_ok=False)
    with (args.out/'comparison.csv').open('w') as stream:
        writer=csv.DictWriter(stream,fieldnames=list(rows[0]));writer.writeheader();writer.writerows(rows)
    report=dict(rows=rows,repeat=repeat,secondary_contribution=secondary,input_hashes=inputs,
        exact_display_gate_passed=all(row['changed_pixels']==0 for row in rows),
        production_changed=False,
        scope='Scene567 only. Safe math, precise positions/orbits for primary, normals, colors, shadows and bounces. Float32 scattering. No AO/fog/clouds/glow. Timing is render wall time, not GPU counters. Native/FPT shading and sampling are not equivalent.')
    (args.out/'summary.json').write_text(json.dumps(report,indent=2)+'\n')
    font=ImageFont.truetype('/System/Library/Fonts/Supplemental/Arial.ttf',14)
    small=ImageFont.truetype('/System/Library/Fonts/Supplemental/Arial.ttf',12)
    sheet=Image.new('RGB',(664,518),'#202326'); draw=ImageDraw.Draw(sheet)
    draw.text((12,10),'Scene 567 | all captures 160 x 90 | FPT 16 SPP',font=font,fill='white')
    draw.text((12,31),'No AO / fog / clouds / glow. Native and FPT shading/sampling differ.',font=small,fill='#d2d4d5')
    native=args.reports/f'mandel-567-path-native160-{args.stamp}'
    tiles=[(native/'headlight/scene.png','Mandelbulber: white headlight'),
        (capture('two','headlight','160x90',16,1)/'render.png','FPT precise: white headlight'),
        (native/'authored/scene.png','Mandelbulber: authored, effects excluded'),
        (four/'render.png','FPT two-term: 4 bounces, NOT parity-cleared')]
    for i,(path,label) in enumerate(tiles):
        im=Image.open(path).convert('RGB')
        if im.size!=(160,90): raise ValueError('comparison size mismatch')
        x=8+(i%2)*328;y=62+(i//2)*222
        draw.text((x,y),label,font=small,fill='white')
        sheet.paste(im.resize((320,180),Image.Resampling.NEAREST),(x,y+23))
    sheet.save(args.out/'comparison.png')
    a=capture('three','authored','32x18',4,4)/'render.png'
    b=capture('two','authored','32x18',4,4)/'render.png'
    x=np.asarray(Image.open(a).convert('RGB')).astype(float); y=np.asarray(Image.open(b).convert('RGB')).astype(float)
    images=[Image.open(a),Image.open(b),Image.fromarray(np.minimum(np.abs(x-y)*8,255).astype('uint8'))]
    gate=Image.new('RGB',(984,236),'#202326');d=ImageDraw.Draw(gate)
    for i,(im,label) in enumerate(zip(images,['Three-term reference | 32 x 18, 4 SPP','Two-term candidate | same settings','Absolute difference x8 | failed exact gate'])):
        d.text((8+i*328,10),label,font=small,fill='white')
        gate.paste(im.resize((320,180),Image.Resampling.NEAREST),(8+i*328,36))
    gate.save(args.out/'four-bounce-gate.png')
    print(json.dumps(report,indent=2))


if __name__=='__main__':
    main()
