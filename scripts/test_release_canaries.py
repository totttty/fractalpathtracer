import hashlib
import sys
import tempfile
import unittest
from pathlib import Path

from run_release_canaries import difference, execute, mandel_reference_command, scene_dimensions, validate_manifest
from PIL import Image


class CanaryTests(unittest.TestCase):
    def test_reference_forces_cpu_not_saved_app_backend(self):
        command = mandel_reference_command(Path('/bin/mandel'), Path('/scene.fract'),
                                           (300,225), Path('/out.png'))
        self.assertEqual(command[command.index('-O')+1], 'opencl_enabled=0')
        self.assertEqual(command[command.index('-r')+1], '300x225')
        self.assertIn('-C', command)

    def test_capture_environment_is_explicit(self):
        with tempfile.TemporaryDirectory() as temp:
            folder = Path(temp)/'capture'
            execute([sys.executable, '-c', 'import os; print(os.environ["FPT_TILE_GATE"])'],
                    folder, 10, env={'FPT_TILE_GATE': 'isolated'})
            self.assertEqual((folder/'stdout.log').read_text().strip(), 'isolated')
            self.assertTrue((folder/'command.json').is_file())

    def test_aspect_preserved(self):
        self.assertEqual(scene_dimensions('image_width 1920;\nimage_height 1080;', 300), (300, 169))
        self.assertEqual(scene_dimensions('image_width 600;\nimage_height 800;', 300), (225, 300))

    def test_native_defaults(self):
        self.assertEqual(scene_dimensions('# only modified parameters\n', 300), (300, 225))

    def test_invalid_dimensions(self):
        with self.assertRaises(ValueError):
            scene_dimensions('image_width 0;\nimage_height 800;', 300)
        with self.assertRaises(ValueError):
            scene_dimensions('image_width invalid;', 300)

    def test_manifest_requires_exact_source(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source = root/'scene.fract'
            source.write_bytes(b'scene')
            row = dict(id='01', path=source.name, sha256=hashlib.sha256(b'scene').hexdigest())
            manifest = dict(version=1, scenes=[row])
            validate_manifest(manifest, root)
            source.write_bytes(b'changed')
            with self.assertRaises(ValueError):
                validate_manifest(manifest, root)
            source.write_bytes(b'scene')
            with self.assertRaises(ValueError):
                validate_manifest(dict(version=1, scenes=[row, row]), root)
            with self.assertRaises(ValueError):
                validate_manifest(dict(version=1, scenes=[dict(row, path='../scene.fract')]), root)

    def test_pixel_gate(self):
        with tempfile.TemporaryDirectory() as temp:
            a, b = Path(temp)/'a.png', Path(temp)/'b.png'
            Image.new('RGB', (2, 2), 'white').save(a)
            Image.new('RGB', (2, 2), 'white').save(b)
            self.assertEqual(difference(a, b)['changed_pixels'], 0)
            image = Image.new('RGB', (2, 2), 'white')
            image.putpixel((0, 0), (0, 0, 0))
            image.save(b)
            self.assertEqual(difference(a, b)['changed_pixels'], 1)
            Image.new('RGB', (3, 2)).save(b)
            with self.assertRaises(ValueError):
                difference(a, b)


if __name__ == '__main__':
    unittest.main()
