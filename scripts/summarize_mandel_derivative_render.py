#!/usr/bin/env python3
"""Compare fresh diagnostic rays with native normals and the accepted FPT dump."""
import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw
from run_mandel_normal_controls import compare
from run_release_canaries import sha256


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference', type=Path, required=True)
    parser.add_argument('--renders', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    original_meta = json.loads((args.reference / 'fpt/hits.bin.json').read_text())
    width, height = original_meta['width'], original_meta['height']
    native = np.fromfile(args.reference / 'native/channels.bin', '<f4').reshape(height, width, 4)
    original = np.fromfile(args.reference / 'fpt/hits.bin', '<f4').reshape(height, width, 20)
    rows = {}
    arrays = {}
    paths = [args.reference / 'native/channels.bin', args.reference / 'fpt/hits.bin']
    views = []
    native_hits = np.isfinite(native[..., 0]) & (native[..., 0] > 0) & (native[..., 0] < 1e10)
    views.append(('Native Mandel normals', native[..., [1, 3, 2]], native_hits))
    for mode in ('baseline', 'analytic'):
        root = args.renders / mode
        meta = json.loads((root / 'summary.json').read_text())
        if (meta['width'], meta['height'], meta['world_scale']) != (width, height, original_meta['world_scale']):
            raise ValueError('render dimensions/scale differ')
        arrays[mode] = np.fromfile(root / 'hits.bin', '<f4').reshape(height, width, 20)
        metrics, nn, fn, angles, valid, close = compare(native, arrays[mode], meta['world_scale'])
        rows[mode] = metrics
        paths.extend([root / 'hits.bin', root / 'summary.json'])
        views.append((f'FPT {mode} normals', fn, arrays[mode][..., 7] > .5))
    baseline, candidate = arrays['baseline'], arrays['analytic']
    original_hits = original[..., 7] > .5
    base_hits, cand_hits = baseline[..., 7] > .5, candidate[..., 7] > .5
    report = {
        'scope': 'Fresh 1-SPP diagnostic rays, not an authored beauty render or a timing gate. Native signed normals mapped Y/Z, no image registration.',
        'size': [width, height], 'rows': rows,
        'baseline_vs_original_structural_byte_exact': baseline.tobytes() == original.tobytes(),
        'baseline_vs_original_changed_hit_pixels': int(np.sum(base_hits != original_hits)),
        'candidate_vs_baseline_changed_hit_pixels': int(np.sum(base_hits != cand_hits)),
        'candidate_vs_baseline_new_misses': int(np.sum(base_hits & ~cand_hits)),
        'candidate_vs_baseline_new_hits': int(np.sum(~base_hits & cand_hits)),
        'identity': {str(p.resolve()): sha256(p) for p in paths},
    }
    a, b = rows['analytic'], rows['baseline']
    checks = {
        'baseline_byte_exact': report['baseline_vs_original_structural_byte_exact'],
        'normal_median_under_one_degree': a['normal_degrees_all']['median'] <= 1.0,
        'normal_p95_improves': a['normal_degrees_all']['p95'] < b['normal_degrees_all']['p95'],
        'depth_median_improves': a['depth_relative']['median'] < b['depth_relative']['median'],
        'no_new_native_visible_misses': a['visible_misses'] <= b['visible_misses'],
    }
    report['diagnostic_gate'] = {'passed': all(checks.values()), 'checks': checks}
    tile_w, tile_h = 400, round(height * 400 / width)
    canvas = Image.new('RGB', (tile_w * 3, tile_h + 100), '#1b1d20')
    draw = ImageDraw.Draw(canvas)
    for i, (label, normal, hits) in enumerate(views):
        rgb = np.uint8(np.clip((normal + 1) * 127.5, 0, 255))
        rgb[~hits] = 0
        im = Image.fromarray(rgb)
        im.save(args.output / f'{i}-normal.png')
        canvas.paste(im.resize((tile_w, tile_h), Image.Resampling.NEAREST), (i * tile_w, 28))
        draw.text((i * tile_w + 8, 8), label, fill='white')
        if i:
            stats = rows[('baseline', 'analytic')[i-1]]['normal_degrees_all']
            draw.text((i * tile_w + 8, tile_h + 38), f"Normal error: median {stats['median']:.2f} deg | P95 {stats['p95']:.2f} deg", fill='white')
    draw.text((8, tile_h + 65), 'Scene 42 | fixed camera | original normal diagnostic | no alignment or material/exposure adjustment', fill='white')
    canvas.save(args.output / 'comparison.png')
    (args.output / 'summary.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({k: v for k, v in report.items() if k != 'identity'}, indent=2))
    if not report['diagnostic_gate']['passed']:
        raise SystemExit('fresh-render diagnostic gate failed; inspect summary')


if __name__ == '__main__':
    main()
