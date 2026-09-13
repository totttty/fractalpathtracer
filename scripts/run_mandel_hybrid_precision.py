#!/usr/bin/env python3
"""Pinned scene-578 compensated-precision diagnostic, never production rendering."""
import argparse
import json
from pathlib import Path
import re

import numpy as np
from PIL import Image

from run_mandel_ifs_precision import SUPPORT, execute, pack_inputs, scalar
from run_mandel_shadow_controls import replace_main_parameters
from run_release_canaries import sha256

SCENE_SHA = 'ed6528d2335f47f90415a3d3874c0f93c78fd073f9cebbd56bbd502a7ab38a13'
FORMULAS = (
    ('fractal_menger_sponge.cpp', 'cFractalMengerSponge',
     '2600bae0c2aa9ee9cf0fdcde30286fde7899b321b0cb6894db661427f489f358'),
    ('fractal_transf_abs_add_tglad_fold4d.cpp', 'cFractalTransfAbsAddTgladFold4d',
     'c6e34e279c598956b337dde92d5c9ce2636c124d5f89ae542d87566a6f009999'),
)


def config_values(path):
    cfg = {}
    for line in path.read_text().splitlines():
        key, *values = line.split()
        if key in cfg:
            raise ValueError('duplicate config key')
        cfg[key] = list(map(float, values))
    expected = {'controls': [250, 1, 10000, 1, 1, 1, 13.68766681086107, 1],
                'axes': [1]*4, 'starts': [0]*4, 'stops': [250]*4,
                'foldColor': [0, 0], 'scale': [3],
                'slot0': [100, 0, 1, 0, 0, 1], 'slot1': [100, 0, 1, 0, 0, 1]}
    for key, values in expected.items():
        if key not in cfg or not np.allclose(cfg[key], values, rtol=0, atol=1e-12):
            raise ValueError(f'unsupported {key}: {cfg.get(key)}')
    if cfg['sequence'] != [0, 0] + [1 if i % 2 == 0 else 0 for i in range(2, 250)]:
        raise ValueError('unexpected native hybrid sequence')
    if len(cfg['camera']) != 3 or len(cfg['target']) != 3 or len(cfg['limits']) != 4 or not all(
            np.isfinite(values).all() for values in cfg.values()):
        raise ValueError('invalid config values')
    light = np.array(cfg['target'])-np.array(cfg['camera'])
    if not np.linalg.norm(light)>0:
        raise ValueError('degenerate camera direction')
    cfg['light_direction']=(light/np.linalg.norm(light)).tolist()
    return cfg


def vector(values):
    if len(values) not in (3, 4):
        raise ValueError('expected three or four components')
    return 'V(' + ','.join(scalar(v) for v in values) + ')'


def import_formula(path, class_name, digest, slot, cfg):
    if sha256(path) != digest:
        raise ValueError('unsupported native formula revision')
    source = path.read_text()
    marker = 'void ' + class_name + '::FormulaCode('
    body = source[source.index('{', source.index(marker)):].strip()
    if not body.endswith('}'):
        raise ValueError('unexpected formula body')
    replacements = {
        'transformCommon.scale3': scalar(cfg['scale'][0]),
        'transformCommon.additionConstant0000': vector(cfg['limits']),
        'foldColor.auxColorEnabledFalse': 'false',
        'foldColor.startIterationsA': '0', 'foldColor.stopIterationsA': '250',
        'transformCommon.functionEnabledCxFalse': 'false',
    }
    for axis, iteration in zip('xyzw', 'ABCD'):
        replacements[f'transformCommon.functionEnabledA{axis}'] = 'true'
        replacements[f'transformCommon.startIterations{iteration}'] = '0'
        replacements[f'transformCommon.stopIterations{iteration}'] = '250'
        # These values are only consumed in the native disabled colour block.
        replacements[f'foldColor.difs0000.{axis}'] = 'R(0)'
        replacements[f'mandelbox.color.factor4D.{axis}'] = 'R(0)'
    body = re.sub(r'fractal->([A-Za-z0-9_.]+)',
                  lambda m: replacements[m[1]], body)
    body = body.replace('CVector4 ', 'V ').replace('double ', 'R ')
    notice = source.split('#include', 1)[0]
    return notice + f'\nvoid formula{slot}(thread V &z, thread Aux &aux) ' + body


