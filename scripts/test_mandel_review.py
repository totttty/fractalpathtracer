import copy
import json
from pathlib import Path
import tempfile
import unittest

from PIL import Image
from mandel_review import apply_reviews, evidence_digest, validate_reviews
from mandel_catalog import sha256, source_index, CATALOG
from publish_mandel_review import append_batch, publish, record_assessment, validate_assets
from prepare_mandel_review_refresh import select


def fixture():
    row=dict(id='051',path='test.fract',sha256='a'*64,name='Test scene',collection='Test collection',
        source_url='https://example.invalid/test.fract',status='experimental',blockers=[],screening={})
    proof=dict(id='051',path='test.fract',sha256='a'*64,samples=32,dimensions=[300,225],bounces='scene default',
        renderer_revision='b'*40,renderer_sha256='c'*64,epoch='historical-test',reference_overrides={},
        native_reference_reduced=False,captures={m:dict(sha256='d'*64,dimensions=[300,225]) for m in ('mandel','geometry','authored')})
    review=dict(id='051',source_sha256='a'*64,evidence_sha256=evidence_digest(proof),
        decision='accepted-with-limitations',note='Geometry is clear; palette differs.',
        geometry='acceptable',illumination='acceptable',reviewer='test',reviewed_at='2026-09-11')
    return dict(scenes=[row],counts={},status_policy={}),dict(version=1,rows=[review]),dict(version=1,rows=[proof])


