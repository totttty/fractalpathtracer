#!/usr/bin/env python3
"""Compare scene567 diagnostic precision images and native EXR depths, without registration."""
import argparse
import csv
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw
from run_release_canaries import sha256


def metrics(native, depth, hits):
    if native.shape != depth.shape or depth.shape != hits.shape:
        raise ValueError('depth dimensions differ')
    if not np.isfinite(depth).all():
        raise ValueError('nonfinite candidate depth')
    hn = np.isfinite(native) & (native > 0) & (native < 1e10)
    both = hn & hits
    relative = abs(native.astype('float64') - depth) / np.maximum(abs(native), 1e-30)
    return dict(native_hits=int(hn.sum()), candidate_hits=int(hits.sum()),
        visible_misses=int((hn & ~hits).sum()), visible_extras=int((hits & ~hn).sum()),
        mutually_visible=int(both.sum()), coverage_pct=float(hits.mean()*100),
        depth_median_relative=float(np.median(relative[both])) if both.any() else None,
        depth_p95_relative=float(np.quantile(relative[both],.95)) if both.any() else None,
        depth_within_1pct=float((relative[both] <= .01).mean()*100) if both.any() else None)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference',type=Path,required=True)
    parser.add_argument('--native-channels',type=Path,required=True)
    parser.add_argument('--baseline',type=Path,required=True)
    parser.add_argument('--candidate',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args = parser.parse_args()
    with Image.open(args.reference) as image:
        size = image.size
        ref = image.convert('RGB')
    w,h = size
    native = np.fromfile(args.native_channels,dtype='<f4').reshape(h,w,4)[:,:,0]
    base = np.fromfile(args.baseline/'hits.bin',dtype='<f4').reshape(h,w,20)
    bm = json.loads((args.baseline/'summary.json').read_text())
    cm = json.loads((args.candidate/'summary.json').read_text())
    if (cm['render']['width'],cm['render']['height']) != size or (bm['width'],bm['height']) != size:
        raise ValueError('capture settings differ')
    candidate = np.fromfile(args.candidate/'render.bin',dtype='<f4').reshape(h,w,8)
    depth = np.fromfile(args.candidate/'depth.f64',dtype='<f8').reshape(h,w)
    rows = [dict(variant='Current float32 FPT',**metrics(native,base[:,:,3]/bm['world_scale'],base[:,:,7]>.5)),
            dict(variant='Experimental precision',**metrics(native,depth,candidate[:,:,2]>.5))]
    sheet = Image.new('RGB',(w*3,2*h+180),'#202326')
    draw = ImageDraw.Draw(sheet)
    for column,(title,path,mask) in enumerate([
        ('Mandelbulber white diffuse',args.reference,(native>0)&(native<1e10)),
        ('Current float32 FPT',args.baseline/'render.png',base[:,:,7]>.5),
        ('Experimental precision',args.candidate/'render.png',candidate[:,:,2]>.5),
    ]):
        draw.text((column*w+8,10),title,fill='white')
        with Image.open(path) as image:
            if image.size != size:
                raise ValueError('image dimensions differ')
            sheet.paste(image.convert('RGB'),(column*w,34))
        sheet.paste(Image.fromarray(mask.astype('uint8')*255).convert('RGB'),(column*w,h+62))
        draw.text((column*w+8,h+42),'Hit mask',fill='white')
    top=2*h+76
    for i,row in enumerate(rows):
        draw.text((8,top+i*20),f'{row["variant"]}: depth error median {row["depth_median_relative"]*100:.3f}%'
            f' | P95 {row["depth_p95_relative"]*100:.3f}% | misses {row["visible_misses"]} | extras {row["visible_extras"]}',fill='white')
    provider=cm.get('input_provider','native adapter')
    arithmetic=cm.get('arithmetic','three')
    draw.text((8,top+46),f'One ray/pixel. {arithmetic}-term arithmetic; {provider} inputs. Not production or NAADF.',fill='#ffc881')
    sampling=cm['render'].get('pixel_sampling','center')
    draw.text((8,top+63),f'Baseline sampling: center. Precision sampling: {sampling}. No image registration or camera adjustment.',fill='#ffc881')
    args.output.mkdir(parents=True,exist_ok=False)
    sheet.save(args.output/'comparison.png')
    paths=[args.reference,args.native_channels,args.baseline/'summary.json',args.baseline/'hits.bin',
           args.baseline/'render.png',args.candidate/'summary.json',args.candidate/'depth.f64',
           args.candidate/'render.bin',args.candidate/'render.png']
    report=dict(rows=rows,inputs={str(p.resolve()):sha256(p) for p in paths},
        size=size,candidate_sampling=sampling,baseline_sampling='center',input_provider=provider,arithmetic=arithmetic,
        limitations=['Relative depth measured only on mutually visible pixels; misses/extras reported separately.',
            'Native stochastic marching and refinement differ; no exact parity claim.',
            'Continuous diagnostic, not authored lighting, NAADF/CVOX, release support or timing gate.'],
        script_sha256=sha256(Path(__file__)),sheet_sha256=sha256(args.output/'comparison.png'))
    (args.output/'summary.json').write_text(json.dumps(report,indent=2)+'\n')
    with (args.output/'metrics.csv').open('w') as stream:
        writer=csv.DictWriter(stream,fieldnames=rows[0].keys())
        writer.writeheader()
        writer.writerows(rows)
    print(json.dumps(rows,indent=2))


if __name__=='__main__':
    main()
