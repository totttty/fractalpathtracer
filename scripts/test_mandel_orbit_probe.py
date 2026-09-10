import unittest
import numpy as np
from run_mandel_orbit_probe import completed_iterations, shifted_points


class OrbitProbeTests(unittest.TestCase):
    def test_native_exhaustion_is_not_an_extra_iteration(self):
        records = np.array([[8., 11, 1, 0, 0, 0], [8., 10, 0, 0, 0, 0]])
        np.testing.assert_array_equal(completed_iterations(records), [10, 10])
        np.testing.assert_array_equal(completed_iterations(records[None]), [[10, 10]])

    def test_shift_axis_order_and_rounding_are_separate(self):
        exact, rounded = shifted_points(np.array([[1., 2., 3.]]), np.array([1e-8]))
        np.testing.assert_array_equal(rounded, [[[1, 2, 3], [1, 2, 3], [1, 2, 3]]])
        self.assertGreater(exact[0, 0, 0], rounded[0, 0, 0])
        np.testing.assert_array_equal(exact[0, 0, 1:], [2., 3.])
        self.assertGreater(exact[0, 1, 1], 2.)
        self.assertGreater(exact[0, 2, 2], 3.)


if __name__ == '__main__':
    unittest.main()
