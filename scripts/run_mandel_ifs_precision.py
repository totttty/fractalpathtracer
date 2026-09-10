#!/usr/bin/env python3
"""Scene-567-only precision diagnosis. Never modifies the production renderer."""
import argparse
import json
import math
import shutil
from pathlib import Path
import struct
import subprocess
import time

import numpy as np
from PIL import Image
from run_metal_precision_gate import split3
from run_release_canaries import sha256

ROOT = Path(__file__).resolve().parents[1]
SUPPORT = ROOT / 'examples/precision'
SCENE_SHA = '170ca7e9d8e4f6e404de61df2c978532b738a024ad99c442b528bccf168ddf7f'
FORMULA_SHA = '6ce96be608cae387e38f040ee0724dac057e8ca519f536c5f4e00789d669552c'


def scalar(value):
    return 'R(' + ','.join(f'{v:.17e}f' for v in split3(value)) + ')'


def vector(values):
    if len(values) != 3:
        raise ValueError('expected XYZ')
    return 'V(' + ','.join(map(scalar, values)) + ')'


def matrix(values):
    if len(values) != 9:
        raise ValueError('expected row-major matrix')
    return 'M{' + ','.join(vector(values[i:i+3]) for i in (0, 3, 6)) + '}'


def config_values(path):
    result = {}
    for line in path.read_text().splitlines():
        name, *raw = line.split()
        if name in result:
            raise ValueError('duplicate native parameter')
        values = list(map(float, raw))
        if not values or not all(map(math.isfinite, values)):
            raise ValueError('invalid native parameter')
        result[name] = values
    if result['bailout'] != [100, 0] or result['controls'][:2] != [250, 1]:
        raise ValueError('unsupported orbit controls')
    return result


def import_formula(path, cfg):
    if sha256(path) != FORMULA_SHA:
        raise ValueError('unsupported upstream formula revision')
    source = path.read_text()
    marker = 'void cFractalKaleidoscopicIfs::FormulaCode('
    body = source[source.index('{', source.index(marker)):].strip()
    if not body.endswith('}'):
        raise ValueError('unsupported formula body')
    declarations = []
    for name, value in zip(('absX', 'absY', 'absZ', 'rotationEnabled', 'edgeEnabled', 'mengerSpongeMode'), cfg['flags']):
        if value not in (0, 1):
            raise ValueError('invalid flag')
        declarations.append(f'bool {name} = {str(bool(value)).lower()};')
    for name in ('offset', 'edge'):
        declarations.append(f'V {name} = {vector(cfg[name])};')
    declarations += [f'R scale = {scalar(cfg["scale"][0])};', f'M mainRot = {matrix(cfg["mainRot"])};']
    for type_, name, values in (
        ('bool', 'enabled', [str(bool(cfg[f'plane{i}'][0])).lower() for i in range(9)]),
        ('R', 'distance', [scalar(cfg[f'plane{i}'][1]) for i in range(9)]),
        ('R', 'intensity', [scalar(cfg[f'plane{i}'][2]) for i in range(9)]),
        ('V', 'direction', [vector(cfg[f'plane{i}Direction']) for i in range(9)]),
        ('M', 'rot', [matrix(cfg[f'plane{i}Rot']) for i in range(9)]),
    ):
        declarations.append(f'{type_} {name}[9] = ' + '{' + ','.join(values) + '};')
    body = body.replace('fractal->IFS.', '').replace('IFS_VECTOR_COUNT', '9').replace('double length', 'R length')
    if 'fractal->' in body:
        raise ValueError('unmapped formula field')
    notice = source.split('#include', 1)[0]
    return notice + '\nvoid formula(thread V &z, thread Aux &aux) {\n' + '\n'.join(declarations) + body[1:]