def field_source(root, cfg, arithmetic):
    helper = SUPPORT / ('TwoTerm.metal' if arithmetic == 'two' else 'Expansion.metal')
    prefix = r'''
using R = fpt_precision::Scalar;
using fpt_precision::absolute;
using fpt_precision::root;
R maximum(R a,R b) { return a>b?a:b; }
R minimum(R a,R b) { return a<b?a:b; }
R fabs(R a) { return absolute(a); }
bool operator!=(R a,R b) { return a<b || a>b; }
void operator+=(thread R &a,R b) { a=a+b; }
void operator-=(thread R &a,R b) { a=a-b; }
void operator*=(thread R &a,R b) { a=a*b; }
struct V {
    R x,y,z,w;
    V(R a,R b,R c,R d=R(0)):x(a),y(b),z(c),w(d) {}
    R Dot(V b) { return x*b.x+y*b.y+z*b.z+w*b.w; }
    R Length() { return root(Dot(*this)); }
};
V operator+(V a,V b) { return V(a.x+b.x,a.y+b.y,a.z+b.z,a.w+b.w); }
V operator-(V a,V b) { return V(a.x-b.x,a.y-b.y,a.z-b.z,a.w-b.w); }
V operator*(V a,R b) { return V(a.x*b,a.y*b,a.z*b,a.w*b); }
void operator*=(thread V &a,R b) { a=a*b; }
V fabs(V a) { return V(absolute(a.x),absolute(a.y),absolute(a.z),absolute(a.w)); }
struct Aux { R DE,color; int i; };
'''
    bodies = [import_formula(root/'formula/definition'/name, cls, digest, i, cfg)
              for i, (name, cls, digest) in enumerate(FORMULAS)]
    sequence = ','.join(str(int(v)) for v in cfg['sequence'])
    field = r'''
R field(V z) {
    Aux aux={R(1),R(1),0};
    R radius=z.Length();
    for(int i=0;i<250;++i) {
        aux.i=i;
        if(kSequence[i]==0) formula0(z,aux); else formula1(z,aux);
        radius=z.Length();
        if(radius>R(100)) break;
    }
    return minimum(maximum(radius/aux.DE,R(0)),R(10));
}
float2 pack(R a) { return float2(a.hi,a.mid+a.lo); }
'''
    return helper.read_text()+prefix+'\n'.join(bodies)+f'\nconstant int kSequence[250]={{{sequence}}};\n'+field


def render_source(shared, cfg, height):
    template = 'kernel void probe' + (SUPPORT/'Reference.metal').read_text().split('kernel void probe', 1)[1]
    controls = cfg['controls']
    template = template.replace('gradient.Dot(direction)', 'gradient.Dot('+vector(cfg['light_direction'])+')')
    for key, value in {
        '@ORIGIN@': vector(cfg['camera']), '@MAX_STEPS@': str(int(controls[2])),
        '@MIN_THRESHOLD@': scalar(1e-12), '@THRESHOLD_SCALE@': scalar(height*controls[4]/controls[5]),
        '@DE_FACTOR@': scalar(controls[3]), '@VIEW_MAX@': scalar(controls[6]),
        '@REFINE_RATIO@': scalar(1-.001*controls[4]), '@NORMAL_SCALE@': scalar(controls[7]),
    }.items():
        template = template.replace(key, value)
    if '@' in template:
        raise ValueError('unexpanded render template')
    return shared+template


