#!/usr/bin/env python3
"""Assemble capture evidence and publish paginated, explicitly reviewed galleries."""
import argparse
import copy
import json
from pathlib import Path
import re
import shutil
import textwrap

from PIL import Image, ImageDraw
from mandel_catalog import ROOT, source_index
from mandel_review import evidence_digest, validate_reviews
from run_release_canaries import sha256

MODES = ('mandel','geometry','authored')


def assemble(ranked, additional, refresh, catalog, ranked_manifest, additional_manifest, revision):
    if not re.fullmatch(r'[0-9a-f]{40}',revision):
        raise ValueError('renderer revision must be a full commit hash')
    sources = source_index(catalog['scenes'])
    base_rows = ranked['rows'] + additional['rows']
    if len({r['id'] for r in base_rows}) != len(base_rows):
        raise ValueError('duplicate review rows')
    refreshed = {r['id']:r for r in refresh['rows']}
    expected_refresh = {r['id'] for r in refresh['identity']['manifest']['scenes']}
    if set(refreshed) != expected_refresh or not expected_refresh <= {r['id'] for r in base_rows}:
        raise ValueError('incomplete or unknown refresh rows')
    binaries = list(refresh['identity']['executables'].values())
    if len(binaries) != 1 or refresh['identity']['settings']['samples'] != 32:
        raise ValueError('unexpected refresh binary/sample contract')
    old_manifests = {r['id']:(r,m) for m in (ranked_manifest,additional_manifest) for r in m['rows']}
    portable, local = [], []
    for original in base_rows:
        row = copy.deepcopy(original)
        source = sources[int(row['id'])]
        for key in ('id','path','sha256'):
            if row[key] != source[key]:
                raise ValueError('capture/catalogue source mismatch')
        old, manifest = old_manifests[row['id']]
        if old['source_sha256'] != row['sha256'] or old['source'] != row['path']:
            raise ValueError('published source mismatch')
        # Validate old references even when FPT is refreshed; never replace one
        # scene's reference with another scene's similarly named image.
        for mode in MODES:
            if row['modes'][mode]['capture']['sha256'] != old['captures'][mode]['sha256']:
                raise ValueError('previous capture differs from published manifest')
        fresh = refreshed.get(row['id'])
        if fresh:
            if any(fresh[k] != row[k] for k in ('id','path','sha256','size')):
                raise ValueError('refresh identity/dimensions mismatch')
            for mode in ('geometry','authored'):
                row['modes'][mode] = fresh['modes'][mode]
        item = dict(id=row['id'],path=row['path'],sha256=row['sha256'], dimensions=row['size'],
            samples=32,bounces=manifest['bounces'],upstream_revision=manifest['upstream_revision'],
            renderer_revision=revision if fresh else manifest['renderer_revision'],
            renderer_sha256=binaries[0] if fresh else manifest['production_sha256'],
            epoch='refreshed-2026-09-11' if fresh else 'historical-2026-09-10',
            reference_overrides=old.get('reference_overrides',{}),
            native_reference_reduced=old['native_reference_reduced'],
            native_sampling='Authored native CPU settings; not equivalent to FPT SPP.',
            captures={})
        for mode in MODES:
            result = row['modes'][mode]
            if result['status'] != 'ok':
                raise ValueError(f'{row["id"]} {mode}: incomplete capture')
            asset = result['capture']
            if sha256(Path(asset['path'])) != asset['sha256']:
                raise ValueError('capture hash mismatch')
            with Image.open(asset['path']) as image:
                size = list(image.size)
            expected = row['size']
            if mode == 'mandel' and item['native_reference_reduced']:
                if max(size) != 96:
                    raise ValueError('invalid reduced reference')
            elif size != expected:
                raise ValueError('capture dimensions mismatch')
            item['captures'][mode] = dict(sha256=asset['sha256'],dimensions=size)
            if mode != 'mandel':
                metadata = result['metadata']
                if sha256(Path(metadata['path'])) != metadata['sha256']:
                    raise ValueError('metadata hash mismatch')
                values = json.loads(Path(metadata['path']).read_text())
                if values['samples'] != 32 or values['scene_sha256'] != row['sha256'] or [values['width'],values['height']] != expected:
                    raise ValueError('capture settings mismatch')
                item['captures'][mode]['metadata_sha256'] = metadata['sha256']
        portable.append(item)
        local.append(dict(id=row['id'],paths={m:row['modes'][m]['capture']['path'] for m in MODES}))
    return dict(version=1,scope='Capture-specific visual review, not whole-corpus current-binary validation or NAADF/CVOX certification.',rows=portable), dict(rows=local)


