import copy
import json
from pathlib import Path
import tempfile
import unittest

from PIL import Image

from publish_mandel_gallery import MODES, publish, validate_report, from_support_report
from run_release_canaries import sha256


class GalleryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        image = self.root/'image.png'
        Image.new('RGB', (300, 169), '#123456').save(image)
        meta = self.root/'meta.json'
        meta.write_text(json.dumps(dict(samples=32,width=300,height=169,scene_sha256='test-source')))
        asset = {'path': str(image), 'sha256': sha256(image), 'rgb_sha256': 'test-pixels'}
        result = {'status': 'ok', 'capture': asset,
                  'metadata': {'path': str(meta), 'sha256': sha256(meta)},
                  'pipeline': {'samples': 32}}
        self.report = dict(samples=32, max_axis=300, bounces='default', environment={},
                           production_sha256='test-binary', rows=[])
        for i in range(1, 51):
            self.report['rows'].append(dict(
                id=f'{i:02d}', path=f'test scene {i}.fract', sha256='test-source',
                size=[300, 169], reference_reduced=False, note='',
                modes={m: copy.deepcopy(result) for m in MODES}))

    def batch(self):
        row=copy.deepcopy(self.report['rows'][0])
        row['id']='051'
        manifest=dict(version=1,provisional=False,scenes=[{k:row[k] for k in ('id','path','sha256')}])
        support=dict(identity=dict(manifest=manifest,
            settings=dict(modes=list(MODES),reference_backend='CPU',samples=32,max_axis=300,bounces='default'),
            executables={'/test/fpt-metal':'binary-sha'},environment={}),rows=[row])
        return manifest,support

    def test_batch_requires_complete_source_matched_cpu_captures(self):
        manifest,support=self.batch()
        report=from_support_report(support,manifest)
        validate_report(report,manifest)
        wrong=copy.deepcopy(manifest)
        wrong['scenes'][0]['sha256']='wrong'
        with self.assertRaisesRegex(ValueError,'source identity'):
            validate_report(report,wrong)
        support['identity']['settings']['reference_backend']='not_requested'
        with self.assertRaisesRegex(ValueError,'CPU'):
            from_support_report(support,manifest)
        provisional=copy.deepcopy(manifest)
        provisional['provisional']=True
        with self.assertRaisesRegex(ValueError,'non-provisional'):
            validate_report(report,provisional)

    def test_batch_gallery_does_not_inherit_cached_ranked50_claims(self):
        manifest,support=self.batch()
        report=from_support_report(support,manifest)
        output=self.root/'batch-gallery'
        result=publish(report,output,'a'*40,'b'*40,manifest)
        text=(output/'README.md').read_text()
        self.assertIn('fresh native',text)
        self.assertIn('| 051 |',text)
        self.assertNotIn('three small native',text)
        self.assertNotIn('96 pixels',text)
        self.assertNotIn('deep-zoom geometry failure',text)
        self.assertIn('Rows 01-01',text)
        self.assertIn('Fresh CPU',result['native_references'])
        self.assertTrue((output/'scenes-01-01.png').is_file())

    def test_review_notes_are_bound_to_source_hash_and_complete_batch(self):
        manifest,support=self.batch()
        report=from_support_report(support,manifest)
        with self.assertRaisesRegex(ValueError,'exactly'):
            publish(report,self.root/'bad-notes','a'*40,'b'*40,manifest,{})
        notes={'051':dict(sha256='wrong',note='Darker than native.')}
        with self.assertRaisesRegex(ValueError,'source mismatch'):
            publish(report,self.root/'bad-notes','a'*40,'b'*40,manifest,notes)
        notes['051']['sha256']=report['rows'][0]['sha256']
        result=publish(report,self.root/'noted-gallery','a'*40,'b'*40,manifest,notes)
        self.assertEqual(result['rows'][0]['note'],'Darker than native.')

    def test_rejects_missing_scene(self):
        self.report['rows'].pop()
        with self.assertRaisesRegex(ValueError, 'exactly'):
            validate_report(self.report)

    def test_rejects_changed_capture_and_incomplete_mode(self):
        self.report['rows'][0]['modes']['authored']['status'] = 'timeout'
        with self.assertRaisesRegex(ValueError, 'incomplete'):
            validate_report(self.report)
        self.report['rows'][0]['modes']['authored']['status'] = 'ok'
        self.report['rows'][0]['modes']['authored']['capture']['sha256'] = 'wrong'
        with self.assertRaisesRegex(ValueError, 'hash mismatch'):
            validate_report(self.report)

    def test_rejects_wrong_dimensions(self):
        self.report['rows'][0]['size'] = [300, 200]
        with self.assertRaisesRegex(ValueError, 'dimensions mismatch'):
            validate_report(self.report)

    def test_metadata_samples_are_checked_not_only_summary_claims(self):
        metadata=self.report['rows'][0]['modes']['authored']['metadata']
        path=Path(metadata['path'])
        data=json.loads(path.read_text())
        data['samples']=1
        path.write_text(json.dumps(data))
        for row in self.report['rows']:
            for mode in ('geometry','authored'):
                row['modes'][mode]['metadata']['sha256']=sha256(path)
        with self.assertRaisesRegex(ValueError,'metadata source/dimensions/samples'):
            validate_report(self.report)

    def test_portable_gallery_is_explicit_about_limits(self):
        output = self.root/'gallery'
        manifest = publish(self.report, output, 'a'*40, 'b'*40)
        self.assertEqual(len(manifest['rows']), 50)
        self.assertEqual(len(manifest['sheets']), 6)
        data = (output/'manifest.json').read_text()
        self.assertNotIn(str(self.root), data)
        self.assertFalse(json.loads(data)['visual_parity_certified'])
        self.assertEqual(json.loads(data)['renderer_revision'], 'b'*40)
        self.assertIn('illumination gap', manifest['rows'][31]['note'])
        self.assertEqual(self.report['rows'][31]['note'], '')
        self.assertIn('major deep-zoom geometry failure', (output/'README.md').read_text())
        for name, expected in manifest['sheets'].items():
            self.assertEqual(sha256(output/name), expected)
        with Image.open(output/'scenes-01-10.png') as sheet:
            top = 70+(300-169)//2
            authored = sheet.crop((600, top, 900, top+169))
            with Image.open(self.root/'image.png') as original:
                self.assertEqual(authored.tobytes(), original.tobytes())