def field_source(formula, cfg, arithmetic='three'):
    # Share the already-tested expansion/vector implementation; do not alter the
    # separately scene-pinned RoadToExascale reference.
    template = (SUPPORT / 'Reference.metal').read_text()
    prefix = template.split('@FORMULA@', 1)[0]
    operators = '''
R fabs(R a) { return absolute(a); }
void operator-=(thread V &a, V b) { a = a-b; }
void operator*=(thread V &a, R b) { a = a*b; }
void operator-=(thread R &a, R b) { a = a-b; }
void operator*=(thread R &a, R b) { a = a*b; }
struct M {
    V a,b,c;
    V RotateVector(V z) { return V(a.Dot(z),b.Dot(z),c.Dot(z)); }
};
'''
    field = '''
R field(V z) {
    Aux aux = {R(1), R(1), 0};
    R radius = z.Length();
    for (int i=0; i<250; ++i) {
        aux.i=i;
        formula(z,aux);
        radius=z.Length();
        if (radius>R(100)) break;
    }
    return minimum(maximum((radius-R(2))/aux.DE,R(0)),R(10));
}
float2 pack(R a) { return float2(a.hi,a.mid+a.lo); }
'''
    arithmetic_path = SUPPORT / ('Expansion.metal' if arithmetic == 'three' else 'TwoTerm.metal')
    return '// Diagnostic generated from external GPL formula; not a release source.\n' + arithmetic_path.read_text() + prefix + operators + formula + field


def probe_kernel():
    return '''
kernel void probe(device const float *p [[buffer(0)]], device float *o [[buffer(1)]], uint i [[thread_position_in_grid]]) {
    V pos(R(p[i*9],p[i*9+1],p[i*9+2]),R(p[i*9+3],p[i*9+4],p[i*9+5]),R(p[i*9+6],p[i*9+7],p[i*9+8]));
    float2 d=pack(field(pos));
    for(int j=0;j<8;++j) o[i*8+j]=0;
    o[i*8]=d.x; o[i*8+1]=d.y;
}
'''


def render_kernel(cfg, height):
    template = (SUPPORT / 'Reference.metal').read_text().split('kernel void probe', 1)[1]
    template = 'kernel void probe' + template
    n, min_n, steps, factor, detail, fov, view_max = cfg['controls']
    substitutions = {'@ORIGIN@': vector(cfg['camera']), '@MIN_THRESHOLD@': scalar(1e-12),
        '@MAX_STEPS@': str(int(steps)), '@THRESHOLD_SCALE@': scalar(height*detail/fov),
        '@DE_FACTOR@': scalar(factor), '@VIEW_MAX@': scalar(view_max),
        '@REFINE_RATIO@': scalar(.998), '@NORMAL_SCALE@': scalar(.1)}
    for key, value in substitutions.items():
        template = template.replace(key, value)
    if '@' in template:
        raise ValueError('unexpanded shader')
    return template


def execute(command, out, label, timeout=900):
    start = time.monotonic()
    result = subprocess.run(list(map(str, command)), capture_output=True, text=True, timeout=timeout)
    (out / f'{label}.json').write_text(json.dumps(dict(command=list(map(str, command)),
        returncode=result.returncode, wall_s=time.monotonic()-start), indent=2)+'\n')
    (out / f'{label}.stdout').write_text(result.stdout)
    (out / f'{label}.stderr').write_text(result.stderr)
    if result.returncode:
        raise RuntimeError(f'{label} failed: {result.stderr[-4000:]}')
    return result.stdout


def pack_inputs(points):
    return b''.join(struct.pack('<9f', *(term for v in xyz for term in split3(v))) for xyz in points)


