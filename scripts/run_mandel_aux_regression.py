#!/usr/bin/env python3
"""Ordinary/neutral exact gates plus native authored references for auxiliary lights."""
import argparse
import json
import os
from pathlib import Path
from PIL import Image, ImageDraw
from run_release_canaries import execute, difference, scene_dimensions, sha256
from run_mandel_support_suite import capture


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline',type=Path,required=True)
    parser.add_argument('--candidate',type=Path,required=True)
    parser.add_argument('--audit',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();root=Path(__file__).resolve().parents[1]
    out=args.output.resolve();out.mkdir(parents=True,exist_ok=False)
    report={'binaries':{str(p.resolve()):sha256(p) for p in [args.baseline,args.candidate]},'rows':[]}
    jobs=[]
    for scene in ['03-Cornell_box','04-Glass_Ball','07-M4','01-Render005']:
        jobs.append(('ordinary',scene,['unused','render',str(root/f'scenes/readme/{scene}.json'),'--width','160','--height','90','--samples','8','--out','unused','--sdf-accumulation','chunked','--sdf-chunk-samples','1'],(160,90),True))
    for mode,ids in [('geometry',['11','21','42','50']),('authored',['14','30','21','42','50'])]:
        for i in ids:
            cmd=json.loads((args.audit/i/mode/'command.json').read_text());size=scene_dimensions(Path(cmd[2]).read_text(),160)
            for flag,value in [('--width',size[0]),('--height',size[1]),('--samples',8 if mode=='geometry' else 32)]:cmd[cmd.index(flag)+1]=str(value)
            jobs.append((mode,i,cmd,size,mode=='geometry' or i in ['14','30']))
    for mode,i,cmd,size,exact in jobs:
        row={'mode':mode,'id':i,'size':size,'require_exact':exact,'source_sha256':sha256(Path(cmd[2])),'modes':{}};report['rows'].append(row)
        for label,binary in [('baseline',args.baseline),('candidate',args.candidate)]:
            command=cmd.copy();command[0]=str(binary.resolve());command[command.index('--out')+1]=str(out/mode/i/label)
            row['modes'][label]=capture(command,out/mode/i/label,size,240,runner=lambda c,d,t:execute(c,d,t,env=dict(os.environ,FPT_MANDEL_TILED_DISPATCH='1',FPT_MANDEL_TILE_ROWS='16')))
        if all(x.get('capture') for x in row['modes'].values()):
            row['diff']=difference(row['modes']['baseline']['capture']['path'],row['modes']['candidate']['capture']['path'])
        if not exact:
            native=json.loads((args.audit/i/'mandel/command.json').read_text());native[native.index('-r')+1]=f'{size[0]}x{size[1]}'
            native[native.index('-o')+1]=str(out/mode/i/'native/scene.png')
            report['binaries'][native[0]]=sha256(Path(native[0]))
            row['modes']['native']=capture(native,out/mode/i/'native',size,240)
            if row['modes']['native'].get('capture'):
                for label in ['baseline','candidate']:
                    if row['modes'][label].get('capture'):
                        row[label+'_native_diff']=difference(row['modes']['native']['capture']['path'],row['modes'][label]['capture']['path'])
        print(mode,i,{k:v['status'] for k,v in row['modes'].items()},row.get('diff'),flush=True)
        (out/'summary.json').write_text(json.dumps(report,indent=2)+'\n')
    rows=[r for r in report['rows'] if not r['require_exact']]
    sheet=Image.new('RGB',(900,40+200*len(rows)),'#191c1e');draw=ImageDraw.Draw(sheet)
    for c,title in enumerate(['Native original authored','Previous FPT authored','FPT with auxiliary directional']):draw.text((c*300+7,10),title,fill='white')
    for r,row in enumerate(rows):
        for c,label in enumerate(['native','baseline','candidate']):
            if row['modes'][label].get('capture'):
                with Image.open(row['modes'][label]['capture']['path']) as im:sheet.paste(im.convert('RGB'),(c*300+(300-im.width)//2,40+r*200))
        draw.text((8,205+r*200),row['id']+' | original authored settings; unsupported effects remain',fill='white')
    sheet.save(out/'comparison.png')
    if any(any(v['status']!='ok' for v in r['modes'].values()) or (r['require_exact'] and r.get('diff',{}).get('changed_pixels')!=0) for r in report['rows']):
        raise SystemExit('capture or exact regression gate failed')


if __name__=='__main__':main()
