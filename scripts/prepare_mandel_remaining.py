#!/usr/bin/env python3
"""Inventory unreviewed unique Mandel scenes without copying upstream source files."""
import argparse
import json
from collections import defaultdict
from pathlib import Path

from run_release_canaries import sha256, validate_manifest
from run_mandel_support_suite import parameters


def inventory(root, reviewed):
    validate_manifest(reviewed, root)
    reviewed_paths = {row['path'] for row in reviewed['scenes']}
    reviewed_hashes = {row['sha256'] for row in reviewed['scenes']}
    groups = defaultdict(list)
    for path in sorted(root.rglob('*.fract')):
        if not path.resolve().is_relative_to(root.resolve()):
            raise ValueError('source symlink outside scene root')
        groups[sha256(path)].append(path.relative_to(root).as_posix())
    remaining = sorted((paths[0], digest, paths) for digest, paths in groups.items()
                       if digest not in reviewed_hashes)
    rows = []
    first_id = max(int(row['id']) for row in reviewed['scenes']) + 1
    for number, (relative, digest, aliases) in enumerate(remaining, first_id):
        values = parameters((root / relative).read_text())
        rows.append(dict(id=f'{number:03d}', path=relative, sha256=digest,
            aliases=aliases[1:], collection=Path(relative).parts[0] if '/' in relative else 'root examples',
            formula_parameters={k:v for k,v in values.items() if k.startswith('formula')},
            declared_file_parameters={k:v for k,v in values.items() if 'file' in k.lower()},
            volume_parameters={k:v for k,v in values.items() if any(s in k for s in ('fog','cloud'))}))
    return dict(version=1, root='Mandelbulber examples (supplied explicitly)',
        scope='Unreviewed continuous FPT Metal scenes; not NAADF/CVOX certification.',
        counts=dict(source_files=sum(map(len,groups.values())), unique_hashes=len(groups),
            reviewed_paths=len(reviewed_paths), remaining_paths=sum(p not in reviewed_paths for paths in groups.values() for p in paths),
            remaining_unique_scenes=len(rows)),
        asset_policy='Declared file parameters are inventory only, not resolved-asset certification.',
        scenes=rows)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--scene-root',type=Path,required=True)
    parser.add_argument('--reviewed',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    result=inventory(args.scene_root,json.loads(args.reviewed.read_text()))
    result['reviewed_manifest_sha256']=sha256(args.reviewed)
    args.output.parent.mkdir(parents=True,exist_ok=True)
    with args.output.open('x') as stream:
        stream.write(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result['counts'],indent=2))


if __name__=='__main__':
    main()
