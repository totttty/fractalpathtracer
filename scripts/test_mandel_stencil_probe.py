import unittest
import numpy as np
from run_mandel_stencil_probe import stencil, normals


class StencilTests(unittest.TestCase):
    def test_pairs_preserve_axis_and_sign(self):
        points = np.array([[1., 2., 3.]])
        result = stencil(points, np.array([.25]))
        np.testing.assert_allclose(result[0], [
            [1.25,2,3], [.75,2,3], [1,2.25,3], [1,1.75,3], [1,2,3.25], [1,2,2.75]])

    def test_linear_field_recovers_normal(self):
        points = np.array([[1., 2., 3.], [-1., -2., -3.]])
        sample_points = stencil(points, np.array([.1, .3]))
        gradient = np.array([2., -3., 6.])
        values = sample_points @ gradient
        np.testing.assert_allclose(normals(values), np.tile(gradient / 7., (2,1)))

    def test_collapsed_stencil_is_not_invented_normal(self):
        np.testing.assert_array_equal(normals(np.ones((2,6))), np.zeros((2,3)))


if __name__ == '__main__':
    unittest.main()
