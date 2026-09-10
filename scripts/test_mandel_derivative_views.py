import unittest
import numpy as np
from run_mandel_derivative_views import camera_offsets


class DerivativeCameraTests(unittest.TestCase):
    def test_translation_preserves_target_direction_and_distance(self):
        text = '[main_parameters]\ncamera 1 2 3;\ntarget 1 4 3;\ncamera_top 0 0 1;\n'
        for name, pair in camera_offsets(text).items():
            c,t = [np.array([float(x) for x in pair[k].split()]) for k in ('camera','target')]
            np.testing.assert_allclose(t-c, [0,2,0])
            if name == 'forward': np.testing.assert_allclose(c, [1,2.3,3])
            if name == 'lateral': np.testing.assert_allclose(c, [1.3,2,3])

    def test_rejects_zero_camera_direction(self):
        text = '[main_parameters]\ncamera 1 2 3;\ntarget 1 2 3;\ncamera_top 0 0 1;\n'
        with self.assertRaises(ValueError): camera_offsets(text)

    def test_rejects_parallel_up_vector(self):
        text = '[main_parameters]\ncamera 1 2 3;\ntarget 1 4 3;\ncamera_top 0 1 0;\n'
        with self.assertRaises(ValueError): camera_offsets(text)


if __name__ == '__main__': unittest.main()