def depth_metrics(native, metal):
    if native.ndim != 2 or native.shape[1] != 16 or metal.shape != (len(native),8):
        raise ValueError('invalid first-hit record shape')
    if not np.isfinite(native).all() or not np.isfinite(metal).all() or not (native[:,14]>0).all():
        raise ValueError('invalid first-hit values')
    hits = (native[:,8]>.5)&(metal[:,2]>.5)
    error = abs(metal[:,0].astype(float)+metal[:,1].astype(float)-native[:,12])
    normalized = error/native[:,14]
    return dict(rays=len(native),native_hits=int((native[:,8]>.5).sum()),metal_hits=int((metal[:,2]>.5).sum()),
        common_hits=int(hits.sum()),median_error_in_thresholds=float(np.median(normalized[hits])) if hits.any() else None,
        max_error_in_thresholds=float(normalized[hits].max()) if hits.any() else None,
        common_over_one_threshold=int((hits&(normalized>1)).sum()))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ('scene', 'mandelbulber-root', 'native-probe', 'points', 'out'):
        parser.add_argument('--'+key, type=Path, required=True)
    parser.add_argument('--arithmetic', choices=('two', 'three'), default='two')
    parser.add_argument('--round-data', choices=('none', 'camera', 'formula'), default='none')
    parser.add_argument('--size', default='160x120')
    parser.add_argument('--field-only', action='store_true')
    parser.add_argument('--metal-probe', type=Path, help='Use generated FPT camera rays instead of native rays.')
    parser.add_argument('--verify-hits', action='store_true', help='Compare a bounded ray grid with the original native marcher.')
    args = parser.parse_args()
    if sha256(args.scene) != SCENE_SHA:
        raise ValueError('only pinned scene 578 is supported')
    width, height = map(int, args.size.split('x'))
    if not (0 < width <= 320 and 0 < height <= 240 and width*3 == height*4):
        raise ValueError('require authored 4:3 aspect, at most 320x240')
    if args.verify_hits and (width<8 or height<8 or args.field_only):
        raise ValueError('hit verification requires an image of at least 8x8')
    points = np.loadtxt(args.points, ndmin=2)
    if points.shape[1] != 4 or not 1 <= len(points) <= 4096 or not np.isfinite(points).all() or not (points[:,3]>0).all():
        raise ValueError('invalid point input')
    args.out.mkdir(parents=True, exist_ok=False)
    out = args.out.resolve()
    scene = out/'scene.fract'
    scene.write_text(replace_main_parameters(args.scene.read_text(), {
        'stereo_enabled':'false', 'image_width':str(width), 'image_height':str(height)}))
    native = args.native_probe.resolve()
    execute([native,scene,args.points,out/'config.tsv','hybrid-config'],out,'config')
    cfg = config_values(out/'config.tsv')
    for key in (['camera'] if args.round_data=='camera' else ['scale','limits'] if args.round_data=='formula' else []):
        cfg[key] = [float(np.float32(v)) for v in cfg[key]]
    (out/'effective-config.json').write_text(json.dumps(cfg,indent=2)+'\n')
    shared = field_source(args.mandelbulber_root,cfg,args.arithmetic)
    from run_mandel_ifs_precision import probe_kernel
    (out/'field.metal').write_text(shared+probe_kernel())
    (out/'points.bin').write_bytes(pack_inputs(points[:,:3]))
    execute(['clang++','-std=c++17','-O2','-fobjc-arc',SUPPORT/'probe.mm',
        '-framework','Foundation','-framework','Metal','-o',out/'probe'],out,'compile-driver')
    pipelines = [json.loads(execute([out/'probe',out/'field.metal',out/'points.bin',out/f'field-{i}.bin'],out,f'field-{i}')) for i in range(2)]
    execute([native,scene,args.points,out/'native.tsv','primary'],out,'native-field')
    expected = np.loadtxt(out/'native.tsv',ndmin=2)[:,0]
    records = np.fromfile(out/'field-0.bin',dtype='<f4').reshape(-1,8)
    actual = records[:,0].astype(float)+records[:,1].astype(float)
    error = abs(actual-expected)
    repeat = (out/'field-0.bin').read_bytes()==(out/'field-1.bin').read_bytes()
    passed = bool(np.isfinite(actual).all() and repeat and np.all(error <= 1e-12+abs(expected)*1e-5))
    report = dict(scope='Scene578 isolated white-headlight precision probe, not production support',
        source_sha256=SCENE_SHA,control_sha256=sha256(scene),arithmetic=args.arithmetic,round_data=args.round_data,
        passed=passed,repeat_byte_exact=repeat,points=len(points),max_absolute_error=float(error.max()),
        median_relative_error=float(np.median(error/np.maximum(abs(expected),1e-30))),
        max_relative_error=float(np.max(error/np.maximum(abs(expected),1e-30))),field_pipelines=pipelines,
        native_probe_sha256=sha256(native),input_sha256=sha256(args.points),script_sha256=sha256(Path(__file__)),
        config_sha256=sha256(out/'config.tsv'),effective_config_sha256=sha256(out/'effective-config.json'),
        field_shader_sha256=sha256(out/'field.metal'),driver_source_sha256=sha256(SUPPORT/'probe.mm'),
        driver_sha256=sha256(out/'probe'),
        arithmetic_source_sha256=sha256(SUPPORT/('TwoTerm.metal' if args.arithmetic=='two' else 'Expansion.metal')),
        formulas=[dict(path=str(args.mandelbulber_root/'formula/definition'/name),sha256=digest) for name,_,digest in FORMULAS],
        samples=[dict(xyz=p[:3].tolist(),native=float(n),metal=float(m),absolute_error=float(e)) for p,n,m,e in zip(points,expected,actual,error)])
    def save():
        (out/'summary.json').write_text(json.dumps(report,indent=2)+'\n')
    save()
    print(json.dumps({k:v for k,v in report.items() if k not in ('samples','formulas')},indent=2),flush=True)
    if not passed:
        raise RuntimeError('field parity gate failed; no image rendered')
    if args.field_only:
        return
    xy=[((x-width/2)/height,(height/2-y)/height,0,1e-12) for y in range(height) for x in range(width)]
    np.savetxt(out/'ray-plane.tsv',xy,fmt='%.17g',delimiter='\t')
    if args.metal_probe:
        inputs = [[x,y,0,-3] for x,y,_,_ in xy]
        (out/'ray-inputs.json').write_text(json.dumps(inputs))
        execute([args.metal_probe.resolve(),scene,args.mandelbulber_root,str(width),str(height),
                 out/'ray-inputs.json',out/'fpt-rays.json'],out,'capture-fpt-rays')
        data=json.loads((out/'fpt-rays.json').read_text())
        rays=np.array(data['samples'],dtype=float)[:,[0,2,1]]
        report['metal_probe_sha256']=sha256(args.metal_probe)
        report['fpt_ray_metadata_sha256']=sha256(out/'fpt-rays.json')
        np.savetxt(out/'rays.tsv',rays,fmt='%.17g',delimiter='\t')
    else:
        execute([native,scene,out/'ray-plane.tsv',out/'rays.tsv','rays'],out,'native-rays')
        rays=np.loadtxt(out/'rays.tsv',ndmin=2)
    if rays.shape!=(width*height,3) or not np.isfinite(rays).all():
        raise ValueError('invalid camera rays')
    (out/'rays.bin').write_bytes(pack_inputs(rays))
    (out/'render.metal').write_text(render_source(shared,cfg,height))
    pipeline=json.loads(execute([out/'probe',out/'render.metal',out/'rays.bin',out/'render.bin'],out,'render',1800))
    raw=np.fromfile(out/'render.bin',dtype='<f4').reshape(height,width,8)
    if not np.isfinite(raw).all():
        raise ValueError('nonfinite render')
    hits=raw[:,:,2]>.5
    shade=np.rint(np.clip(raw[:,:,4].astype(float)+raw[:,:,5],0,1)*255).astype('uint8')
    shade[~hits]=0
    Image.fromarray(np.repeat(shade[:,:,None],3,axis=2)).save(out/'render.png')
    report['render']=dict(width=width,height=height,pixel_sampling='native integer, 1 ray/pixel',
        ray_provider='generated FPT Metal' if args.metal_probe else 'native',lighting='camera-aligned directional white diffuse',
        hits=int(hits.sum()),stalls=int((raw[:,:,6]>.5).sum()),max_step_exits=int((raw[:,:,3]>=10000).sum()),
        pipeline=pipeline,sha256=sha256(out/'render.png'),shader_sha256=sha256(out/'render.metal'),
        ray_sha256=sha256(out/'rays.tsv'),raw_sha256=sha256(out/'render.bin'),
        limitations='White directional light, no AO/shadows, safe math; isolated diagnostic, not production FPT or NAADF')
    if args.verify_hits:
        ids=[int(y)*width+int(x) for y in np.linspace(3,height-4,min(13,height)).round()
             for x in np.linspace(3,width-4,min(21,width)).round()]
        np.savetxt(out/'hit-inputs.tsv',np.column_stack([rays[ids],np.zeros(len(ids))]),fmt='%.17g',delimiter='\t')
        execute([native,scene,out/'hit-inputs.tsv',out/'native-hits.tsv','hybrid-march'],out,'native-hits')
        audit=depth_metrics(np.loadtxt(out/'native-hits.tsv',ndmin=2),raw.reshape(-1,8)[ids])
        audit.update(input_sha256=sha256(out/'hit-inputs.tsv'),native_sha256=sha256(out/'native-hits.tsv'),
                     seed=0,pixel_indices=ids,origin='original authored camera, no float rounding override')
        report['native_hit_check']=audit
    save()
    print(json.dumps(report['render'],indent=2),flush=True)
    if report['render']['stalls'] or report['render']['max_step_exits']:
        raise RuntimeError('render incomplete')


if __name__ == '__main__':
    main()
