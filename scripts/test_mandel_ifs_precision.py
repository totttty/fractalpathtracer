import tempfile
from pathlib import Path
import struct
import unittest
import numpy as np

from run_mandel_ifs_precision import (config_values, import_formula, matrix,
    pack_inputs, render_kernel, round_config, scalar, vector)
from review_mandel_ifs_precision import metrics


class IfsPrecisionTests(unittest.TestCase):
    def test_split_preserves_camera_and_tiny_offsets(self):
        values = [-1.08473976030058, .115304126733863, .961518346656556]
        terms = struct.unpack('<9f', pack_inputs([values]))
        for i, value in enumerate(values):
            self.assertEqual(sum(terms[i*3:i*3+3]), value)

    def test_nonfinite_and_shape_rejected(self):
        for value in (float('nan'), float('inf'), 1e300):
            with self.assertRaises(ValueError):
                scalar(value)
        with self.assertRaises(ValueError):
            vector([0, 1])
        with self.assertRaises(ValueError):
            matrix([1, 0, 0])

    def test_unknown_formula_fails_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'formula.cpp'
            path.write_text('void other_formula() {}')
            with self.assertRaisesRegex(ValueError, 'unsupported upstream'):
                import_formula(path, {})

    def test_config_duplicate_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'config.tsv'
            path.write_text('scale\t3\nscale\t4\n')
            with self.assertRaisesRegex(ValueError, 'duplicate'):
                config_values(path)

    def test_orbit_controls_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'config.tsv'
            path.write_text('bailout\t100\t1\ncontrols\t250\t1\n')
            with self.assertRaisesRegex(ValueError, 'orbit controls'):
                config_values(path)

    def test_render_template_expands(self):
        source = render_kernel(dict(camera=[1,2,3],controls=[250,1,10000,.75,1,1,10]),169)
        self.assertNotIn('@', source)
        self.assertIn('steps < 10000', source)
        self.assertIn('pos - direction*step', source)

    def test_depth_does_not_hide_misses(self):
        result=metrics(np.array([[1.,2.,1e20]]),np.array([[1.,0.,3.]]),np.array([[True,False,True]]))
        self.assertEqual(result['visible_misses'],1)
        self.assertEqual(result['visible_extras'],1)
        self.assertEqual(result['depth_median_relative'],0)
        self.assertEqual(result['mutually_visible'],1)

    def test_nonfinite_depth_rejected(self):
        with self.assertRaises(ValueError):
            metrics(np.ones((1,1)),np.array([[float('nan')]]),np.ones((1,1),dtype=bool))

    def test_rounding_experiments_are_isolated(self):
        original=dict(camera=[1.0000000001],target=[2.0000000001],scale=[3.0000000001],controls=[4.0000000001])
        camera=round_config(original,'camera')
        self.assertEqual(camera['camera'],[1.])
        self.assertEqual(camera['scale'],original['scale'])
        formula=round_config(original,'formula')
        self.assertEqual(formula['scale'],[3.])
        self.assertEqual(formula['camera'],original['camera'])
        self.assertEqual(formula['controls'],original['controls'])
        self.assertEqual(round_config(original,'none'),original)
        self.assertNotEqual(original['camera'],[1.])


if __name__ == '__main__':
    unittest.main()
