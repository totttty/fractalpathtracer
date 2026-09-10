import copy
import json
from pathlib import Path
import tempfile
import unittest

from PIL import Image

from publish_mandel_gallery import MODES, publish, validate_report
from run_release_canaries import sha256


class GalleryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        image = self.root/'image.png'
        Image.new('RGB', (300, 169), '#123456').save(image)
        meta = self.root/'meta.json'
        meta.write_text('{}')
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
