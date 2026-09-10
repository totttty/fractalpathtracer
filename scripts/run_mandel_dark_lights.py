#!/usr/bin/env python3
"""Capture explicit dark-scene lighting A/Bs without substituting references."""
import argparse
import json
import os
from pathlib import Path

from PIL import Image, ImageDraw, ImageStat
from run_mandel_support_suite import capture
from run_release_canaries import execute, sha256, difference


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--scenes', default='07,08,40')
    p.add_argument('--baseline', type=Path)
    p.add_argument('--size', type=int, default=160)
    p.add_argument('--samples', type=int, default=8)
    p.add_argument('--audit', type=Path, required=True,
                   help='Completed ranked-50 summary JSON with cached native captures.')
    p.add_argument('--mandel-root', type=Path, required=True)
    p.add_argument('--scene-root', type=Path,
                   help='Defaults to deploy/share/mandelbulber2/examples under --mandel-root.')
    args = p.parse_args()
    audit = json.loads(args.audit.read_text())
    root = args.mandel_root.resolve()
    scene_root = args.scene_root or root/'deploy/share/mandelbulber2/examples'
    out = args.output.resolve(); out.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ, FPT_MANDEL_TILED_DISPATCH='1', FPT_MANDEL_TILE_ROWS='8')
    previous = {} if args.baseline is None else {r['id']: r for r in json.loads((args.baseline/'summary.json').read_text())['rows']}
    report = dict(binary=str(args.binary.resolve()), binary_sha256=sha256(args.binary), samples=args.samples, rows=[])
    for old in audit['rows']:
        if old['id'] not in args.scenes.split(','): continue
        size = [max(1, round(x*args.size/max(old['size']))) for x in old['size']]
        row = dict(id=old['id'], path=old['path'], size=size, modes={}, native=old['modes']['mandel']['capture'])
        for mode in ('geometry','authored'):
            folder = out/old['id']/mode
            cmd = [str(args.binary.resolve()),'render',str((scene_root/old['path']).resolve()),
                   '--mandelbulber-root',str(root),'--width',str(size[0]),'--height',str(size[1]),
                   '--samples',str(args.samples),'--sdf-accumulation','chunked','--sdf-chunk-samples','1',
                   '--mandel-appearance','authored-path' if mode=='authored' else 'geometry','--out',str(folder)]
            print(old['id'], mode, 'starting', flush=True)
            result = capture(cmd,folder,tuple(size),900,runner=lambda c,d,t:execute(c,d,t,env=env))
            row['modes'][mode] = result
            if result['status']=='ok':
                with Image.open(result['capture']['path']) as im:
                    rgb=im.convert('RGB'); gray=rgb.convert('L')
                    result['mean_rgb']=ImageStat.Stat(rgb).mean
                    result['dark_fraction']=sum(gray.histogram()[:8])/(im.width*im.height)
                if old['id'] in previous:
                    result['vs_before']=difference(previous[old['id']]['modes'][mode]['capture']['path'],result['capture']['path'])
            print(old['id'],mode,result['status'],flush=True)
        report['rows'].append(row)
        (out/'summary.json').write_text(json.dumps(report,indent=2)+'\n')
    sheet=Image.new('RGB',(900,50+len(report['rows'])*340),'#202225');draw=ImageDraw.Draw(sheet)
    for col,label in enumerate(['Mandelbulber reference (authored)','FPT before','FPT current']):draw.text((col*300+5,15),label,fill='white')
    for i,row in enumerate(report['rows']):
        before=previous.get(row['id'],row)
        assets=[row['native'],before['modes']['authored'].get('capture'),row['modes']['authored'].get('capture')]
        for col,asset in enumerate(assets):
            if not asset:continue
            with Image.open(asset['path']) as im:
                im=im.convert('RGB');im.thumbnail((300,300))
                sheet.paste(im,(col*300+(300-im.width)//2,50+i*340+(300-im.height)//2))
        draw.text((5,355+i*340),row['id']+' '+Path(row['path']).stem,fill='white')
    sheet.save(out/'comparison.png')
    if any(v['status']!='ok' for r in report['rows'] for v in r['modes'].values()):raise SystemExit('incomplete capture')


if __name__=='__main__':main()
