#!/usr/bin/env python3
"""Reproduce every new native-visible miss from the derivative camera sweep."""
import argparse
import json
import os
from pathlib import Path
import numpy as np
from run_release_canaries import execute, sha256


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--views', type=Path, required=True)
    p.add_argument('--probe', type=Path, required=True)
    p.add_argument('--mandel-root', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    args = p.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    results = {'probe_sha256':sha256(args.probe), 'views_sha256':sha256(args.views/'summary.json'), 'rows':[]}
    for row in json.loads((args.views/'summary.json').read_text())['rows']:
        folder = args.views.resolve()/row['view']
        meta = json.loads((folder/'analytic-normal/summary.json').read_text())
        width,height = meta['width'],meta['height']
        native = np.fromfile(folder/'native-normal/channels.bin','<f4').reshape(-1,4)
        baseline = np.fromfile(folder/'baseline-normal/hits.bin','<f4').reshape(-1,20)
        candidate = np.fromfile(folder/'analytic-normal/hits.bin','<f4').reshape(-1,20)
        hn = np.isfinite(native[:,0]) & (native[:,0] > 0) & (native[:,0] < 1e10)
        ids = np.flatnonzero(hn & (baseline[:,7] > .5) & (candidate[:,7] <= .5))
        if not len(ids): continue
        points = [[int(i%width),int(i//width),0,0] for i in ids]
        input = args.output.resolve()/f'{row["view"]}-pixels.json'; input.write_text(json.dumps(points))
        entry = {'view':row['view'],'pixel_indices':ids.tolist(),'native_depths':native[ids,0].tolist(),'probes':{}}
        for label in ['baseline','analytic']:
            out = args.output.resolve()/f'{row["view"]}-{label}'
            cmd = [str(args.probe.resolve()),str(folder/'scene.fract'),'--mandelbulber-root',str(args.mandel_root.resolve()),
                   '--width',str(width),'--height',str(height),'--samples','1','--sdf-accumulation','chunked',
                   '--sdf-chunk-samples','1','--mandel-appearance','authored-path','--sdf-bounce-cap','4','--mode','normal','--out',str(out)]
            execute(cmd,args.output/f'{row["view"]}-{label}-run',180,env=dict(os.environ,
                FPT_NORMAL_PROBE_ANALYTIC85='1' if label=='analytic' else '0', FPT_DERIVATIVE_RAY_PIXELS=str(input)))
            data = json.loads((out/'ray-exits.json').read_text())
            if any(item['found'] != (label=='baseline') for item in data['rows']):
                raise ValueError('ray probe did not reproduce saved hit classification')
            entry['probes'][label] = data
        results['rows'].append(entry)
    (args.output/'summary.json').write_text(json.dumps(results,indent=2)+'\n')
    print(json.dumps(results,indent=2),flush=True)


if __name__ == '__main__': main()