def validate_assets(evidence, assets, partial=False):
    """Hash-check raw captures. Partial maps cover only rows whose raw captures are still local."""
    local = {r['id']:r for r in assets['rows']}
    ids = {r['id'] for r in evidence['rows']}
    if len(local) != len(assets['rows']) or not (set(local) <= ids if partial else set(local) == ids):
        raise ValueError('asset map does not match evidence')
    for row in evidence['rows']:
        if row['id'] not in local:
            continue
        for mode in MODES:
            path = Path(local[row['id']]['paths'][mode])
            if sha256(path) != row['captures'][mode]['sha256']:
                raise ValueError('publication asset hash mismatch')
    return local


def record_assessment(assessment, evidence, legacy, evidence_hash, existing=None):
    if assessment['version'] != 1 or assessment['evidence_sha256'] != evidence_hash:
        raise ValueError('assessment was made against different capture evidence')
    annotated={r['id']:r for r in assessment['rows']}
    if len(annotated)!=len(assessment['rows']):
        raise ValueError('duplicate assessment')
    previous={r['id']:r for r in legacy['rows']} if legacy else {}
    retained={r['id']:r for r in existing['rows']} if existing else {}
    if existing and len(retained)!=len(existing['rows']):
        raise ValueError('duplicate retained review')
    ids={r['id'] for r in evidence['rows']}
    if not set(annotated)<=ids or not ids<=set(annotated)|set(previous)|set(retained) or not set(retained)<=ids:
        raise ValueError('every new scene requires an explicit visual assessment')
    rows=[]
    for proof in evidence['rows']:
        review=annotated.get(proof['id'])
        if review is None and proof['id'] in retained:
            old=retained[proof['id']]
            if old['source_sha256']!=proof['sha256'] or old['evidence_sha256']!=evidence_digest(proof):
                raise ValueError('changed evidence requires a fresh assessment')
            rows.append(old)
            continue
        if review is None:
            old=previous[proof['id']]
            if (old['source_sha256']!=proof['sha256'] or old['source']!=proof['path']
                    or any(old['captures'][m]['sha256']!=proof['captures'][m]['sha256'] for m in MODES)):
                raise ValueError('legacy acceptance cannot migrate onto changed captures')
            review=dict(id=proof['id'],decision='accepted-with-limitations',geometry='acceptable',illumination='acceptable',
                note=old['note'] or 'Prior ranked-gallery visual acceptance retained for these unchanged captures. Native appearance and fine-detail parity are not certified.')
            reviewer='Prior user-accepted ranked gallery; migrated capture-specific decision'
        else:
            reviewer=assessment['reviewer']
        rows.append(dict(review,source_sha256=proof['sha256'],evidence_sha256=evidence_digest(proof),
            reviewer=reviewer,reviewed_at=assessment['reviewed_at']))
    return dict(version=1,policy='Visual acceptance of specific captures; colours may differ. Not whole-corpus current-binary or NAADF/CVOX validation.',rows=rows)


def append_batch(evidence, assets, batch, catalog, revision):
    """Preserve accepted evidence verbatim and append only complete new triplets."""
    validate_assets(evidence,assets,partial=True)
    from mandel_catalog import matching_rows
    sources=source_index(catalog['scenes'])
    expected=matching_rows(batch['identity']['manifest']['scenes'],sources)
    observed=matching_rows(batch['rows'],sources)
    if set(expected)!=set(observed) or any(set(MODES)-set(r['modes']) for r in batch['rows']):
        raise ValueError('batch has not finished all requested modes')
    if {r['id'] for r in evidence['rows']} & {r['id'] for r in batch['rows']}:
        raise ValueError('batch overlaps previously reviewed evidence')
    settings=batch['identity']['settings']
    if settings.get('native_mc_samples') is not None or any(
            set(r.get('reference_overrides',{})) & {'DOF_samples','DOF_min_samples'}
            for r in batch['rows']):
        raise ValueError('reduced native sampling is screening-only, not authored gallery evidence')
    if settings['reference_backend']!='CPU' or set(settings['modes'])!=set(MODES) or settings['max_axis']!=300 or settings['samples']!=32:
        raise ValueError('batch must include native CPU references and both FPT modes')
    binaries=[digest for path,digest in batch['identity']['executables'].items() if Path(path).name=='fpt-metal']
    if len(binaries)!=1:raise ValueError('expected one FPT binary')
    complete=[r for r in batch['rows'] if all(r['modes'][m]['status']=='ok' for m in MODES)]
    held=[dict(id=r['id'],path=r['path'],sha256=r['sha256'],
        status={m:r['modes'][m]['status'] for m in MODES}) for r in batch['rows'] if r not in complete]
    if not complete:raise ValueError('no complete reference-backed captures to review')
    upstream={r['upstream_revision'] for r in evidence['rows']}
    if len(upstream)!=1:raise ValueError('ambiguous upstream source revision')
    manifest=dict(upstream_revision=next(iter(upstream)),renderer_revision=revision,
        production_sha256=binaries[0],bounces=settings['bounces'],rows=[dict(id=r['id'],source=r['path'],source_sha256=r['sha256'],
            native_reference_reduced=False,reference_overrides=r.get('reference_overrides',{}),
            captures={m:r['modes'][m]['capture'] for m in MODES}) for r in complete])
    refresh=dict(identity=dict(manifest=dict(scenes=complete),settings=settings,executables={'fpt-metal':binaries[0]}),rows=complete)
    new_evidence,new_assets=assemble(dict(rows=[]),dict(rows=complete),refresh,catalog,dict(rows=[]),manifest,revision)
    combined=dict(evidence,rows=evidence['rows']+new_evidence['rows'])
    paths=dict(rows=assets['rows']+new_assets['rows'])
    return combined,paths,dict(scope='Incomplete triplets remain unpromoted; failures are not hidden or counted as visually reviewed.',rows=held)


