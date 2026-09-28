#!/usr/bin/env python3
"""Independently verify capture metadata, source bindings and gallery assets."""
import argparse
from collections import Counter
import hashlib
from html.parser import HTMLParser
import json
from pathlib import Path
from PIL import Image


def digest(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


class Links(HTMLParser):
    def __init__(self):
        super().__init__(); self.images=[]; self.links=[]; self.figures=0
    def handle_starttag(self,tag,attrs):
        a=dict(attrs)
        if tag=='img':self.images.append(a)
        if tag=='a':self.links.append(a)
        if tag=='figure':self.figures+=1


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('output',type=Path);a=p.parse_args()
    out=a.output.resolve();report=json.loads((out/'report.json').read_text())
    source=Path(report['source_report']);assert digest(source)==report['source_report_sha256']
    expected={r['id']:r for r in json.loads(source.read_text())['scenes']}
    assert len(expected)==427
    actual={r['id']:r for r in report['scenes']}
    assert len(actual)==len(report['scenes']) and actual.keys()==expected.keys()
    assert report['profile']==dict(size=[1280,720],samples=32,bounces=4)
    assert report['renderer_unchanged'] and report['totals']==dict(expected=427,rendered=427,failed=0)
    modes=Counter()
    for sid,row in actual.items():
        assert row['rendered'] and row['manifest_sha256']==expected[sid]['manifest_sha256']
        assert digest(Path(row['manifest']))==row['manifest_sha256']
        p=Path(row['receipt']);assert digest(p)==row['receipt_sha256']
        receipt=json.loads(p.read_text());assert receipt['renderer_identity']==report['renderer']
        assert receipt['bundle_sha256']==row['manifest_sha256']
        command=receipt['command']
        assert command[command.index('--size')+1]=='1280x720'
        assert command[command.index('--gpu-naadf-path-max-samples')+1]=='32'
        assert command[command.index('--gpu-naadf-path-bounces')+1]=='4'
        stats=json.loads((p.parent/'render.json').read_text())[0]
        assert stats['gpu_renderer']=='naadf' and stats['naadf_path_local_cubes']
        assert stats['naadf_path_samples']==32 and stats['naadf_path_max_samples']==32 and stats['naadf_path_bounces']==4
        assert stats['naadf_path_primary_rays_per_frame']==1280*720*32
        modes[stats['naadf_path_tracing']]+=1
        raw=p.parent/'render-naadf_aadf_cpu-normal.ppm'
        assert digest(raw)==row['raw_sha256']==receipt['image_sha256']
        with Image.open(raw) as im:
            assert im.size==(1280,720)
            assert any(lo!=hi for lo,hi in im.convert('RGB').getextrema())
        image=out/'site/dist'/row['image'];assert digest(image)==row['image_sha256']==receipt['gallery_sha256']
        with Image.open(image) as im:assert im.size==(1280,720);im.verify()
        with Image.open(out/'site/dist/thumbs'/f'{sid}.webp') as im:assert im.size==(480,270);im.verify()
    parser=Links();parser.feed((out/'site/dist/index.html').read_text())
    assert parser.figures==len(parser.images)==len(parser.links)==427
    ids=sorted(expected,key=int)
    assert [r['src'] for r in parser.images]==[f'thumbs/{sid}.webp' for sid in ids]
    assert [r['href'] for r in parser.links]==[f'images/{sid}.webp' for sid in ids]
    for r in parser.images:assert r['alt'] and r['loading']=='lazy'
    result=dict(verified=True,scenes=len(actual),size=[1280,720],samples=32,bounces=4,
                renderer='naadf',path_modes=dict(modes),gallery_links=len(parser.links),
                report_sha256=digest(out/'report.json'),index_sha256=digest(out/'site/dist/index.html'))
    (out/'verification.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result))


if __name__=='__main__':main()
