#!/usr/bin/env python3
"""Select a source-pinned subset for the existing resumable support suite."""
import argparse
import json
from pathlib import Path


def select(report, ids):
    source = report['identity']['manifest']
    if len(ids) != len(set(ids)) or not set(ids) <= {r['id'] for r in source['scenes']}:
        raise ValueError('unknown or duplicate refresh IDs')
    return dict(version=1, root=source['root'],
        scope='Targeted FPT refresh; native references reused separately after hash checks.',
        scenes=[r for r in source['scenes'] if r['id'] in ids])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--ids', nargs='+', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    result = select(json.loads(args.report.read_text()), args.ids)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open('x') as stream:
        stream.write(json.dumps(result, indent=2)+'\n')


if __name__ == '__main__':
    main()