def round_config(cfg, mode):
    result = {key:list(values) for key,values in cfg.items()}
    if mode not in ('none','formula','camera'):
        raise ValueError('unknown precision ablation')
    keys = ['camera'] if mode == 'camera' else (
        [key for key in cfg if key not in ('camera','target','top','controls','bailout')] if mode == 'formula' else [])
    for key in keys:
        result[key] = [float(np.float32(v)) for v in result[key]]
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--scene', type=Path, required=True)
    parser.add_argument('--mandelbulber-root', type=Path, required=True)
    parser.add_argument('--native-probe', type=Path)
    parser.add_argument('--fpt-inputs', type=Path, help='Input directory from mandel_ifs_precision_inputs')
    parser.add_argument('--reference-report', type=Path, help='Existing native-validated point report; avoids native process')
    parser.add_argument('--arithmetic', choices=('three','two'), default='three')
    parser.add_argument('--round-data', choices=('none','formula','camera'), default='none',
                        help='Diagnostic only: round selected source values to float32, keeping arithmetic unchanged')
    parser.add_argument('--points', type=Path, required=True, help='Native XYZ + threshold TSV')
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--size', help='Optional diagnostic render, maximum 320x240')
    parser.add_argument('--render-repeats', type=int, choices=range(1,6), default=1)
    parser.add_argument('--pixel-sampling', choices=('center', 'native'), default='center',
                        help='Native CPU samples integer image coordinates; center matches FPT convention')
    args = parser.parse_args()
    formula_path = args.mandelbulber_root / 'formula/definition/fractal_kaleidoscopic_ifs.cpp'
    if sha256(args.scene) != SCENE_SHA or sha256(formula_path) != FORMULA_SHA:
        raise ValueError('only the pinned scene567 and upstream formula are supported')
    size = tuple(map(int, args.size.split('x'))) if args.size else None
    if size and (len(size) != 2 or not 0 < size[0] <= 320 or not 0 < size[1] <= 240):
        raise ValueError('invalid diagnostic size')
    points = np.loadtxt(args.points, ndmin=2)
    if points.shape[1] != 4 or not np.isfinite(points).all() or not (points[:,3] > 0).all():
        raise ValueError('invalid point input')
    if not args.fpt_inputs and not args.native_probe:
        raise ValueError('native probe or FPT inputs required')
    if not args.reference_report and not args.native_probe:
        raise ValueError('native probe or validated distance reference required')
    input_report = None
    if args.fpt_inputs:
        input_report=json.loads((args.fpt_inputs/'summary.json').read_text())
        if input_report['scene_sha256'] != SCENE_SHA or any(
            sha256(args.fpt_inputs/name) != input_report[key]
            for name,key in [('config.tsv','config_sha256'),('rays.tsv','rays_sha256')]):
            raise ValueError('FPT input identity mismatch')
        if size and ((input_report['width'],input_report['height']) != size or input_report['pixel_sampling'] != args.pixel_sampling):
            raise ValueError('FPT ray dimensions or sampling mismatch')
    reference_report=None
    if args.reference_report:
        reference_report=json.loads((args.reference_report/'summary.json').read_text())
        if reference_report['scene_sha256'] != SCENE_SHA or reference_report['input_sha256'] != sha256(args.points) or not reference_report['passed']:
            raise ValueError('distance reference identity mismatch')
    args.out.mkdir(parents=True, exist_ok=False)
    out = args.out.resolve()
    native = args.native_probe.resolve() if args.native_probe else None
    if args.fpt_inputs:
        shutil.copyfile(args.fpt_inputs/'config.tsv',out/'config.tsv')
    else:
        execute([native, args.scene, args.points, out/'config.tsv', 'ifs10-config'], out, 'config')
    cfg = round_config(config_values(out/'config.tsv'),args.round_data)
    (out/'effective-config.json').write_text(json.dumps(cfg,indent=2)+'\n')
    formula = import_formula(formula_path, cfg)
    shared = field_source(formula, cfg, args.arithmetic)
    (out/'fields.metal').write_text(shared + probe_kernel())
    (out/'points.bin').write_bytes(pack_inputs(points[:,:3]))
    execute(['clang++','-std=c++17','-O2','-fobjc-arc',SUPPORT/'probe.mm',
        '-framework','Foundation','-framework','Metal','-o',out/'probe'], out, 'compile-driver')
    pipelines = []
    for label in ('first','repeat'):
        pipelines.append(json.loads(execute([out/'probe',out/'fields.metal',out/'points.bin',out/f'{label}.bin'], out, label)))
    if reference_report:
        expected=np.array([row['native'] for row in reference_report['samples']],dtype='float64')
    else:
        execute([native,args.scene,args.points,out/'native.tsv'],out,'native-distance')
        expected = np.loadtxt(out/'native.tsv',ndmin=2)[:,0]
    records = np.fromfile(out/'first.bin',dtype='<f4').reshape(-1,8)
    actual = records[:,0].astype('float64')+records[:,1].astype('float64')
    if len(actual) != len(expected) or not np.isfinite(actual).all():
        raise ValueError('invalid GPU field output')
    absolute = np.abs(actual-expected)
    relative = absolute/np.maximum(np.abs(expected),1e-30)
    repeat = (out/'first.bin').read_bytes() == (out/'repeat.bin').read_bytes()
    passed = bool(np.all(absolute <= 1e-12 + np.abs(expected)*1e-5) and repeat)
    report = dict(scope='Scene567 diagnostic only; not production support',
        input_provider='FPT Rust' if args.fpt_inputs else 'native adapter', arithmetic=args.arithmetic,
        round_data=args.round_data,effective_config_sha256=sha256(out/'effective-config.json'),
        fpt_input_report=input_report,
        reference_report_sha256=sha256(args.reference_report/'summary.json') if args.reference_report else None,
        passed=passed, repeat_byte_exact=repeat, points=len(points), max_absolute_error=float(absolute.max()),
        median_relative_error=float(np.median(relative)), max_relative_error=float(relative.max()),
        samples=[dict(xyz=list(p[:3]),native=float(n),metal=float(m),relative_error=float(r))
            for p,n,m,r in zip(points,expected,actual,relative)], field_pipelines=pipelines,
        scene_sha256=sha256(args.scene), formula_sha256=sha256(formula_path),
        native_probe_sha256=sha256(native) if native else None, input_sha256=sha256(args.points),
        script_sha256=sha256(Path(__file__)), field_shader_sha256=sha256(out/'fields.metal'),
        precision_shader_sha256=sha256(SUPPORT/('Expansion.metal' if args.arithmetic=='three' else 'TwoTerm.metal')))
    (out/'summary.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({k:report[k] for k in ('passed','points','max_absolute_error','median_relative_error','max_relative_error')},indent=2),flush=True)
    if not passed:
        raise RuntimeError('field gate failed; rendering skipped')
    if size:
        width,height=size
        offset = .5 if args.pixel_sampling == 'center' else 0.
        xy=[((x+offset-width/2)/height, (height/2-y-offset)/height, 0, 1e-12)
            for y in range(height) for x in range(width)]
        (out/'ray-plane.tsv').write_text(''.join('\t'.join(map(str,row))+'\n' for row in xy))
        if args.fpt_inputs:
            shutil.copyfile(args.fpt_inputs/'rays.tsv',out/'rays.tsv')
        else:
            execute([native,args.scene,out/'ray-plane.tsv',out/'rays.tsv','rays'],out,'native-rays')
        rays=np.loadtxt(out/'rays.tsv',ndmin=2)
        (out/'rays.bin').write_bytes(pack_inputs(rays))
        (out/'render.metal').write_text(shared+render_kernel(cfg,height))
        pipeline=json.loads(execute([out/'probe',out/'render.metal',out/'rays.bin',out/'render.bin'],out,'render',1800))
        render_pipelines=[pipeline]
        render_repeat_exact=True
        for repeat_index in range(1,args.render_repeats):
            destination=out/f'render-repeat-{repeat_index}.bin'
            render_pipelines.append(json.loads(execute([out/'probe',out/'render.metal',out/'rays.bin',destination],out,f'render-repeat-{repeat_index}',1800)))
            render_repeat_exact &= destination.read_bytes() == (out/'render.bin').read_bytes()
        raw=np.fromfile(out/'render.bin',dtype='<f4').reshape(height,width,8)
        if not np.isfinite(raw).all():
            raise ValueError('nonfinite render')
        hits=raw[:,:,2]>.5
        depth=raw[:,:,0].astype('float64')+raw[:,:,1].astype('float64')
        shade=np.rint(np.clip(raw[:,:,4].astype('float64')+raw[:,:,5],0,1)*255).astype('uint8')
        rgb=np.repeat(shade[:,:,None],3,axis=2)
        rgb[~hits]=[0,0,0]
        Image.fromarray(rgb).save(out/'render.png')
        depth.astype('<f8').tofile(out/'depth.f64')
        Image.fromarray(hits.astype('uint8')*255).save(out/'mask.png')
        report['render']=dict(width=width,height=height,pixel_sampling=args.pixel_sampling,
            hits=int(hits.sum()),coverage_pct=float(hits.mean()*100),
            stalls=int((raw[:,:,6]>.5).sum()),max_step_exits=int((raw[:,:,3]>=cfg['controls'][2]).sum()),pipeline=pipeline,
            render_pipelines=render_pipelines,repeat_byte_exact=render_repeat_exact if args.render_repeats>1 else None,
            shader_sha256=sha256(out/'render.metal'), image_sha256=sha256(out/'render.png'),
            input_provider=report['input_provider'],arithmetic=args.arithmetic,
            limitations='One ray per pixel; white headlight; pinned diagnostic, not production support or a performance gate')
        (out/'summary.json').write_text(json.dumps(report,indent=2)+'\n')
        print(json.dumps(report['render'],indent=2))
        if report['render']['stalls'] or report['render']['max_step_exits'] or not render_repeat_exact:
            raise RuntimeError('precision render incomplete; inspect recorded exits')


if __name__ == '__main__':
    main()
