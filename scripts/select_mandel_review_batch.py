#!/usr/bin/env python3
"""Select a deterministic, collection-balanced batch without re-reviewing scenes."""
import argparse
from collections import Counter
import json
from pathlib import Path

from mandel_catalog import ROOT, matching_rows, sha256, source_index
from summarize_mandel_screening import summarize


def select(catalog, screening, count, deferred=()):
    if not 1 <= count <= 50:
        raise ValueError('batch size must be 1..50')
    sources=source_index(catalog['scenes'])
    original=matching_rows(screening['identity']['manifest']['scenes'],sources)
    observed=matching_rows(screening['rows'],sources)
    if set(original)!=set(observed):
        raise ValueError('incomplete screening inventory')
    excluded={r['id'] for r in catalog['scenes'] if r['status']!='experimental' or 'visual_decision' in r}
    deferred_ids=set()
    for inventory in deferred:
        deferred_ids.update(r['id'] for r in matching_rows(inventory['rows'],sources).values())
    excluded.update(deferred_ids)
    result=summarize(screening,selection_count=count,excluded_ids=excluded)
    if result['selection_provisional'] or len(result['next_review_ids'])!=count:
        raise ValueError('insufficient complete, unreviewed execution-pass candidates')
    rows=[original[int(key)] for key in result['next_review_ids']]
    return dict(version=1,root=screening['identity']['manifest']['root'],provisional=False,
        scope='Collection-balanced visual review candidates, not supported-scene certification.',
        selection_policy='Exclude non-experimental, already decided and deferred incomplete scenes; require both modes passed with no dark/blank warnings. Round-robin source collections in deterministic source order.',
        deferred_ids=sorted(deferred_ids,key=int),
        collections=dict(Counter(r['collection'] for r in rows)),scenes=rows)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--catalog',type=Path,default=ROOT/'docs/mandel-catalog/catalog.json')
    parser.add_argument('--screening',type=Path,required=True)
    parser.add_argument('--count',type=int,default=50)
    parser.add_argument('--deferred',type=Path,action='append',help='Additional source-bound deferred inventory; checked-in incomplete-batch inventories are always excluded')
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    deferred=sorted(set(args.catalog.parent.glob('incomplete-batch*.json'))|set(args.deferred or []))
    result=select(json.loads(args.catalog.read_text()),json.loads(args.screening.read_text()),args.count,
        [json.loads(p.read_text()) for p in deferred])
    result['input_sha256']=dict(catalog=sha256(args.catalog),screening=sha256(args.screening),
        deferred={str(p):sha256(p) for p in deferred})
    args.output.parent.mkdir(parents=True,exist_ok=True)
    with args.output.open('x') as stream:stream.write(json.dumps(result,indent=2)+'\n')
    print(json.dumps(dict(ids=[r['id'] for r in result['scenes']],collections=result['collections']),indent=2))


if __name__=='__main__':main()
