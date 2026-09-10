#!/usr/bin/env python3
"""Publish a hash-checked ranked-50 capture report as a portable GitHub gallery."""
import argparse
import copy
import json
from pathlib import Path
import re
from urllib.parse import quote

from PIL import Image

from refresh_mandel_merge_sheets import SCENE_NOTES, sheets
from run_release_canaries import sha256


MODES = ('mandel', 'geometry', 'authored')


def validate_report(report):
    rows = report['rows']
    if [r['id'] for r in rows] != [f'{i:02d}' for i in range(1, 51)]:
        raise ValueError('gallery requires exactly the ordered scenes 01-50')
    if report['samples'] != 32 or report['max_axis'] != 300:
        raise ValueError('gallery contract is 300px max axis, 32 SPP')
    for row in rows:
        if max(row['size']) != 300 or min(row['size']) < 1:
            raise ValueError(f"{row['id']}: invalid render dimensions")
        for mode in MODES:
            result = row['modes'][mode]
            if result.get('status') != 'ok':
                raise ValueError(f"{row['id']} {mode}: capture incomplete")
            asset = result['capture']
            if sha256(Path(asset['path'])) != asset['sha256']:
                raise ValueError(f"{row['id']} {mode}: image hash mismatch")
            with Image.open(asset['path']) as image:
                expected = tuple(row['size'])
                if mode == 'mandel' and row['reference_reduced']:
                    if max(image.size) != 96:
                        raise ValueError('reduced native reference must have a 96px max axis')
                elif image.size != expected:
                    raise ValueError(f"{row['id']} {mode}: dimensions mismatch")
            if mode != 'mandel':
                metadata = result['metadata']
                if sha256(Path(metadata['path'])) != metadata['sha256']:
                    raise ValueError(f"{row['id']} {mode}: metadata hash mismatch")
                if result['pipeline']['samples'] != 32:
                    raise ValueError('render sample count differs from gallery contract')


