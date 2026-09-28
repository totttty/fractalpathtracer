#!/usr/bin/env python3
"""Render source-pinned catalogue controls through the production Rust library."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess


def main():
    p=argparse.ArgumentParser(description=__doc__)
    for key in ('worker','catalog','mandel-root','output'):
        p.add_argument('--'+key,type=Path,required=True)
    p.add_argument('--ids',nargs='+',required=True)
    a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
    rows={r['id']:r for r in json.loads(a.catalog.read_text())['scenes']}
    env={k:v for k,v in os.environ.items() if not k.startswith('FPT_')}
    env.update(FPT_MANDEL_TILED_DISPATCH='1',FPT_MANDEL_TILE_ROWS='8')
    report=dict(worker_sha256=hashlib.file_digest(a.worker.open('rb'),'sha256').hexdigest(),
                maximum_axis=300,samples=32,bounces=4,rows=[])
    for sid in a.ids:
        row=rows[sid];root=a.output/sid;root.mkdir()
        source=a.mandel_root/'deploy/share/mandelbulber2/examples'/row['path']
        request=dict(source=dict(path=str(source.resolve()),sha256=row['sha256'],
                     mandelbulber_root=str(a.mandel_root.resolve())),
                     output_directory=str((root/'image').resolve()),maximum_axis=300,samples=32,
                     bounces=4,chunk_samples=1,appearance='authored')
        req=root/'request.json';req.write_text(json.dumps(request,indent=2))
        receipt=root/'receipt.json'
        with (root/'log').open('w') as log:
            subprocess.run([str(a.worker.resolve()),'render',str(req),str(receipt)],env=env,
                           stdout=log,stderr=subprocess.STDOUT,check=True,timeout=300)
        report['rows'].append(dict(id=sid,source_sha256=row['sha256'],receipt=json.loads(receipt.read_text())))
        (a.output/'report.json').write_text(json.dumps(report,indent=2))
        print(sid,'rendered',flush=True)


if __name__=='__main__':main()