class VisualReviewTests(unittest.TestCase):
    def test_explicit_acceptance_independent_of_ranked_gallery(self):
        cat,reviews,evidence=fixture()
        result=apply_reviews(cat,reviews,evidence)
        self.assertEqual(result['scenes'][0]['status'],'reviewed')
        reviews['rows'][0]['decision']='needs-work'
        result=apply_reviews(cat,reviews,evidence)
        self.assertEqual(result['scenes'][0]['status'],'blocked')

    def test_capture_settings_source_and_decision_drift_fail_closed(self):
        for mutation in ('capture','samples','source','geometry','illumination','duplicate'):
            cat,reviews,evidence=fixture()
            if mutation=='capture': evidence['rows'][0]['captures']['authored']['sha256']='e'*64
            elif mutation=='samples': evidence['rows'][0]['samples']=1
            elif mutation=='source': evidence['rows'][0]['sha256']='e'*64
            elif mutation=='duplicate': reviews['rows'].append(copy.deepcopy(reviews['rows'][0]))
            else: reviews['rows'][0][mutation]='needs-work'
            with self.subTest(mutation=mutation),self.assertRaises(ValueError):
                validate_reviews(reviews,evidence,source_index(cat['scenes']))

    def test_missing_decision_never_grants_reviewed(self):
        cat,reviews,evidence=fixture()
        cat['scenes'][0]['status']='reviewed'
        reviews['rows']=[]
        self.assertEqual(apply_reviews(cat,reviews,evidence)['scenes'][0]['status'],'experimental')

    def test_appending_preserves_prior_decisions_but_rejects_changed_evidence(self):
        _,reviews,evidence=fixture()
        assessment=dict(version=1,evidence_sha256='a'*64,rows=[],reviewer='test',reviewed_at='2026-09-11')
        result=record_assessment(assessment,evidence,None,'a'*64,existing=reviews)
        self.assertEqual(result['rows'],reviews['rows'])
        evidence['rows'][0]['renderer_sha256']='e'*64
        with self.assertRaisesRegex(ValueError,'fresh assessment'):
            record_assessment(assessment,evidence,None,'a'*64,existing=reviews)

    def test_refresh_selection_rejects_unknown_or_duplicate_ids(self):
        manifest = dict(root='test',scenes=[dict(id='051')])
        report = dict(identity=dict(manifest=manifest))
        self.assertEqual(select(report,['051'])['scenes'],[dict(id='051')])
        for ids in (['052'],['051','051']):
            with self.assertRaises(ValueError):select(report,ids)

    def test_publisher_verifies_assets_and_labels_capture_epoch(self):
        cat,reviews,evidence=fixture()
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            paths={}
            for i,mode in enumerate(('mandel','geometry','authored')):
                path=root/f'{mode}.png'
                Image.new('RGB',(300,225),(i*40+10,25,30)).save(path)
                paths[mode]=str(path)
                evidence['rows'][0]['captures'][mode]['sha256']=sha256(path)
            reviews['rows'][0]['evidence_sha256']=evidence_digest(evidence['rows'][0])
            assets=dict(rows=[dict(id='051',paths=paths)])
            # No preferred highlight ID is required for an arbitrary new batch.
            result=publish(cat,reviews,evidence,assets,root/'published')
            self.assertEqual(result['reviewed'],1)
            self.assertIn('historical-test',(root/'published/051.md').read_text())
            self.assertIn('051.md',(root/'published/page-01.md').read_text())
            (root/'authored.png').write_bytes(b'changed')
            with self.assertRaisesRegex(ValueError,'hash mismatch'):
                validate_assets(evidence,assets)

    def test_republish_reuses_verified_images_when_raw_captures_are_gone(self):
        cat,reviews,evidence=fixture()
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            paths={}
            for i,mode in enumerate(('mandel','geometry','authored')):
                path=root/f'{mode}.png'
                Image.new('RGB',(300,225),(i*40+10,25,30)).save(path)
                paths[mode]=str(path)
                evidence['rows'][0]['captures'][mode]['sha256']=sha256(path)
            reviews['rows'][0]['evidence_sha256']=evidence_digest(evidence['rows'][0])
            publish(cat,reviews,evidence,dict(rows=[dict(id='051',paths=paths)]),root/'first')
            # raw captures deleted: only the published showcase remains
            for path in paths.values():
                Path(path).unlink()
            with self.assertRaisesRegex(ValueError,'asset map'):
                publish(cat,reviews,evidence,dict(rows=[]),root/'strict')
            result=publish(cat,reviews,evidence,dict(rows=[]),root/'second',previous=root/'first')
            self.assertEqual(result['reviewed'],1)
            for name in ('images/051.png','images/051-thumb.webp'):
                self.assertEqual(sha256(root/'second'/name),sha256(root/'first'/name))
            (root/'first/images/051.png').write_bytes(b'tampered')
            with self.assertRaisesRegex(ValueError,'changed since publication'):
                publish(cat,reviews,evidence,dict(rows=[]),root/'third',previous=root/'first')

    def test_append_batch_preserves_old_evidence_and_requires_complete_modes(self):
        cat,_,evidence=fixture()
        evidence['rows'][0]['upstream_revision']='f'*40
        cat['scenes'].append(dict(cat['scenes'][0],id='052',path='new.fract',sha256='b'*64))
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            paths={}
            modes={}
            for i,mode in enumerate(('mandel','geometry','authored')):
                path=root/f'{mode}.png'
                Image.new('RGB',(300,225),(40+i,50,60)).save(path)
                paths[mode]=str(path)
                digest=sha256(path)
                evidence['rows'][0]['captures'][mode]['sha256']=digest
                result=dict(status='ok',capture=dict(path=str(path),sha256=digest))
                if mode!='mandel':
                    meta=root/f'{mode}.json'
                    meta.write_text(json.dumps(dict(samples=32,scene_sha256='b'*64,width=300,height=225)))
                    result['metadata']=dict(path=str(meta),sha256=sha256(meta))
                modes[mode]=result
            assets=dict(rows=[dict(id='051',paths=paths)])
            row=dict(id='052',path='new.fract',sha256='b'*64,size=[300,225],modes=modes)
            report=dict(rows=[row],identity=dict(manifest=dict(scenes=[{k:row[k] for k in ('id','path','sha256')}]),
                settings=dict(reference_backend='CPU',modes=list(modes),max_axis=300,samples=32,bounces='default'),
                executables={'fpt-metal':'d'*64}))
            combined,_,held=append_batch(evidence,assets,report,cat,'c'*40)
            self.assertEqual(combined['rows'][0],evidence['rows'][0])
            self.assertEqual(combined['rows'][1]['id'],'052')
            self.assertEqual(held['rows'],[])
            # prior rows whose raw captures are gone may be omitted from the asset map
            _,partial,_=append_batch(evidence,dict(rows=[]),report,cat,'c'*40)
            self.assertEqual([r['id'] for r in partial['rows']],['052'])
            report['identity']['settings']['native_mc_samples']=16
            with self.assertRaisesRegex(ValueError,'screening-only'):
                append_batch(evidence,assets,report,cat,'c'*40)
            report['identity']['settings'].pop('native_mc_samples')
            row['reference_overrides']={'DOF_samples':16}
            with self.assertRaisesRegex(ValueError,'screening-only'):
                append_batch(evidence,assets,report,cat,'c'*40)
            row.pop('reference_overrides')
            failed=dict(row,id='053',path='failed.fract',sha256='e'*64,
                modes={m:dict(status='timeout') for m in modes})
            cat['scenes'].append(dict(cat['scenes'][0],id='053',path='failed.fract',sha256='e'*64))
            report['rows'].append(failed)
            report['identity']['manifest']['scenes'].append({k:failed[k] for k in ('id','path','sha256')})
            combined,_,held=append_batch(evidence,assets,report,cat,'c'*40)
            self.assertEqual([r['id'] for r in combined['rows']],['051','052'])
            self.assertEqual([r['id'] for r in held['rows']],['053'])
            report['rows'][0]['modes'].pop('mandel')
            with self.assertRaisesRegex(ValueError,'not finished'):
                append_batch(evidence,assets,report,cat,'c'*40)

    def test_checked_in_review_evidence_is_bound_to_catalogue(self):
        if not CATALOG.with_name('reviews.json').exists():
            self.skipTest('review records not generated yet')
        cat=json.loads(CATALOG.read_text())
        reviews=json.loads(CATALOG.with_name('reviews.json').read_text())
        evidence=json.loads(CATALOG.with_name('review-evidence.json').read_text())
        decisions,_=validate_reviews(reviews,evidence,source_index(cat['scenes']))
        self.assertEqual(len(decisions),len(evidence['rows']))
        self.assertGreaterEqual(len(decisions),100)
        for row in cat['scenes']:
            if row['status']=='reviewed':
                self.assertIn(int(row['id']),decisions)


if __name__=='__main__':
    unittest.main()
