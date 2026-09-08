#!/usr/bin/env python3
"""Fresh, serial Mandel/FPT captures; successful execution is not visual parity."""
import argparse
import hashlib
import json
import math
import os
import re
import subprocess
import time
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw

ROOT = Path(__file__).resolve().parents[1]


def sha256(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def scene_dimensions(text, maximum):
    def value(key, default):
        match = re.search(r'^\s*' + key + r'\s+([^;]*)\s*;', text, re.M)
        if not match:
            return default
        if not match[1].strip().isdigit() or int(match[1]) <= 0:
            raise ValueError('invalid ' + key)
        return int(match[1])
    # Mandelbulber src/initparameters.cpp; .fract stores only modified values.
    width, height = value('image_width', 800), value('image_height', 600)
    scale = maximum / max(width, height)
    return max(1, round(width * scale)), max(1, round(height * scale))


def validate_manifest(manifest, scene_root):
    if manifest.get('version') != 1 or not manifest.get('scenes'):
        raise ValueError('expected nonempty version-1 scene manifest')
    root = scene_root.resolve()
    seen = set()
    for row in manifest['scenes']:
        if not re.fullmatch(r'[a-zA-Z0-9_-]+', row['id']) or row['id'] in seen:
            raise ValueError('invalid/duplicate scene id')
        seen.add(row['id'])
        path = (root / row['path']).resolve()
        if not path.is_relative_to(root) or not path.is_file():
            raise ValueError('scene path missing or outside root: ' + str(path))
        if sha256(path) != row['sha256']:
            raise ValueError('source hash mismatch: ' + str(path))


def execute(command, folder, timeout):
    folder.mkdir(parents=True, exist_ok=True)
    (folder / 'command.json').write_text(json.dumps(command, indent=2))
    start = time.monotonic()
    with (folder / 'stdout.log').open('w') as stdout, (folder / 'stderr.log').open('w') as stderr:
        result = subprocess.run(command, stdout=stdout, stderr=stderr, timeout=timeout)
    elapsed = time.monotonic() - start
    if result.returncode:
        raise RuntimeError(f'command exited {result.returncode}: {folder}/stderr.log')
    return elapsed


def image_result(folder, expected):
    images = list(folder.glob('*.png'))
    if len(images) != 1:
        raise ValueError(f'expected one fresh PNG in {folder}, found {len(images)}')
    with Image.open(images[0]) as image:
        if image.size != expected:
            raise ValueError(f'image size {image.size} != {expected}')
        pixels = image.convert('RGB').tobytes()
    return dict(path=str(images[0]), sha256=sha256(images[0]),
                rgb_sha256=hashlib.sha256(pixels).hexdigest())


def difference(reference, candidate):
    with Image.open(reference) as a, Image.open(candidate) as b:
        if a.size != b.size:
            raise ValueError('comparison dimensions differ')
        diff = ImageChops.difference(a.convert('RGB'), b.convert('RGB'))
        values = diff.tobytes()
        return dict(mae=sum(values)/len(values)/255,
                    rmse=math.sqrt(sum(v*v for v in values)/len(values))/255,
                    max_error=max(values)/255,
                    changed_pixels=sum(any(p) for p in diff.getdata()))


def contact_sheet(rows, output, maximum):
    columns = [('mandel', 'Mandelbulber authored'), ('geometry', 'FPT geometry'),
               ('authored', 'FPT authored path')]
    canvas = Image.new('RGB', (maximum*3, 40 + len(rows)*(maximum+42)), '#202326')
    draw = ImageDraw.Draw(canvas)
    for col, (_, title) in enumerate(columns):
        draw.text((col*maximum+8, 12), title, fill='white')
    for index, row in enumerate(rows):
        top = 40 + index*(maximum+42)
        for col, (key, _) in enumerate(columns):
            capture = row.get('captures', {}).get(key)
            if capture:
                with Image.open(capture['path']) as source:
                    image = source.convert('RGB')
                    canvas.paste(image, (col*maximum+(maximum-image.width)//2,
                                        top+(maximum-image.height)//2))
            else:
                draw.text((col*maximum+10, top+maximum//2), 'NOT CAPTURED', fill='#ffaaaa')
        draw.text((8, top+maximum+8), row['id']+' '+Path(row['path']).stem, fill='white')
        if row['status'] != 'ok':
            draw.text((8, top+maximum+23), 'FAILED: see summary/logs', fill='#ffaaaa')
    canvas.save(output / 'contact-sheet.png')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', type=Path, default=ROOT/'tests/fixtures/mandel-release-canaries.json')
    parser.add_argument('--scene-root', type=Path, required=True)
    parser.add_argument('--mandelbulber-root', type=Path, required=True)
    parser.add_argument('--mandelbulber-bin', type=Path, required=True)
    parser.add_argument('--fpt', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--max-axis', type=int, default=300)
    parser.add_argument('--samples', type=int, default=32)
    parser.add_argument('--chunk-samples', type=int, default=1)
    parser.add_argument('--timeout', type=int, default=600)
    parser.add_argument('--baseline', type=Path, help='prior summary for byte-exact FPT regression checks')
    args = parser.parse_args()
    if not 16 <= args.max_axis <= 2048 or not 1 <= args.samples <= 512 or args.timeout <= 0 or not 1 <= args.chunk_samples <= 64:
        parser.error('invalid dimensions, samples or timeout')
    manifest = json.loads(args.manifest.read_text())
    validate_manifest(manifest, args.scene_root)
    for path in (args.fpt, args.mandelbulber_bin):
        if not path.is_file():
            parser.error('missing executable: '+str(path))
    output = args.output.resolve()
    if output.exists() and any(output.iterdir()):
        parser.error('output must be empty; captures are never silently reused')
    output.mkdir(parents=True, exist_ok=True)
    baseline = json.loads(args.baseline.read_text()) if args.baseline else None
    settings = dict(max_axis=args.max_axis, samples=args.samples, aspect='authored',
                    chunk_samples=args.chunk_samples,
                    orientation='native output; no post-render flip',
                    reference='fresh Mandelbulber CPU authored render',
                    bounces='FPT scene/config default; see raw render metadata')
    if baseline and (baseline['settings'] != settings or baseline['manifest'] != manifest):
        parser.error('baseline scene hashes/settings differ')
    summary = dict(settings=settings, manifest=manifest, captures_fresh=True,
                   visual_parity_certified=False, rows=[],
                   fpt_environment={k:v for k,v in os.environ.items() if k.startswith('FPT_')},
                   executables={str(p.resolve()): sha256(p) for p in (args.fpt,args.mandelbulber_bin)})
    for scene in manifest['scenes']:
        row = dict(scene, status='running', captures={})
        try:
            path = (args.scene_root / scene['path']).resolve()
            size = scene_dimensions(path.read_text(), args.max_axis)
            row['size'] = size
            common = [str(path), '--mandelbulber-root', str(args.mandelbulber_root.resolve()),
                      '--width', str(size[0]), '--height', str(size[1]), '--samples', str(args.samples),
                      '--sdf-accumulation', 'chunked', '--sdf-chunk-samples', str(args.chunk_samples)]
            for mode in ('mandel', 'geometry', 'authored'):
                folder = output / scene['id'] / mode
                if mode == 'mandel':
                    command = [str(args.mandelbulber_bin.resolve()), '-n', '-C', '-f', 'png',
                               '-r', f'{size[0]}x{size[1]}', '-o', str(folder/'scene.png'), str(path)]
                else:
                    command = [str(args.fpt.resolve()), 'render', *common, '--out', str(folder),
                               '--mandel-appearance', 'geometry' if mode=='geometry' else 'authored-path']
                elapsed = execute(command, folder, args.timeout)
                row['captures'][mode] = dict(image_result(folder, size), wall_seconds=elapsed)
                print(f'{scene["id"]} {mode}: {elapsed:.2f}s', flush=True)
            row['appearance_difference'] = difference(row['captures']['mandel']['path'], row['captures']['authored']['path'])
            if baseline:
                previous = next(r for r in baseline['rows'] if r['id']==scene['id'])
                row['checkpoint_exact'] = all(row['captures'][m]['rgb_sha256']==previous['captures'][m]['rgb_sha256'] for m in ('geometry','authored'))
                if not row['checkpoint_exact']:
                    raise ValueError('FPT changed pixels against checkpoint')
            row['status'] = 'ok'
        except Exception as error:
            row.update(status='failed', error=str(error))
            print(scene['id']+' FAILED: '+str(error), flush=True)
        summary['rows'].append(row)
        summary['successful_scenes'] = sum(r['status']=='ok' for r in summary['rows'])
        (output/'summary.json').write_text(json.dumps(summary, indent=2))
    contact_sheet(summary['rows'], output, args.max_axis)
    print(output/'contact-sheet.png', flush=True)
    return int(any(r['status']!='ok' for r in summary['rows']))


if __name__ == '__main__':
    raise SystemExit(main())
