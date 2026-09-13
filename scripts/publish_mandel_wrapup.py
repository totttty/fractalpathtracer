#!/usr/bin/env python3
"""Publish unreviewed wrap-up evidence without changing catalogue decisions."""
import argparse
from collections import Counter
import json
from pathlib import Path
import re

from PIL import Image, ImageDraw

from run_release_canaries import sha256, scene_dimensions


def load(path):
    return json.loads(Path(path).read_text())


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def categories(review):
    """Non-exclusive work queues from observations, never causal diagnoses."""
    if review['id'] == '095':
        return ['user-deferred']
    note = review['note'].lower()
    rules = {
        'volume-atmosphere': r'fog|volum|cloud|atmospher',
        'exposure-clipping': r'clip|overbright|overexpos|saturat|wash',
        'missing-illumination': r'dark|black|illumination|glow|light|orbit.trap',
        'material-optics': r'material|reflect|transparen|refract|roughness|palette',
        'geometry-framing': r'fram|silhouette|missing.*(structur|geometr)|filled|cavit|hole',
    }
    found = [key for key, pattern in rules.items() if re.search(pattern, note)]
    if review.get('geometry') in ('uncertain', 'needs-work'):
        found.append('geometry-control-needed')
    return sorted(set(found)) or ['other-visual']


def checked_asset(result, key='capture'):
    asset = result.get(key)
    if not asset or sha256(Path(asset['path'])) != asset['sha256']:
        raise ValueError('missing or mismatched ' + key)
    return asset


def reusable_fpt(report, row, source, binary, scene_root, mandel_root, mode):
    if (row['id'], row['path'], row['sha256']) != (source['id'], source['path'], source['sha256']):
        raise ValueError('source identity mismatch')
    identity = report['identity']
    settings = identity['settings']
    if identity['executables'].get(str(binary.resolve())) != sha256(binary):
        raise ValueError('renderer identity mismatch')
    if settings['samples'] != 32 or settings['max_axis'] != 300 or settings['aspect'] != 'authored':
        raise ValueError('reuse requires authored 300px/32-SPP contract')
    if identity.get('environment') != {'FPT_MANDEL_TILED_DISPATCH': '1', 'FPT_MANDEL_TILE_ROWS': '8'}:
        raise ValueError('environment mismatch')
    scene = (scene_root / source['path']).resolve()
    if sha256(scene) != source['sha256']:
        raise ValueError('scene changed')
    size = list(scene_dimensions(scene.read_text(), 300))
    if row['size'] != size:
        raise ValueError('authored dimensions changed')
    result = row['modes'][mode]
    if result['status'] != 'ok':
        raise ValueError('not a completed capture')
    asset = checked_asset(result)
    metadata_asset = checked_asset(result, 'metadata')
    metadata = load(metadata_asset['path'])
    if (metadata['scene_sha256'], metadata['samples'], metadata['width'], metadata['height']) != (source['sha256'], 32, *size):
        raise ValueError('metadata contract mismatch')
    folder = Path(asset['path']).parent
    command_path = folder / 'command.json'
    command = load(command_path)
    expected = [str(binary.resolve()), 'render', str(scene), '--mandelbulber-root', str(mandel_root.resolve()),
                '--width', str(size[0]), '--height', str(size[1]), '--samples', '32',
                '--sdf-accumulation', 'chunked', '--sdf-chunk-samples', '1',
                '--mandel-appearance', 'geometry' if mode == 'geometry' else 'authored-path',
                '--out', str(folder)]
    if command != expected:
        raise ValueError('camera/render command overrides differ')
    lightmap = identity.get('lightmaps', {}).get(source['id'])
    if lightmap and 'path' in lightmap and sha256(Path(lightmap['path'])) != lightmap['sha256']:
        raise ValueError('lightmap changed')
    with Image.open(asset['path']) as image:
        if image.size != tuple(size):
            raise ValueError('image dimensions mismatch')
    return dict(status='ok', capture=asset, metadata=metadata_asset, dimensions=size, samples=32,
                reused=True, camera={k:metadata.get(k) for k in ('camera_position','camera_yaw_pitch','camera_roll','camera_fov')},
                command_sha256=sha256(command_path), renderer_sha256=sha256(binary),
                source_sha256=source['sha256'], note='Source-bound authored camera; no command camera overrides. Lightmap fingerprint checked; not a complete external-texture audit.')


