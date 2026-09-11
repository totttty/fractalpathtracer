#!/usr/bin/env python3
"""Summarize execution screening without promoting it to visual-parity certification."""
import argparse
from collections import Counter, defaultdict, deque
import csv
import json
import math
from pathlib import Path
import textwrap

from PIL import Image, ImageDraw

from run_release_canaries import sha256

MODES = ('geometry', 'authored')


def summarize(report, selection_count=50, excluded_ids=()):
    expected = report['identity']['manifest']['scenes']
    by_id = {row['id']:row for row in report['rows']}
    rows = []
    for source in expected:
        row = by_id.get(source['id'], dict(source, modes={}))
        modes = row['modes']
        status = {mode:modes.get(mode, {}).get('status', 'pending') for mode in MODES}
        flags = {mode:modes.get(mode, {}).get('screening', {}).get('review_flags', []) for mode in MODES}
        rows.append(dict(id=source['id'], path=source['path'], sha256=source['sha256'],
            collection=source['collection'], status=status, review_flags=flags,
            execution_passed=all(s=='ok' for s in status.values()),
            failure_details={mode:(modes[mode].get('stderr_tail') or modes[mode].get('error',''))
                for mode in MODES if status[mode] not in ('ok','pending')},
            wall_seconds={mode:modes.get(mode, {}).get('capture', {}).get('wall_seconds') for mode in MODES},
            visual_review='not_established'))
    # Round-robin collections, rather than filling the next review batch from
    # the alphabetically first author's collection. Dark/blank flags need triage.
    groups = defaultdict(deque)
    for row in rows:
        if row['id'] not in excluded_ids and row['execution_passed'] and not any(row['review_flags'].values()):
            groups[row['collection']].append(row)
    selected = []
    while groups and len(selected) < selection_count:
        for collection in sorted(list(groups)):
            selected.append(groups[collection].popleft())
            if not groups[collection]:
                del groups[collection]
            if len(selected) == selection_count:
                break
    return dict(scope='Continuous FPT Metal execution screening, not NAADF/CVOX or appearance parity.',
        visual_parity_certified=False, settings=report['identity']['settings'],
        total_scenes=len(expected), execution_passed=sum(r['execution_passed'] for r in rows),
        completed_scenes=sum(all(s!='pending' for s in r['status'].values()) for r in rows),
        status_counts={mode:dict(Counter(r['status'][mode] for r in rows)) for mode in MODES},
        flagged_scenes=sum(any(r['review_flags'].values()) for r in rows), rows=rows,
        next_review_ids=[r['id'] for r in selected],
        selection_provisional=any(s=='pending' for row in rows for s in row['status'].values()),
        selection_note='Collection-balanced execution-pass candidates only; selection does not certify visual quality.')


def publish(report, output):
    result = summarize(report)
    by_id = {r['id']:r for r in report['rows']}
    for row in report['rows']:
        for mode in MODES:
            for key in ('capture','diagnostic_capture','metadata'):
                asset=row['modes'].get(mode,{}).get(key)
                if asset and sha256(Path(asset['path']))!=asset['sha256']:
                    raise ValueError('screening artifact hash mismatch: '+asset['path'])
    output.mkdir(parents=True,exist_ok=False)
    (output/'screening.json').write_text(json.dumps(result,indent=2)+'\n')
    selected=set(result['next_review_ids'])
    manifest=report['identity']['manifest']
    next_batch=dict(version=1,root=manifest['root'],
        scope='Candidate batch for fresh reference-backed review, not a supported-scenes list.',
        provisional=result['selection_provisional'],
        scenes=[r for r in manifest['scenes'] if r['id'] in selected])
    (output/'next-review50.json').write_text(json.dumps(next_batch,indent=2)+'\n')
    with (output/'triage.csv').open('w',newline='') as stream:
        writer=csv.writer(stream)
        writer.writerow(['id','scene','geometry','authored','geometry_flags','authored_flags','next_review_candidate','failure_details'])
        for row in result['rows']:
            writer.writerow([row['id'],row['path'],*[row['status'][m] for m in MODES],
                *[';'.join(row['review_flags'][m]) for m in MODES],row['id'] in selected,
                json.dumps(row['failure_details'])])
    sheets=[]
    for start in range(0,len(result['rows']),50):
        chunk=result['rows'][start:start+50]
        canvas=Image.new('RGB',(1200,65+math.ceil(len(chunk)/4)*174),'#202326')
        draw=ImageDraw.Draw(canvas)
        draw.text((12,10),'FPT Metal screening | Geometry left / authored right | Not reference-validated',fill='white')
        draw.text((12,29),f"{result['settings']['max_axis']}px max edge, {result['settings']['samples']} SPP | Original aspect ratio | Page {start//50+1}",fill='#cccccc')
        for i,row in enumerate(chunk):
            x=(i%4)*300
            y=65+(i//4)*174
            original=by_id.get(row['id'],dict(modes={}))
            for col,mode in enumerate(MODES):
                capture=original['modes'].get(mode,{})
                asset=capture.get('capture',capture.get('diagnostic_capture'))
                if asset:
                    with Image.open(asset['path']) as source:
                        image=source.convert('RGB')
                        image.thumbnail((138,96))
                        canvas.paste(image,(x+col*150+(150-image.width)//2,y+(96-image.height)//2))
                else:
                    label=capture.get('status','pending')
                    for j,line in enumerate(textwrap.wrap(label,22)):
                        draw.text((x+col*150+5,y+35+j*13),line,fill='#ffaaaa')
            title=row['id']+' '+Path(row['path']).stem
            for j,line in enumerate(textwrap.wrap(title,43)[:2]):
                draw.text((x+7,y+100+j*14),line,fill='white')
            warning='; '.join(f'{m}: {",".join(row["review_flags"][m])}' for m in MODES if row['review_flags'][m])
            if not row['execution_passed']:
                warning=' / '.join(row['status'][m] for m in MODES)
            draw.text((x+7,y+132),warning[:44] or 'Execution only; visual review pending',fill='#ffbf80' if warning else '#aaaaaa')
        name=f'screening-{start//50+1:02d}.png'
        canvas.save(output/name)
        sheets.append(name)
    lines=['# Remaining Mandel Scene Screening','',result['scope'],'',
        f"- Completed: {result['completed_scenes']} / {result['total_scenes']} scenes.",
        f"- Both FPT modes executed: {result['execution_passed']}.",
        f"- Scenes with advisory dark/blank flags: {result['flagged_scenes']}.",'',
        '**These low-sample previews have no native-reference comparison and are not a release-quality gallery.**',
        'Fog/cloud limitations remain deferred. Timings include process startup and compilation; they are not GPU benchmarks.','',
        'The next-review manifest balances source collections, excluding execution failures and automatic dark/blank flags. '
        'It still requires native comparisons and manual review before publication. '
        + ('Selection is provisional because the full inventory has not finished screening.' if result['selection_provisional'] else ''),'',
        '[Triage table](triage.csv) | [Full summary](screening.json) | [Next review candidates](next-review50.json)','']
    for name in sheets:
        lines.extend([f'## {name}',f'![Geometry and authored screening]({name})',''])
    (output/'README.md').write_text('\n'.join(lines)+'\n')
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    result=publish(json.loads(args.report.read_text()),args.output)
    print(json.dumps({k:result[k] for k in ('total_scenes','completed_scenes','execution_passed','flagged_scenes','status_counts')},indent=2))


if __name__=='__main__':
    main()
