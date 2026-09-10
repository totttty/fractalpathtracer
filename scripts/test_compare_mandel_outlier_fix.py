import contextlib
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from PIL import Image
from compare_mandel_outlier_fix import main
from run_release_canaries import sha256


class ComparisonTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.image = self.root / 'capture.png'
        Image.new('RGB', (300, 158), 'red').save(self.image)
        result = dict(status='ok', capture=dict(path=str(self.image), sha256=sha256(self.image)))
        self.report = dict(identity=dict(settings=dict(samples=32, max_axis=300)),
                           rows=[dict(id='001', path='example.fract', sha256='source',
                                      size=[300, 158], modes=dict(mandel=result, authored=result))])
        self.before = self.root / 'before.json'
        self.after = self.root / 'after.json'
        self.notes = self.root / 'notes.json'
        self.before.write_text(json.dumps(self.report))
        self.after.write_text(json.dumps(self.report))
        self.notes.write_text(json.dumps({'001': 'Review only.'}))
        self.out = self.root / 'review'

    def run_review(self):
        argv = ['compare', '--baseline', str(self.before), '--candidate', str(self.after),
                '--notes', str(self.notes), '--output', str(self.out), '--scenes', '001']
        with patch('sys.argv', argv), contextlib.redirect_stdout(io.StringIO()):
            main()

    def test_exact_comparison_and_hash_manifest(self):
        self.run_review()
        summary = json.loads((self.out / 'summary.json').read_text())
        self.assertEqual(summary['rows'][0]['after']['changed_pixels'], 0)
        self.assertEqual(summary['pages']['comparison-01.png'], sha256(self.out / 'comparison-01.png'))
        self.assertEqual(sha256(self.image), self.report['rows'][0]['modes']['authored']['capture']['sha256'])

    def test_changed_source_rejected_before_output(self):
        self.report['rows'][0]['sha256'] = 'different'
        self.after.write_text(json.dumps(self.report))
        with self.assertRaisesRegex(ValueError, 'identity'):
            self.run_review()
        self.assertFalse(self.out.exists())

    def test_changed_capture_rejected(self):
        Image.new('RGB', (300, 158), 'blue').save(self.image)
        with self.assertRaisesRegex(ValueError, 'capture'):
            self.run_review()

    def test_mismatched_dimensions_rejected_even_with_valid_hash(self):
        self.report['rows'][0]['size'] = [300, 300]
        for path in (self.before, self.after):
            path.write_text(json.dumps(self.report))
        with self.assertRaisesRegex(ValueError, 'dimensions'):
            self.run_review()


if __name__ == '__main__':
    unittest.main()