def compact(result, dimensions, samples):
    data = {k:result[k] for k in ('status','error','stderr_tail','screening','reference_cache','timeout_seconds') if k in result}
    data.update(dimensions=dimensions, samples=samples, reused=False)
    if result.get('status') == 'ok':
        data['capture'] = checked_asset(result)
    return data


def draw_card(row, output):
    canvas = Image.new('RGB', (900, 245), '#202326')
    draw = ImageDraw.Draw(canvas)
    columns = [('mandel','Native reduced screen'),('geometry','FPT neutral'),('authored','FPT authored')]
    for i, (mode, label) in enumerate(columns):
        x = i * 300
        result = row['modes'].get(mode, {'status':'not_requested'})
        draw.text((x+8, 8), label, fill='white')
        asset = result.get('capture')
        if asset:
            with Image.open(asset['path']) as source:
                image = source.convert('RGB')
                image.thumbnail((290, 160))
                canvas.paste(image, (x+(300-image.width)//2, 30+(160-image.height)//2))
            detail = f"{result['dimensions']} | {result['samples']} | " + ('reused' if result['reused'] else 'fresh')
        else:
            draw.text((x+8, 100), result['status'], fill='#ffbb99')
            detail = 'No validated completed image'
        draw.text((x+8, 194), detail, fill='#bbbbbb')
    title = row['id'] + ' ' + Path(row['path']).stem + ' | PREVIEW ONLY'
    draw.text((8, 218), title[:125], fill='white')
    canvas.save(output, quality=85)


def publish(root, capture_root, output, binary, scene_root, mandel_root):
    catalog_path = root/'docs/mandel-catalog/catalog.json'
    catalog = load(catalog_path)
    reviews = load(root/'docs/mandel-catalog/reviews.json')
    native = load(capture_root/'native/summary.json')
    retest = load(capture_root/'execution/summary.json')
    by_id = {r['id']:r for r in catalog['scenes']}
    expected_native = {r['id'] for r in catalog['scenes'] if r['status']=='experimental'}
    expected_retest = {r['id'] for r in catalog['scenes'] if r['status']=='blocked' and 'visual_decision' not in r}
    for report, expected, modes in ((native,expected_native,{'mandel'}),(retest,expected_retest,{'geometry','authored'})):
        ids = [r['id'] for r in report['rows']]
        if len(ids)!=len(set(ids)) or set(ids)!=expected:
            raise ValueError('incomplete or duplicate wrap-up inventory')
        for row in report['rows']:
            source=by_id[row['id']]
            if row['path']!=source['path'] or row['sha256']!=source['sha256'] or set(row['modes'])!=modes:
                raise ValueError('wrap-up source/mode mismatch')
        if report['identity']['executables'].get(str(binary.resolve()))!=sha256(binary):
            raise ValueError('wrap-up binary mismatch')
        for path,digest in report['identity']['executables'].items():
            if sha256(Path(path))!=digest:raise ValueError('capture executable changed')
    if (native['identity']['settings']['max_axis']!=150 or native['identity']['settings']['native_mc_samples']!=1
            or native['identity']['settings']['native_timeout']!=10 or native['identity']['settings']['reference_backend']!='CPU'):
        raise ValueError('native screening settings mismatch')
    if (retest['identity']['settings']['max_axis']!=96 or retest['identity']['settings']['samples']!=1
            or retest['identity']['settings']['timeout']!=15):
        raise ValueError('execution screening settings mismatch')
    output.mkdir(exist_ok=False, parents=True)
    (output/'images').mkdir()
    historical=[]
    for path in sorted((root/'reports').glob('mandel-review-batch*/captures/summary.json'), reverse=True):
        report=load(path)
        for row in report['rows']:
            if row['id'] in expected_native:
                historical.append((path,report,row))
    previews=[]
    for row in native['rows']:
        source=by_id[row['id']]
        item=dict(id=row['id'],path=row['path'],source_sha256=row['sha256'],catalog_status=source['status'],
                  tier='preview-only',modes={'mandel':compact(row['modes']['mandel'],row['size'],'MC cap 1 (not SPP)')},
                  native_sampling=row['native_sampling'],reuse_rejections=[])
        for mode in ('geometry','authored'):
            for path,report,old in historical:
                if old['id']!=row['id']:continue
                try:
                    result=reusable_fpt(report,old,source,binary,scene_root,mandel_root,mode)
                    result['samples']='32 SPP'
                    result['evidence_report']=str(path)
                    result['evidence_report_sha256']=sha256(path)
                    item['modes'][mode]=result
                    break
                except (ValueError,KeyError,OSError) as error:
                    item['reuse_rejections'].append(dict(mode=mode,report=str(path),reason=str(error)))
            item['modes'].setdefault(mode,dict(status='no_matching_completed_capture'))
        previews.append(item)
    for row in retest['rows']:
        previews.append(dict(id=row['id'],path=row['path'],source_sha256=row['sha256'],
            catalog_status='blocked',tier='preview-only' if all(r['status']=='ok' for r in row['modes'].values()) else 'known-issue',
            modes={m:compact(r,row['size'],'1 SPP') for m,r in row['modes'].items()}))
    for row in previews:
        name=f"images/{row['id']}.jpg"
        draw_card(row, output/name)
        row['preview']=name
    pages=[]
    for start in range(0,len(previews),5):
        group=previews[start:start+5];name=f'preview-{start//5+1:02d}.md';pages.append(name)
        lines=['# Unreviewed Screening Previews','','[Status](README.md)','',
               '**Not matched-quality comparisons or reviewed-gallery approvals.** Native: 150px max, authored effects, enabled MC capped at one. Reused FPT: 300px/32 SPP. Execution retests: 96px/1 SPP. Images are fit without cropping or upscaling.','']
        for row in group:
            lines += [f"## {row['id']} {Path(row['path']).stem}",'',f"![Unreviewed preview]({row['preview']})",'',
                      ' | '.join(f"{m}: {r['status']}" for m,r in row['modes'].items()),'']
        (output/name).write_text('\n'.join(lines).rstrip()+'\n')
    groups={}
    for review in reviews['rows']:
        if review['decision']!='needs-work':continue
        for category in categories(review):groups.setdefault(category,[]).append(review)
    group_lines=['# Known-Issue Work Queues','','[Status](README.md) | [Full visual audit](../mandel-showcase/needs-work.md)','',
                 'Non-exclusive keyword groupings from existing visual notes, not proven shared causes. No new diagnoses or acceptance decisions. Scene 095 is explicitly deferred by the user.','']
    for category, rows in sorted(groups.items()):
        group_lines += [f'## {category} ({len(rows)})','']
        for r in sorted(rows,key=lambda r:int(r['id'])):
            group_lines.append(f"- [{r['id']}](../mandel-showcase/{r['id']}.md): {r['note']}")
        group_lines.append('')
    (output/'known-issues.md').write_text('\n'.join(group_lines).rstrip()+'\n')
    stats=dict(catalog=dict(Counter(r['status'] for r in catalog['scenes'])),
        native_status=dict(Counter(r['modes']['mandel']['status'] for r in previews if r['id'] in expected_native)),
        reused_fpt={m:sum(r['modes'].get(m,{}).get('reused',False) for r in previews) for m in ('geometry','authored')},
        execution_status={m:dict(Counter(r['modes'][m]['status'] for r in retest['rows'])) for m in ('geometry','authored')},
        execution_both_passed=sum(all(v['status']=='ok' for v in r['modes'].values()) for r in retest['rows']),
        native_cache_hits=native['counts'].get('native_cache_hits',0),
        native_wall_seconds=native['suite_wall_seconds'],execution_wall_seconds=retest['suite_wall_seconds'],
        issue_groups={k:len(v) for k,v in groups.items()})
    write(output/'screening-evidence.json',dict(version=1,visual_review='not-established',stats=stats,
        settings={'native':native['identity']['settings'],'execution':retest['identity']['settings']},
        executables=native['identity']['executables'],
        capture_reports={str(capture_root/p):sha256(capture_root/p) for p in ('native/summary.json','execution/summary.json')},rows=previews))
    indexed={r['id']:r for r in previews}
    index=[]
    for source in catalog['scenes']:
        row=dict(id=source['id'],name=source['name'],path=source['path'],source_sha256=source['sha256'],
                 catalog_status=source['status'],tier='reviewed' if source['status']=='reviewed' else
                 'known-issue' if 'visual_decision' in source else indexed[source['id']]['tier'])
        if source['id'] in indexed:row['preview']=indexed[source['id']]['preview']
        if 'visual_decision' in source:row['review_page']=f"../mandel-showcase/{source['id']}.md"
        index.append(row)
    write(output/'catalog-index.json',dict(version=1,source_catalog_sha256=sha256(catalog_path),rows=index))
    lines=['# Mandel Release Status','','[Reviewed gallery](../mandel-showcase/README.md) | [All 746 scenes](../mandel-catalog/scenes.md) | [Known-issue groups](known-issues.md) | [Screening evidence](screening-evidence.json) | [Complete index](catalog-index.json)','',
           '**Reviewed output is frozen. Nothing in this screening report promotes a scene or certifies NAADF/CVOX interoperability.**','',
           '## Release Tiers','',f"- Reviewed: {stats['catalog']['reviewed']}, unchanged.",
           f"- Incomplete comparisons: {len(expected_native)}; native reduced-screen outcomes: `{stats['native_status']}`.",
           f"- Existing visual holds: {sum(r['decision']=='needs-work' for r in reviews['rows'])}; scene 095 remains skipped.",
           f"- Historical execution failures: {len(expected_retest)} retried; {stats['execution_both_passed']} now execute in both modes at screening settings. Their catalogue status remains blocked pending full review.",'',
           '## Limits','',
           'Native previews preserve authored effects at 150px maximum edge; Monte Carlo is capped at one only when already enabled. This is not universally one SPP. Ten seconds per native attempt; no repeated retries.',
           'Execution retests use 96px maximum edge, one FPT sample and 15 seconds per mode. A timeout is not proof of unsupported geometry. These settings are intentionally cheaper than gallery captures.',
           'Reused 300px/32-SPP FPT captures must match source, authored dimensions, exact render command, environment, executable and image/metadata hashes. The source hash binds the authored camera; camera metadata is retained. Lightmaps are checked, but this is not a complete external-texture dependency audit.',
           'There are no cross-resolution MAE rankings, automatic promotions or claims of visual review for these screening previews. Known-issue groups overlap and are work queues, not diagnoses. Fog/cloud fixes and scene 095 remain deferred. Renderer code is unchanged.','',
           '## Timing','',f"Native screening: {stats['native_wall_seconds']:.2f}s. Execution retests: {stats['execution_wall_seconds']:.2f}s. Elapsed workflow time includes overhead and is not GPU performance.",'',
           '## Previews','']
    lines += [f'- [Preview page {i+1}]({name})' for i,name in enumerate(pages)]
    lines += ['', '## Follow-Up','', 'Use matching-resolution native evidence and explicit visual review before promoting promising previews. Keep reproducible execution errors, geometry issues, surface lighting and deferred volumes in separate work queues. Do not block release of the reviewed gallery on all outliers.','']
    (output/'README.md').write_text('\n'.join(lines).rstrip()+'\n')
    write(output/'manifest.json',dict(version=1,files={str(p.relative_to(output)):sha256(p) for p in sorted(output.rglob('*')) if p.is_file()}))
    return stats


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root',type=Path,default=Path(__file__).resolve().parents[1])
    parser.add_argument('--captures',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--fpt',type=Path,required=True)
    parser.add_argument('--scene-root',type=Path,required=True)
    parser.add_argument('--mandelbulber-root',type=Path,required=True)
    a=parser.parse_args()
    print(json.dumps(publish(a.root.resolve(),a.captures.resolve(),a.output.resolve(),a.fpt.resolve(),a.scene_root.resolve(),a.mandelbulber_root.resolve()),indent=2))


if __name__=='__main__':main()