def publish(report, output, upstream_revision, renderer_revision):
    validate_report(report)
    if not re.fullmatch(r'[0-9a-f]{40}', upstream_revision):
        raise ValueError('upstream revision must be a full Git commit hash')
    if not re.fullmatch(r'[0-9a-f]{40}', renderer_revision):
        raise ValueError('renderer revision must be a full Git commit hash')
    # Reviewed annotations are separate from the immutable capture evidence.
    report = copy.deepcopy(report)
    for row in report['rows']:
        if row['id'] in SCENE_NOTES:
            row['note'] = SCENE_NOTES[row['id']]
    output.mkdir(parents=True, exist_ok=False)
    sheets(report, output)
    for path in output.glob('*.png'):
        with Image.open(path) as image:
            image.save(path, optimize=True)
    source_prefix = (
        'https://github.com/buddhi1980/mandelbulber2/blob/'
        + upstream_revision + '/mandelbulber2/deploy/share/mandelbulber2/examples/'
    )
    manifest = {
        'version': 1,
        'scope': 'Continuous FPT Metal; not NAADF or voxel captures.',
        'production_sha256': report['production_sha256'],
        'renderer_revision': renderer_revision,
        'samples': report['samples'],
        'max_axis': report['max_axis'],
        'bounces': report['bounces'],
        'environment': report['environment'],
        'upstream_revision': upstream_revision,
        'native_references': 'Cached CPU renders; source and image hashes checked, not freshly rendered.',
        'visual_parity_certified': False,
        'rows': [],
        'sheets': {p.name: sha256(p) for p in sorted(output.glob('*.png'))},
    }
    for row in report['rows']:
        manifest['rows'].append({
            'id': row['id'], 'source': row['path'], 'source_sha256': row['sha256'],
            'source_url': source_prefix + quote(row['path'], safe='/'),
            'dimensions': row['size'], 'native_reference_reduced': row['reference_reduced'],
            'note': row.get('note', ''),
            'captures': {mode: {
                'sha256': row['modes'][mode]['capture']['sha256'],
                'rgb_sha256': row['modes'][mode]['capture']['rgb_sha256'],
            } for mode in MODES},
        })
    (output/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
    lines = [
        '# Mandelbulber Ranked-50 Review Gallery', '',
        '[Back to FPT Metal](../../README.md)', '',
        '**All 50 scenes rendered in both FPT modes. This is a review gallery, '
        'not a claim of full Mandelbulber parity.**', '',
        'These are continuous procedural **FPT Metal** renders, not NAADF voxel images. '
        'FPT captures were regenerated after the point, generated-light and orbit-trap '
        'surface-lighting fixes, at **300 pixels on the longest edge, 32 SPP**, '
        'with authored aspect ratio and unchanged default bounce settings. '
        'No rejected analytic-derivative or offline precision prototype is substituted.', '',
        'Each detailed row shows **cached Mandelbulber CPU reference / FPT neutral '
        'geometry / FPT authored path tracing**. Neutral geometry intentionally uses '
        'different material/lighting controls; it is not an appearance-parity target.', '',
        'Native references are hash-verified earlier captures, not a new native render '
        'run. References for **09, 17 and 49** have only 96 pixels on their longest '
        'edge and are explicitly enlarged. The other 47 references match the FPT '
        'dimensions. Native sampling/lighting are not identical to FPT.', '',
        '## Known Limitations', '',
        '- **46:** unresolved float32 position stalls; geometry is not certified.',
        '- **48:** major deep-zoom geometry failure in the production renderer. '
        'Included for transparency, not as a supported showcase.',
        '- **07 and 40:** restored surface lighting does not implement clouds '
        'or volumetric light halos.',
        '- **08:** generated lights restore illumination, but native light '
        'placement/colour is not certified byte-exact.',
        '- **42:** normal/lighting residual remains; the rejected derivative '
        'experiment is not enabled.',
        '- **32 and 37:** substantial authored illumination gaps remain; '
        'these are still darker than the native references.',
        '- Palettes, exposure, materials, sky and indirect illumination can '
        'still differ. These images are not new performance benchmarks.', '',
        '## Overview', '',
        'Each pair: native reference on the left, current FPT authored on the right.', '',
        '![All 50 native and current FPT authored render pairs](overview.png)', '',
        '## Detailed Comparisons', '',
    ]
    for start in range(1, 51, 10):
        end = start+9
        lines.extend([
            f'### Scenes {start:02d}-{end:02d}', '',
            f'![Scenes {start:02d}-{end:02d}: native reference, FPT geometry, '
            f'FPT authored](scenes-{start:02d}-{end:02d}.png)', '',
        ])
    lines.extend([
        '## Sources and Capture Identity', '',
        'Scene names and collection credits are retained below; links point to '
        'the source examples. This gallery does not bundle the source `.fract` '
        'files, generated formula code, metallibs, or voxel volumes. Source '
        'collection licence labels are preserved, not replaced by this repository\'s licence.', '',
        '| Scene | Source | Collection credit |',
        '| --- | --- | --- |',
    ])
    for row in manifest['rows']:
        source = Path(row['source'])
        credit = source.parent.as_posix() if source.parent != Path('.') else 'Mandelbulber example collection'
        lines.append(f"| {row['id']} | [{source.stem}]({row['source_url']}) | {credit} |")
    lines.extend([
        '', '[Capture hashes, dimensions and per-scene notes](manifest.json).', '',
        'The manifest intentionally contains no machine-local absolute paths. '
        'Original PNG pixels and metadata were hash-checked before sheet generation. '
        'Detailed FPT tiles retain the captured pixels. Overview thumbnails are '
        'downsampled, and the three small native references are enlarged as labelled. '
        'Sheets use lossless PNG compression; no colour correction, cropping or '
        'orientation changes are applied.', '',
        'For reproduction, use `scripts/refresh_mandel_merge_sheets.py` with '
        'the ranked-50 audit and native retry reports, then '
        '`scripts/publish_mandel_gallery.py`. Supply an external Mandelbulber '
        'checkout; local raw reports remain ignored by Git.', '',
    ])
    (output/'README.md').write_text('\n'.join(lines))
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--upstream-revision', required=True)
    parser.add_argument('--renderer-revision', required=True,
                        help='Commit containing the renderer source used by the capture binary.')
    args = parser.parse_args()
    report = json.loads(args.report.read_text())
    manifest = publish(report, args.output, args.upstream_revision, args.renderer_revision)
    print(f"Published {len(manifest['rows'])} scenes and {len(manifest['sheets'])} sheets")


if __name__ == '__main__':
    main()
