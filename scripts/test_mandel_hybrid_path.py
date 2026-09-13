import tempfile
import unittest
from pathlib import Path

from PIL import Image

from run_mandel_hybrid_path import metrics, verify_reference


class HybridPathReportTests(unittest.TestCase):
    def test_exact_images(self):
        with tempfile.TemporaryDirectory() as directory:
            p = Path(directory)/'a.png'
            Image.new('RGB', (3, 2), (10, 20, 30)).save(p)
            self.assertEqual(metrics(p, p), dict(mae_255=0.0, rmse_255=0.0, changed_pixels=0))

    def test_uniform_delta(self):
        with tempfile.TemporaryDirectory() as directory:
            a, b = Path(directory)/'a.png', Path(directory)/'b.png'
            Image.new('RGB', (3, 2), (10, 10, 10)).save(a)
            Image.new('RGB', (3, 2), (20, 20, 20)).save(b)
            self.assertEqual(metrics(a, b), dict(mae_255=10.0, rmse_255=10.0, changed_pixels=6))

    def test_dimensions_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            a, b = Path(directory)/'a.png', Path(directory)/'b.png'
            Image.new('RGB', (3, 2)).save(a)
            Image.new('RGB', (4, 2)).save(b)
            with self.assertRaisesRegex(ValueError, 'dimensions'):
                metrics(a, b)

    def test_untrusted_reference_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            p = Path(directory)/'a.png'
            Image.new('RGB', (300, 225)).save(p)
            with self.assertRaisesRegex(ValueError, 'validated'):
                verify_reference(p, 'geometry')


if __name__ == '__main__':
    unittest.main()
