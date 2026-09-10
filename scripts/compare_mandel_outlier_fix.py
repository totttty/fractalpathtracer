#!/usr/bin/env python3
"""Hash-checked authored reference/before/after review, without image adjustments."""
import argparse
import json
from pathlib import Path

from PIL import Image, ImageDraw
from run_release_canaries import difference, sha256


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--baseline', type=Path, required=True)
    p.add_argument('--candidate', type=Path, required=True)
    p.add_argument('--notes', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--scenes', nargs='+', required=True)
    args = p.parse_args()
    before = json.loads(args.baseline.read_text())
    after = json.loads(args.candidate.read_text())
    notes = json.loads(args.notes.read_text())
    b = {r['id']: r for r in before['rows']}
    a = {r['id']: r for r in after['rows']}
    for report in (before, after):
        if report['identity']['settings']['samples'] != 32 or report['identity']['settings']['max_axis'] != 300:
            raise ValueError('expected 300px max edge and 32 FPT samples')
    records = []
    for scene_id in args.scenes:
        old, new = b[scene_id], a[scene_id]
        if old['sha256'] != new['sha256'] or old['size'] != new['size']:
            raise ValueError('source/framing identity changed')
        results = [old['modes']['mandel'], old['modes']['authored'], new['modes']['authored']]
        for result in results:
            asset = result['capture']
            if result['status'] != 'ok' or sha256(Path(asset['path'])) != asset['sha256']:
                raise ValueError('invalid or changed capture')
            with Image.open(asset['path']) as im:
                if list(im.size) != old['size']:
                    raise ValueError('capture dimensions differ')
        records.append(dict(id=scene_id, source=old['path'], source_sha256=old['sha256'],
                            size=old['size'], captures=[r['capture'] for r in results], note=notes[scene_id],
                            reference_overrides=old.get('reference_overrides', {}),
                            before=difference(results[0]['capture']['path'], results[1]['capture']['path']),
                            after=difference(results[0]['capture']['path'], results[2]['capture']['path'])))
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    pages = []
    for start in range(0, len(records), 4):
        rows = records[start:start+4]
        im = Image.new('RGB', (900, 70+len(rows)*355), '#202326')
        d = ImageDraw.Draw(im)
        d.text((8, 8), 'Continuous FPT Metal | 300px max edge | FPT 32 SPP | review, not full parity', fill='white')
        for col, label in enumerate(('Mandelbulber CPU authored', 'FPT before', 'FPT after')):
            d.text((col*300+8, 36), label, fill='white')
        for i, row in enumerate(rows):
            top = 70+i*355
            for col, asset in enumerate(row['captures']):
                with Image.open(asset['path']) as source:
                    im.paste(source.convert('RGB'), (col*300+(300-source.width)//2, top+(300-source.height)//2))
            d.text((8, top+303), row['id']+' '+Path(row['source']).stem, fill='white')
            d.text((8, top+320), row['note'], fill='#ffce86')
            d.text((8, top+336), f"Authored RGB MAE {row['before']['mae']:.4f} -> {row['after']['mae']:.4f}; not a geometry score", fill='#cccccc')
        name = f'comparison-{start//4+1:02d}.png'
        im.save(out/name)
        pages.append(name)
    report = dict(scope='Continuous FPT Metal, not NAADF/CVOX; unchanged camera and source, different integrators.',
                  baseline_report_sha256=sha256(args.baseline), candidate_report_sha256=sha256(args.candidate),
                  harness_sha256=sha256(Path(__file__)), rows=records,
                  pages={name: sha256(out/name) for name in pages})
    (out/'summary.json').write_text(json.dumps(report, indent=2)+'\n')
    lines = ['# Remaining Scene Fix Review', '',
             'Native authored reference / previous FPT / corrected FPT. 300px max edge, 32 FPT SPP.',
             'Continuous FPT Metal only. This is not NAADF/CVOX certification or a performance benchmark.', '',
             '## Comparisons', '']
    for name in pages:
        lines.extend([f'![Reference, before and after]({name})', ''])
    lines.extend(['## Findings', ''])
    lines.extend(f"- **{r['id']}:** {r['note']}" for r in records)
    (out/'README.md').write_text('\n'.join(lines)+'\n')
    print(f'Wrote {len(records)} reviewed rows and {len(pages)} sheets')


if __name__ == '__main__':
    main()
