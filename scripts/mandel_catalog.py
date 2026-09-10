#!/usr/bin/env python3
"""Build, inspect and render the source-pinned experimental Mandel catalogue."""
import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shlex
import subprocess
from urllib.parse import quote

ROOT = Path(__file__).resolve().parents[1]
CATALOG = ROOT / 'docs/mandel-catalog/catalog.json'
MODES = ('geometry', 'authored')
STATUSES = ('reviewed', 'experimental', 'blocked')


def sha256(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def source_index(rows):
    indexed = {}
    hashes = set()
    for row in rows:
        path = PurePosixPath(row['path'])
        if (not re.fullmatch(r'[0-9]+', row['id']) or int(row['id']) in indexed
                or path.is_absolute() or '..' in path.parts or '\\' in row['path']
                or path.suffix != '.fract'
                or not re.fullmatch(r'[0-9a-f]{64}', row['sha256'])
                or row['sha256'] in hashes):
            raise ValueError('invalid or duplicate scene identity: ' + row['id'])
        indexed[int(row['id'])] = row
        hashes.add(row['sha256'])
    return indexed


def matching_rows(rows, sources, gallery=False):
    indexed = {}
    for row in rows:
        key = int(row['id'])
        source = sources.get(key)
        if (key in indexed or source is None
                or row['id'] != source['id']
                or row['source' if gallery else 'path'] != source['path']
                or row['source_sha256' if gallery else 'sha256'] != source['sha256']):
            raise ValueError('evidence/source identity mismatch: ' + row['id'])
        indexed[key] = row
    return indexed


def build_catalog(ranked, remaining, gallery, screening, additional):
    sources = source_index(ranked['scenes'] + remaining['scenes'])
    ranked_ids = {int(r['id']) for r in ranked['scenes']}
    remaining_ids = set(sources) - ranked_ids
    reviewed = matching_rows(gallery['rows'], sources, gallery=True)
    extra = matching_rows(additional['rows'], sources, gallery=True)
    screened = matching_rows(screening['rows'], sources)
    screening_sources = matching_rows(screening['identity']['manifest']['scenes'], sources)
    if (set(reviewed) != ranked_ids or set(screened) != remaining_ids
            or set(screening_sources) != remaining_ids or not set(extra) <= remaining_ids):
        raise ValueError('incomplete or overlapping evidence coverage')
    revision = gallery['upstream_revision']
    if not re.fullmatch(r'[0-9a-f]{40}', revision) or additional['upstream_revision'] != revision:
        raise ValueError('upstream revision mismatch')
    rows = []
    for key, source in sorted(sources.items()):
        screen = screened.get(key)
        review = reviewed.get(key, extra.get(key))
        execution = {}
        if screen:
            for mode in MODES:
                result = screen['modes'].get(mode, {})
                status = result.get('status', 'pending')
                if status == 'pending':
                    raise ValueError('incomplete screening: ' + source['id'])
                execution[mode] = dict(status=status,
                    flags=result.get('screening', {}).get('review_flags', []))
                capture = result.get('capture', result.get('diagnostic_capture'))
                if capture:
                    execution[mode]['capture_sha256'] = capture['sha256']
        blocked = [f'{mode}: {r["status"]} in historical screening; needs retest'
                   for mode, r in execution.items() if r['status'] != 'ok']
        if key in (46, 48) and key in ranked_ids:
            blocked.append('Known production deep-zoom/precision geometry failure; see ranked-50 gallery.')
        status = 'blocked' if blocked else 'reviewed' if key in ranked_ids else 'experimental'
        path = PurePosixPath(source['path'])
        row = dict(id=source['id'], path=source['path'], sha256=source['sha256'],
            aliases=source.get('aliases', []), name=path.stem,
            collection=path.parts[0] if len(path.parts) > 1 else 'Mandelbulber example collection',
            source_url=f'https://github.com/buddhi1980/mandelbulber2/blob/{revision}/mandelbulber2/deploy/share/mandelbulber2/examples/{quote(source["path"])}',
            status=status, blockers=blocked, screening=execution,
            review='ranked-gallery' if key in ranked_ids else 'additional-reference-review' if review else 'not-reviewed',
            review_note=review.get('note', '') if review else '',
            gallery_published=key in ranked_ids,
            naadf_cvox_validation='not-established-by-this-catalogue')
        if review:
            row['review_reference_overrides'] = review.get('reference_overrides', {})
            row['review_capture_hashes'] = {m:c['sha256'] for m,c in review['captures'].items()}
        rows.append(row)
    return dict(version=1, scope='Continuous FPT Metal scene catalogue; not native visual parity or NAADF/CVOX certification.',
        upstream_revision=revision,
        status_policy=dict(reviewed='Published ranked gallery, with documented limitations; not exact parity.',
            experimental='Available for opt-in rendering; execution or additional review is not gallery approval.',
            blocked='Known gallery geometry failure or historical screening failure in at least one mode; explicit opt-in required.'),
        evidence_policy='Historical observations, not a fresh full-corpus run of the latest binary. Later targeted fixes do not automatically clear failures.',
        screening_settings=screening['identity']['settings'],
        screening_binary_sha256=sorted(set(screening['identity']['executables'].values())),
        gallery_binary_sha256=gallery['production_sha256'],
        counts=dict(total=len(rows), statuses=dict(Counter(r['status'] for r in rows)),
            gallery_published=len(reviewed), additional_reference_reviewed=len(extra),
            historical_both_modes_passed=sum(all(r['status']=='ok' for r in s.values()) for s in (r['screening'] for r in rows) if s)),
        scenes=rows)


def markdown(catalog):
    lines = ['# All Mandel Scenes', '', '[Usage and status definitions](README.md) | [Machine-readable catalogue](catalog.json)', '',
        'Historical observations only. Reviewed does not mean exact parity; no row certifies NAADF/CVOX output.', '',
        '| ID | Source / collection credit | Status | Review | Notes |', '| --- | --- | --- | --- | --- |']
    for row in catalog['scenes']:
        notes = row['blockers'] + [row['review_note']]
        notes += [f'{mode}: {", ".join(r["flags"])}' for mode,r in row['screening'].items() if r['flags']]
        safe = lambda s: s.replace('|', '\\|').replace('\n', ' ')
        lines.append(f'| {row["id"]} | [{safe(row["name"])}]({row["source_url"]}) / {safe(row["collection"])} | {row["status"]} | {row["review"]} | {safe("; ".join(n for n in notes if n))} |')
    return '\n'.join(lines) + '\n'


def resolve_scene(row, scene_root):
    source_index([row])
    root = scene_root.resolve()
    path = (root / row['path']).resolve()
    if not path.is_relative_to(root) or not path.is_file():
        raise ValueError('scene missing or outside supplied examples root: ' + row['path'])
    if sha256(path) != row['sha256']:
        raise ValueError('source hash mismatch; use the catalogue-pinned upstream revision')
    return path


def render_command(row, args):
    if row['status'] == 'blocked' and not args.allow_blocked:
        raise ValueError('blocked scene: ' + '; '.join(row['blockers']) + ' (use --allow-blocked to investigate)')
    if args.samples <= 0 or args.max_axis <= 0:
        raise ValueError('samples and max-axis must be positive')
    scene = resolve_scene(row, args.mandelbulber_root / 'deploy/share/mandelbulber2/examples')
    # Share the authored-aspect contract with the existing release captures.
    from run_release_canaries import scene_dimensions
    width, height = scene_dimensions(scene.read_text(), args.max_axis)
    return [str(args.fpt.resolve()), 'render', str(scene), '--renderer', 'sdf',
        '--mandelbulber-root', str(args.mandelbulber_root.resolve()),
        '--mandel-appearance', 'geometry' if args.mode == 'geometry' else 'authored-path',
        '--width', str(width), '--height', str(height), '--samples', str(args.samples),
        '--sdf-accumulation', 'chunked', '--sdf-chunk-samples', '1', '--out', str(args.out.resolve())]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--catalog', type=Path, default=CATALOG)
    sub = parser.add_subparsers(dest='command', required=True)
    build = sub.add_parser('build', help='Generate metadata only; does not copy scenes or render images')
    build.add_argument('--ranked', type=Path, default=ROOT/'tests/fixtures/mandel-release-ranked50.json')
    build.add_argument('--remaining', type=Path, default=ROOT/'tests/fixtures/mandel-release-remaining696.json')
    build.add_argument('--gallery', type=Path, default=ROOT/'docs/mandel-gallery/manifest.json')
    build.add_argument('--screening', type=Path, required=True)
    build.add_argument('--additional', type=Path, required=True)
    build.add_argument('--check', action='store_true')
    listing = sub.add_parser('list')
    listing.add_argument('--status', choices=STATUSES)
    listing.add_argument('--search', default='')
    show = sub.add_parser('show')
    show.add_argument('id', type=int)
    render = sub.add_parser('render', help='Opt-in continuous FPT render, not NAADF/CVOX export')
    render.add_argument('id', type=int)
    render.add_argument('--mandelbulber-root', type=Path, required=True)
    render.add_argument('--fpt', type=Path, default=ROOT/'target/release/fpt-metal')
    render.add_argument('--mode', choices=MODES, default='authored')
    render.add_argument('--max-axis', type=int, default=300)
    render.add_argument('--samples', type=int, default=32)
    render.add_argument('--out', type=Path, required=True)
    render.add_argument('--allow-blocked', action='store_true')
    render.add_argument('--dry-run', action='store_true')
    args = parser.parse_args()
    try:
        if args.command == 'build':
            files = {key:getattr(args, key) for key in ('ranked','remaining','gallery','screening','additional')}
            catalog = build_catalog(**{k:json.loads(p.read_text()) for k,p in files.items()})
            catalog['evidence_sha256'] = {k:sha256(p) for k,p in files.items()}
            outputs = {args.catalog:json.dumps(catalog, indent=2)+'\n', args.catalog.with_name('scenes.md'):markdown(catalog)}
            for path, content in outputs.items():
                if args.check:
                    if path.read_text() != content:
                        raise ValueError('catalogue is stale: ' + str(path))
                else:
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text(content)
            print(json.dumps(catalog['counts'], indent=2))
            return 0
        catalog = json.loads(args.catalog.read_text())
        if catalog['version'] != 1:
            raise ValueError('unsupported catalogue version')
        sources = source_index(catalog['scenes'])
        if args.command == 'list':
            for row in sources.values():
                if (not args.status or args.status == row['status']) and args.search.lower() in row['path'].lower():
                    print(f'{row["id"]}\t{row["status"]}\t{row["path"]}')
            return 0
        if args.id not in sources:
            raise ValueError('unknown scene ID')
        row = sources[args.id]
        if args.command == 'show':
            print(json.dumps(row, indent=2))
            return 0
        command = render_command(row, args)
        print(f'{row["id"]}: {row["status"]}. {row["review_note"]}', flush=True)
        print(shlex.join(command), flush=True)
        if args.dry_run:
            return 0
        if args.out.exists():
            raise ValueError('output already exists; choose a new directory')
        env = dict(os.environ, FPT_MANDEL_TILED_DISPATCH='1', FPT_MANDEL_TILE_ROWS='8')
        return subprocess.run(command, env=env, check=False).returncode
    except (ValueError, OSError, KeyError) as error:
        parser.exit(2, f'{error}\n')


if __name__ == '__main__':
    raise SystemExit(main())
