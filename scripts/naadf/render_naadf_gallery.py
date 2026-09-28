#!/usr/bin/env python3
"""Render a resumable gallery from a source-bound NAADF adaptive catalog report."""
import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import html
import json
from pathlib import Path
import re
import subprocess
import sys
import time
from PIL import Image


def write_json(path, value):
    temporary = path.with_suffix(path.suffix + '.tmp')
    temporary.write_text(json.dumps(value, indent=2) + '\n')
    temporary.replace(path)


def write_gallery(output, rows, count):
    ready = sorted((r for r in rows if r.get('rendered')), key=lambda r: int(r['id']))
    cards = '\n'.join(
        f'<figure><a href="images/{html.escape(r["id"])}.webp" aria-label="Open scene {html.escape(r["id"])} at 1280 by 720">'
        f'<img src="thumbs/{html.escape(r["id"])}.webp" alt="NAADF render of scene {html.escape(r["id"])}" '
        'width="480" height="270" loading="lazy" decoding="async"></a>'
        f'<figcaption>Scene {html.escape(r["id"])}</figcaption></figure>' for r in ready)
    page = '''<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1"><meta name="color-scheme" content="dark">
<title>NAADF Scene Gallery</title><meta name="description" content="427 adaptive NAADF scenes at 1280 by 720, 32 samples and 4 bounces.">
<style>
*{box-sizing:border-box}body{margin:0;background:#10141c;color:#f2f5fa;font:16px/1.5 system-ui,-apple-system,BlinkMacSystemFont,sans-serif}
header,main,footer{max-width:1600px;margin:auto;padding:24px}header{padding-bottom:18px}h1{font-size:clamp(24px,4vw,36px);letter-spacing:-.035em;line-height:1.2;margin:0 0 10px}p{margin:6px 0;color:#aebacd}.settings{color:#e0e6ef;font-size:15px}.count{display:inline-block;margin-top:10px;color:#c0d9c8;font-variant-numeric:tabular-nums}
main{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:24px;padding-top:0}figure{margin:0;min-width:0;background:#1b2330;border-radius:10px;overflow:hidden}a{display:block;color:inherit}a:focus-visible{outline:3px solid #99c8ff;outline-offset:-3px}img{display:block;width:100%;height:auto;aspect-ratio:16/9;object-fit:contain;background:#080c12}figcaption{padding:10px 14px;font-weight:600;font-variant-numeric:tabular-nums}footer{padding-top:0;font-size:13px;color:#9cabc0}
@media(min-width:1450px){main{grid-template-columns:repeat(4,minmax(0,1fr))}}@media(max-width:800px){header,main,footer{padding-left:14px;padding-right:14px}main{grid-template-columns:repeat(2,minmax(0,1fr));gap:12px}figcaption{padding:8px 10px;font-size:14px}}@media(max-width:360px){main{grid-template-columns:1fr}}
</style></head><body><header><h1>NAADF Scene Gallery</h1><p class="settings">1280 × 720 &nbsp;·&nbsp; 32 samples &nbsp;·&nbsp; 4 bounces</p><p>Open a scene to view it at full resolution.</p><span class="count">COUNT</span></header><main>CARDS</main><footer>Rendered 15 September 2026 using the adaptive NAADF renderer. Visual parity with FPT remains in progress.</footer></body></html>'''
    page = page.replace('COUNT', f'{len(ready)} / {count} scenes').replace('CARDS', cards)
    destination = output/'site/dist/index.html'
    temporary = destination.with_suffix('.tmp')
    temporary.write_text(page)
    temporary.replace(destination)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--native', type=Path, required=True)
    p.add_argument('--catalog-report', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--jobs', type=int, choices=(1,2), default=2)
    args = p.parse_args()
    sys.path.insert(0,str(args.native/'tools'))
    from view_fpt_bundle import prepare, capture_health, renderer_identity
    from view_voxel_scene import sha256
    app = args.native/'build/MetalVoxel.app/Contents/MacOS/MetalVoxel'
    identity = renderer_identity(app)
    catalog = json.loads(args.catalog_report.read_text())
    assert identity == catalog['renderer'], 'renderer changed since source report'
    cases = sorted(catalog['scenes'],key=lambda r:int(r['id']))
    assert cases and len({r['id'] for r in cases}) == len(cases)
    for row in cases:
        assert re.fullmatch(r'[0-9]+',row['id'])
        assert row['rendered'] and sha256(Path(row['manifest'])) == row['manifest_sha256']
    out = args.output.resolve()
    for d in ('captures','site/dist/images','site/dist/thumbs'):
        (out/d).mkdir(parents=True, exist_ok=True)
    report = dict(version=1,source_report=str(args.catalog_report.resolve()),source_report_sha256=sha256(args.catalog_report),
                  renderer=identity,profile=dict(size=[1280,720],samples=32,bounces=4),
                  gallery_encoding='WebP quality 75; original lossless PPM captures retained separately',
                  naadf_accepted=False,scenes=[])
    start = time.monotonic()
    def run(source):
        sid = source['id']; folder = out/'captures'/sid
        folder.mkdir(exist_ok=True)
        row = dict(id=sid,manifest=source['manifest'],manifest_sha256=source['manifest_sha256'],rendered=False)
        try:
            receipt_path=folder/'receipt.json'
            raw=folder/'render-naadf_aadf_cpu-normal.ppm'
            receipt=json.loads(receipt_path.read_text()) if receipt_path.exists() else None
            reusable=(receipt and raw.exists() and receipt.get('bundle_sha256')==source['manifest_sha256']
                      and receipt.get('renderer_identity')==identity and receipt.get('size')==[1280,720]
                      and receipt.get('samples')==32 and receipt.get('bounces')==4
                      and (not receipt.get('image_sha256') or sha256(raw)==receipt['image_sha256']))
            if not reusable:
                command, env, receipt = prepare(Path(source['manifest']),app,maximum_axis=1280,
                    samples=32,bounces=4,destination=folder,diagnostic=True)
                command[command.index('--size')+1]='1280x720';receipt['size']=[1280,720]
                receipt['environment']={k:v for k,v in env.items() if k.startswith('NAADF_PATH_')}
                write_json(folder/'launch.json',receipt)
                t=time.monotonic()
                with (folder/'render.log').open('w') as log:
                    subprocess.run(command,env=env,stdout=log,stderr=subprocess.STDOUT,check=True,timeout=1800)
                receipt['seconds']=time.monotonic()-t
            stats=json.loads((folder/'render.json').read_text())[0]
            assert stats['gpu_renderer']=='naadf' and stats['naadf_path_local_cubes']
            assert stats['naadf_path_samples']==32 and stats['naadf_path_bounces']==4
            assert stats['naadf_path_primary_rays_per_frame']==1280*720*32
            health=capture_health(folder)
            assert len(health)==1 and all(h['size']==[1280,720] and not h['all_black'] and not h['uniform'] for h in health.values()),health
            image=out/'site/dist/images'/f'{sid}.webp'
            thumb=out/'site/dist/thumbs'/f'{sid}.webp'
            with Image.open(raw) as img:
                rgb=img.convert('RGB');rgb.save(image,quality=75,method=4)
                rgb.thumbnail((480,270),Image.Resampling.LANCZOS);rgb.save(thumb,quality=84,method=4)
            receipt.update(capture_health=health,image_sha256=sha256(raw),gallery_sha256=sha256(image),execution_success=True)
            write_json(receipt_path,receipt)
            row.update(rendered=True,receipt=str(receipt_path),receipt_sha256=sha256(receipt_path),
                       image=f'images/{sid}.webp',image_sha256=sha256(image),raw_sha256=sha256(raw),
                       gpu_ms=stats['gpu_ms'],seconds=receipt['seconds'],health=health)
        except Exception as exc:
            row['error']=f'{type(exc).__name__}: {exc}'
        return row
    def save():
        report['scenes'].sort(key=lambda r:int(r['id']))
        report['seconds']=time.monotonic()-start
        report['totals']=dict(expected=len(cases),rendered=sum(r['rendered'] for r in report['scenes']),
                              failed=sum(not r['rendered'] for r in report['scenes']))
        write_json(out/'report.json',report)
        write_gallery(out,report['scenes'],len(cases))
    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        for f in as_completed([pool.submit(run,r) for r in cases]):
            row=f.result();report['scenes'].append(row);save()
            print(f"{len(report['scenes'])}/{len(cases)} scene {row['id']}: {'rendered' if row['rendered'] else row['error']} ({time.monotonic()-start:.1f}s)",flush=True)
    report['renderer_unchanged']=renderer_identity(app)==identity
    save()
    assert report['renderer_unchanged'] and report['totals']['rendered']==len(cases), report['totals']
    print(json.dumps(report['totals']),flush=True)


if __name__=='__main__':
    main()
