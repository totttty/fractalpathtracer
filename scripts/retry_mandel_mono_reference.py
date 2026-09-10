#!/usr/bin/env python3
"""Derive an explicitly monoscopic native control without modifying original audit evidence."""
import argparse
import copy
import json
from pathlib import Path

from run_mandel_support_suite import capture, classify, parameters, save, validate_resume
from run_release_canaries import mandel_reference_command, sha256, validate_manifest, verify_lightmap


def replace_reference(report, scene_id, result):
    derived=copy.deepcopy(report)
    row=next(r for r in derived['rows'] if r['id']==scene_id)
    result=copy.deepcopy(result)
    result['previous_reference']=row['modes']['mandel']
    result['reference_overrides']={'stereo_enabled':False}
    row['modes']['mandel']=result
    row['reference_overrides']={'stereo_enabled':False}
    classify(row)
    return derived


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report',type=Path,required=True)
    parser.add_argument('--scene-root',type=Path,required=True)
    parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--scene-id',required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--timeout',type=int,default=900)
    args=parser.parse_args()
    base_sha256=sha256(args.report)
    report=json.loads(args.report.read_text())
    identity=report['identity']
    if len(report['rows'])!=len(identity['manifest']['scenes']) or any(set(r['modes'])!=set(identity['settings']['modes']) for r in report['rows']):
        raise ValueError('finish the original capture audit before deriving a control')
    validate_manifest(identity['manifest'],args.scene_root)
    validate_resume(report,identity)
    binary=args.binary.resolve()
    if identity['executables'].get(str(binary))!=sha256(binary):
        raise ValueError('native binary differs from original audit')
    row=next(r for r in report['rows'] if r['id']==args.scene_id)
    source=(args.scene_root/row['path']).resolve()
    if parameters(source.read_text()).get('stereo_enabled')!='true':
        raise ValueError('this control is only for authored stereo scenes')
    lightmap=identity['lightmaps'][row['id']]
    verify_lightmap(lightmap)
    output=args.output.resolve()
    output.mkdir(parents=True,exist_ok=False)
    folder=output/row['id']/'mandel-mono'
    command=mandel_reference_command(binary,source,row['size'],folder/'scene.png',Path(lightmap['path']))
    command[command.index('-O')+1]+='#stereo_enabled=false'
    result=capture(command,folder,tuple(row['size']),args.timeout)
    verify_lightmap(lightmap)
    if sha256(source)!=row['sha256'] or sha256(binary)!=identity['executables'][str(binary)]:
        raise ValueError('source or native binary changed during control')
    if (folder/'stdout.log').exists() and 'opencl - rendering' in (folder/'stdout.log').read_text().lower():
        result=dict(status='invalid_reference_backend',error='OpenCL used despite CPU override')
    derived=replace_reference(report,row['id'],result)
    if sha256(args.report)!=base_sha256:
        raise ValueError('base report changed during control')
    original_artifacts={str(path.relative_to(args.report.parent)):sha256(path)
        for folder in (args.report.parent/row['id']).glob('mandel*') if folder.is_dir()
        for path in folder.rglob('*') if path.is_file()}
    derived['derivation']=dict(base_report=str(args.report.resolve()),base_report_sha256=base_sha256,
        tool_sha256=sha256(Path(__file__)),scope='Native monoscopic comparison control; FPT captures unchanged.',
        scene_id=row['id'],source_sha256=row['sha256'],reference_overrides={'stereo_enabled':False},
        original_reference_artifacts=original_artifacts)
    save(derived,output)
    print(json.dumps(dict(scene_id=row['id'],status=result['status'],counts=derived['counts'])))
    return int(result['status']!='ok')


if __name__=='__main__':
    raise SystemExit(main())
