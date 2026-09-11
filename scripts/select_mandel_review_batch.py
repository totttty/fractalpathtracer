#!/usr/bin/env python3
"""Select a deterministic, collection-balanced batch without re-reviewing scenes."""
import argparse
from collections import Counter
import json
from pathlib import Path

from mandel_catalog import ROOT, matching_rows, sha256, source_index
from summarize_mandel_screening import summarize


def select(catalog, screening, count):
    if not 1 <= count <= 50:
        raise ValueError('batch size must be 1..50')
    sources=source_index(catalog['scenes'])
    original=matching_rows(screening['identity']['manifest']['scenes'],sources)
    observed=matching_rows(screening['rows'],sources)
    if set(original)!=set(observed):
        raise ValueError('incomplete screening inventory')
    excluded={r['id'] for r in catalog['scenes'] if r['status']!='experimental' or 'visual_decision' in r}
    result=summarize(screening,selection_count=count,excluded_ids=excluded)
    if result['selection_provisional'] or len(result['next_review_ids'])!=count:
        raise ValueError('insufficient complete, unreviewed execution-pass candidates')
    rows=[original[int(key)] for key in result['next_review_ids']]
    return dict(version=1,root=screening['identity']['manifest']['root'],provisional=False,
        scope='Collection-balanced visual review candidates, not supported-scene certification.',
        selection_policy='Exclude non-experimental or already decided scenes; require both modes passed with no dark/blank warnings. Round-robin source collections in deterministic source order.',
        collections=dict(Counter(r['collection'] for r in rows)),scenes=rows)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--catalog',type=Path,default=ROOT/'docs/mandel-catalog/catalog.json')
    parser.add_argument('--screening',type=Path,required=True)
    parser.add_argument('--count',type=int,default=50)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    result=select(json.loads(args.catalog.read_text()),json.loads(args.screening.read_text()),args.count)
    result['input_sha256']=dict(catalog=sha256(args.catalog),screening=sha256(args.screening))
    args.output.parent.mkdir(parents=True,exist_ok=True)
    with args.output.open('x') as stream:stream.write(json.dumps(result,indent=2)+'\n')
    print(json.dumps(dict(ids=[r['id'] for r in result['scenes']],collections=result['collections']),indent=2))


if __name__=='__main__':main()
