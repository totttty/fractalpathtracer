import json
import tempfile
import unittest
from pathlib import Path

import numpy as np

from render_fptvox7_parity_sheets import load_fpt_structural


class StructuralLayoutTests(unittest.TestCase):
    def fixture(self, root, version, stride):
        path = root / "hits.bin"
        values = np.arange(4 * stride, dtype="<f4").reshape(2, 2, stride)
        values[:, :, 7] = 1
        values.tofile(path)
        Path(f"{path}.json").write_text(json.dumps({
            "format": "FptStructuralDiagnostic", "version": version,
            "record_bytes": stride * 4, "width": 2, "height": 2,
            "byte_order": "little-endian", "row_order": "top-to-bottom",
        }))
        return path, values

    def test_old_and_current_layout_preserve_pixel_fields(self):
        for version, stride in [(2, 16), (3, 20)]:
            with self.subTest(version=version), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                path, values = self.fixture(root, version, stride)
                result = load_fpt_structural(root / "unused.png", path, 2)
                np.testing.assert_array_equal(result["depth"], values[:, :, 3])
                np.testing.assert_array_equal(result["material"], values[:, :, 12:15])

    def test_version_stride_mismatch_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            path, _ = self.fixture(Path(temp), 2, 20)
            with self.assertRaises(RuntimeError):
                load_fpt_structural(Path(temp) / "unused.png", path, 2)

    def test_truncated_record_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            path, _ = self.fixture(Path(temp), 3, 20)
            path.write_bytes(path.read_bytes()[:-1])
            with self.assertRaises(RuntimeError):
                load_fpt_structural(Path(temp) / "unused.png", path, 2)

    def test_wrong_dimensions_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            path, _ = self.fixture(Path(temp), 3, 20)
            with self.assertRaises(RuntimeError):
                load_fpt_structural(Path(temp) / "unused.png", path, 3)


if __name__ == "__main__":
    unittest.main()