def published_images(previous):
    """Images of a committed showcase, each checked against that showcase's manifest."""
    manifest = json.loads((previous/'manifest.json').read_text())
    images = {}
    for name, digest in manifest['files'].items():
        if name.startswith('images/'):
            if sha256(previous/name) != digest:
                raise ValueError('previous showcase image changed since publication: '+name)
            images[name] = previous/name
    return images


def publish(catalog, reviews, evidence, assets, output, previous=None):
    sources = source_index(catalog['scenes'])
    decisions, proofs = validate_reviews(reviews,evidence,sources)
    if set(decisions) != set(proofs):
        raise ValueError('publication requires a decision for every captured row')
    local = validate_assets(evidence,assets,partial=previous is not None)
    prior = published_images(previous) if previous is not None else {}
    output.mkdir(parents=True,exist_ok=False)
    (output/'images').mkdir()
    accepted, held = [], []
    for key, proof in sorted(proofs.items()):
        row, review = sources[key], decisions[key]
        entry = (row,review,proof)
        (held if review['decision']=='needs-work' else accepted).append(entry)
        if row['id'] in local:
            height = max(proof['dimensions'][1],100)
            canvas = Image.new('RGB',(900,height+28),'#202326')
            draw = ImageDraw.Draw(canvas)
            names = ('Native CPU reference','FPT neutral geometry','FPT authored path')
            for col,mode in enumerate(MODES):
                with Image.open(local[row['id']]['paths'][mode]) as source:
                    im = source.convert('RGB')
                if mode=='mandel' and proof['native_reference_reduced']:
                    im = im.resize(tuple(proof['dimensions']),Image.Resampling.NEAREST)
                canvas.paste(im,(col*300+(300-im.width)//2,28+(height-im.height)//2))
                draw.text((col*300+8,8),names[col],fill='white')
            canvas.save(output/'images'/f'{row["id"]}.png',optimize=True)
            pair = Image.new('RGB',(360,120),'#202326')
            for col,mode in enumerate(('mandel','authored')):
                with Image.open(local[row['id']]['paths'][mode]) as source:
                    im=source.convert('RGB');im.thumbnail((178,120))
                pair.paste(im,(col*180+(180-im.width)//2,(120-im.height)//2))
            pair.save(output/'images'/f'{row["id"]}-thumb.webp',quality=85)
        else:
            # raw captures are gone; reuse the images published from them, verified by hash
            for name in (f'images/{row["id"]}.png', f'images/{row["id"]}-thumb.webp'):
                if name not in prior:
                    raise ValueError('no raw captures or verified published image for '+row['id'])
                shutil.copy2(prior[name], output/name)
        lines=[f'# {row["id"]}: {row["name"]}', '', '[Reviewed gallery](README.md) | [Needs-work audit](needs-work.md) | [Full catalogue](../mandel-catalog/README.md)', '',
            f'**{review["decision"]}**. {review["note"]}', '', f'![Native reference / FPT neutral / FPT authored](images/{row["id"]}.png)', '',
            f'FPT: {proof["dimensions"][0]}x{proof["dimensions"][1]}, 32 SPP; {proof["bounces"]}.',
            'Native CPU: authored sampling/lighting, not an identical integrator. Neutral FPT uses a white geometry control, not matched authored lighting.', '',
            f'Capture: **{proof["epoch"]}**, renderer revision `{proof["renderer_revision"]}`.',
            f'Renderer SHA256: `{proof["renderer_sha256"]}`.', '',
            f'Review: {review["reviewed_at"]}, {review["reviewer"]}. Geometry: {review["geometry"]}; illumination: {review["illumination"]}.', '',
            f'[Upstream scene]({row["source_url"]}) | Collection credit: {row["collection"]}',
            f'Source SHA256: `{row["sha256"]}`.', '',
            'This acceptance applies to these captures. It is not exact native parity or NAADF/CVOX certification. Colours, skies and unsupported volumes may differ. No new timing claim.','']
        if proof['native_reference_reduced']:
            lines += ['Native reference is only 96px max edge and enlarged; fine-detail parity is not established.','']
        if proof['reference_overrides']:
            lines += ['Reference-only overrides: `'+json.dumps(proof['reference_overrides'],sort_keys=True)+'`. No hidden crop or flip.','']
        lines += ['[Source/capture evidence](../mandel-catalog/review-evidence.json) | [Explicit decisions](../mandel-catalog/reviews.json)','']
        (output/f'{row["id"]}.md').write_text('\n'.join(lines))
    page_count = (len(accepted)+9)//10
    nav = ' | '.join(f'[Page {i+1}](page-{i+1:02d}.md)' for i in range(page_count))
    for start in range(0,len(accepted),10):
        lines=[f'# Reviewed Scenes: Page {start//10+1}', '',nav,'',
            'Each thumbnail: native left, FPT authored right. Open a scene for full neutral/beauty comparisons, limitations and capture settings.','']
        for row,review,proof in accepted[start:start+10]:
            lines += [f'## [{row["id"]}: {row["name"]}]({row["id"]}.md)', '',
                f'[![Native / FPT authored](images/{row["id"]}-thumb.webp)]({row["id"]}.md)', '',
                f'{review["decision"]} | {proof["epoch"]} | 32 SPP', '',review['note'],'']
        (output/f'page-{start//10+1:02d}.md').write_text('\n'.join(lines))
    lines=['# Needs-Work Audit','','[Reviewed gallery](README.md) | [All experimental scenes](../mandel-catalog/scenes.md)','',
        'These rows are deliberately excluded from the showcase. Historical reviewed-gallery membership does not override a failed visual gate.','']
    for row,review,proof in held:
        lines += [f'## [{row["id"]}: {row["name"]}]({row["id"]}.md)', '',
            f'[![Native / FPT authored](images/{row["id"]}-thumb.webp)]({row["id"]}.md)', '',review['note'],'']
    (output/'needs-work.md').write_text('\n'.join(lines))
    # Representative choices are explicit, not an image-metric quality ranking.
    preferred = [1,2,3,4,5,6,55,57,60,571,630,635]
    featured = [entry for key in preferred for entry in accepted if int(entry[0]['id'])==key]
    featured += [entry for entry in accepted if entry not in featured][:max(0,12-len(featured))]
    highlights = Image.new('RGB',(1080,max(150,150*((len(featured)+2)//3))),'#202326')
    draw = ImageDraw.Draw(highlights)
    for i,(row,review,proof) in enumerate(featured):
        x,y = (i%3)*360,(i//3)*150
        with Image.open(output/'images'/f'{row["id"]}-thumb.webp') as im:
            highlights.paste(im,(x,y))
        draw.text((x+7,y+125),textwrap.shorten(row['id']+' '+row['name'],width=48,placeholder='...'),fill='white')
    highlights.save(output/'highlights.jpg',quality=90)
    lines=['# Reviewed Mandel Showcase','','[Back to FPT Metal](../../README.md) | [Experimental catalogue](../mandel-catalog/README.md) | [Needs-work audit](needs-work.md)','',
        f'**{len(accepted)} visually accepted captures; {len(held)} reviewed cases held back.** Colours need not match exactly. Significant geometry and illumination failures are excluded.','',
        'Continuous FPT Metal only, not NAADF voxel output. Captures use 300px max edge and 32 FPT SPP; native sampling differs. Individual pages label historical versus refreshed captures, renderer identity and reduced/monoscopic references.', '',
        '![Twelve featured native / FPT comparisons](highlights.jpg)','',nav,'',
        '## Review Policy','','Promotion requires an explicit decision tied to the source hash and all three capture hashes. Execution success and gallery membership alone cannot promote a scene. Minor colour/material differences are allowed; missing geometry, uncertain framing and obscuring darkness need work.', '',
        'The original ranked-50 gallery remains an unchanged historical record. This showcase excludes known failures rather than hiding their caveats among successful thumbnails.', '',
        'Thumbnails are compressed navigation previews. Detail PNGs retain source RGB pixels at captured size, except labelled enlarged 96px native references. No colour correction, crop or orientation changes. Raw captures and generated voxel assets remain outside Git.','',
        '[Decisions](../mandel-catalog/reviews.json) | [Capture evidence](../mandel-catalog/review-evidence.json)','']
    (output/'README.md').write_text('\n'.join(lines))
    manifest=dict(version=1,reviewed=len(accepted),needs_work=len(held),featured=[r['id'] for r,_,_ in featured],
        files={p.relative_to(output).as_posix():sha256(p) for p in sorted(output.rglob('*')) if p.is_file()})
    (output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    return manifest



def main():
    parser=argparse.ArgumentParser(description=__doc__)
    sub=parser.add_subparsers(dest='command',required=True)
    a=sub.add_parser('assemble')
    for key in ('ranked','additional','refresh','ranked-manifest','additional-manifest'):
        a.add_argument('--'+key,type=Path,required=True)
    a.add_argument('--revision',required=True)
    a.add_argument('--output',type=Path,required=True)
    p=sub.add_parser('publish')
    for key in ('evidence','reviews','assets','output'):
        p.add_argument('--'+key,type=Path,required=True)
    p.add_argument('--previous',type=Path,help='Committed showcase whose verified images stand in for raw captures no longer on disk')
    preview=sub.add_parser('preview',help='Contact sheets for refreshed rows before visual decisions')
    for key in ('evidence','assets','output'):
        preview.add_argument('--'+key,type=Path,required=True)
    record=sub.add_parser('record',help='Bind an explicit assessment to immutable capture evidence')
    for key in ('assessment','evidence','output'):
        record.add_argument('--'+key,type=Path,required=True)
    record.add_argument('--legacy-gallery',type=Path)
    record.add_argument('--existing-reviews',type=Path)
    append=sub.add_parser('append-batch',help='Append finished batch evidence without changing previous decisions')
    for key in ('evidence','assets','batch','output'):
        append.add_argument('--'+key,type=Path,required=True)
    append.add_argument('--revision',required=True)
    for child in (a,p,preview,record,append):
        child.add_argument('--catalog',type=Path,default=ROOT/'docs/mandel-catalog/catalog.json')
    args=parser.parse_args()
    read=lambda p:json.loads(p.read_text())
    if args.command=='assemble':
        evidence,assets=assemble(read(args.ranked),read(args.additional),read(args.refresh),read(args.catalog),read(args.ranked_manifest),read(args.additional_manifest),args.revision)
        args.output.mkdir(parents=True,exist_ok=False)
        for name,result in (('evidence.json',evidence),('assets.json',assets)):
            (args.output/name).write_text(json.dumps(result,indent=2)+'\n')
    elif args.command=='append-batch':
        evidence,assets,held=append_batch(read(args.evidence),read(args.assets),read(args.batch),read(args.catalog),args.revision)
        args.output.mkdir(parents=True,exist_ok=False)
        for name,value in (('evidence.json',evidence),('assets.json',assets),('incomplete.json',held)):
            (args.output/name).write_text(json.dumps(value,indent=2)+'\n')
    elif args.command=='record':
        evidence=read(args.evidence)
        result=record_assessment(read(args.assessment),evidence,read(args.legacy_gallery) if args.legacy_gallery else None,
            sha256(args.evidence),read(args.existing_reviews) if args.existing_reviews else None)
        validate_reviews(result,evidence,source_index(read(args.catalog)['scenes']))
        with args.output.open('x') as stream:
            stream.write(json.dumps(result,indent=2)+'\n')
    elif args.command=='preview':
        from refresh_mandel_merge_sheets import sheets
        evidence,assets=read(args.evidence),read(args.assets)
        local=validate_assets(evidence,assets)
        rows=[]
        for row in evidence['rows']:
            if not row['epoch'].startswith('refreshed'):
                continue
            rows.append(dict(id=row['id'],path=row['path'],size=row['dimensions'],
                reference_reduced=row['native_reference_reduced'],
                modes={m:dict(capture=dict(path=local[row['id']]['paths'][m])) for m in MODES}))
        args.output.mkdir(parents=True,exist_ok=False)
        sheets(dict(rows=rows,gallery_title='REFRESH REVIEW | Native CPU / FPT geometry / FPT authored | 300px max edge, 32 FPT SPP'),args.output)
    else:
        result=publish(read(args.catalog),read(args.reviews),read(args.evidence),read(args.assets),args.output,args.previous)
        print(json.dumps({k:v for k,v in result.items() if k!='files'},indent=2))


if __name__=='__main__':
    main()
