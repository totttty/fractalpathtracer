#!/usr/bin/env python3
"""Isolate fractal and box displacement using native CPU headlight controls."""
import argparse
import json
from pathlib import Path
from PIL import Image, ImageDraw
from run_mandel_shadow_controls import replace_main_parameters
from run_mandel_geometry_controls import headlight_command
from run_mandel_support_suite import capture, parameters
from run_release_canaries import difference, sha256


def neutral_perlin_color_overrides(source):
    """Bypass native surface-colour modulation without enabling new geometry.

    Only already-enabled noise materials qualify. Zero colour intensity makes
    the native Perlin colour branch return white; displacement stays unchanged.
    This is an explicit diagnostic control, not an authored rendering setting.
    """
    values = parameters(source)
    result = {}
    for key, value in values.items():
        if key.startswith('mat') and key.endswith('_perlin_noise_enable') and value in ('true', '1'):
            prefix = key.removesuffix('_enable')
            result[prefix + '_color_enable'] = '1'
            result[prefix + '_color_intensity'] = '0'
    return result


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--audit',type=Path,required=True)
    p.add_argument('--binary',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True)
    p.add_argument('--neutral-perlin-color',action='store_true',
                   help='explicit zero-intensity Perlin white-colour control; retains displacement')
    args=p.parse_args()
    audit=json.loads((args.audit/'summary.json').read_text())
    row=next(r for r in audit['rows'] if r['id']=='380')
    native=json.loads((args.audit/'380/mandel/command.json').read_text())
    fpt=json.loads((args.audit/'380/geometry/command.json').read_text())
    source=Path(native[-1])
    if sha256(source)!=row['sha256']: raise ValueError('source changed')
    out=args.output.resolve();out.mkdir(parents=True,exist_ok=False)
    report=dict(source_sha256=sha256(source),size=row['size'],samples=32,
                binaries={str(args.binary.resolve()):sha256(args.binary),native[0]:sha256(Path(native[0]))},
                neutral_perlin_color=args.neutral_perlin_color,
                harness_sha256=sha256(Path(__file__)), rows=[])
    sheet=Image.new('RGB',(600,40+4*215),'#202326');d=ImageDraw.Draw(sheet)
    d.text((8,10),'Native CPU / FPT Metal | white diffuse, unchanged camera',fill='white')
    for index,(label,a,b) in enumerate([('neither',False,False),('fractal-only',True,False),('box-only',False,True),('both',True,True)]):
        folder=out/label;folder.mkdir()
        overrides={f'mat{i}_perlin_noise_displacement_enable':str(v).lower() for i,v in [(1,a),(2,b)]}
        derived=folder/'scene.fract';derived.write_text(replace_main_parameters(source.read_text(),overrides))
        n=native.copy();n[-1]=str(derived)
        n,neutral=headlight_command(n,derived.read_text(),folder/'native')
        color_control=neutral_perlin_color_overrides(derived.read_text()) if args.neutral_perlin_color else {}
        if color_control:
            n[n.index('-O')+1]+='#'+'#'.join(f'{key}={value}' for key,value in color_control.items())
        results={'native':capture(n,folder/'native',tuple(row['size']),300)}
        f=fpt.copy();f[0]=str(args.binary.resolve());f[2]=str(derived);f[f.index('--out')+1]=str(folder/'fpt')
        results['fpt']=capture(f,folder/'fpt',tuple(row['size']),300)
        for mode,result in results.items():
            if result['status']!='ok': raise RuntimeError((label,mode,result))
        if "doesn't exists" in (folder/'native/stderr.log').read_text(errors='replace'): raise RuntimeError('unrecognized native override')
        if 'OpenCl - rendering' in (folder/'native/stdout.log').read_text(errors='replace'): raise RuntimeError('expected native CPU')
        if sha256(source)!=report['source_sha256']: raise RuntimeError('source changed during capture')
        for binary,digest in report['binaries'].items():
            if sha256(Path(binary))!=digest: raise RuntimeError('binary changed during capture')
        delta=difference(results['native']['capture']['path'],results['fpt']['capture']['path'])
        report['rows'].append(dict(variant=label,overrides=overrides,neutral=neutral,color_control=color_control,derived_sha256=sha256(derived),results=results,rgb_diff_not_geometry_metric=delta))
        for col,mode in enumerate(['native','fpt']):
            with Image.open(results[mode]['capture']['path']) as im:sheet.paste(im.convert('RGB'),(col*300,40+index*215))
        d.text((8,40+index*215+178),f'{label}: RGB MAE {delta["mae"]:.5f}',fill='white')
        sheet.save(out/'comparison.png')
        (out/'summary.json').write_text(json.dumps(report,indent=2)+'\n')
        print(label,delta,flush=True)


if __name__=='__main__':main()
