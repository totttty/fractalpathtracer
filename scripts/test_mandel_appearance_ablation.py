import unittest
import numpy as np
from run_mandel_appearance_ablation import variants
from run_mandel_normal_position_probe import angles, distribution


class AppearanceAblationTests(unittest.TestCase):
    def test_single_bounce_only_changes_path_depth(self):
        rows = {name: (parent, values, cap) for name, parent, values, cap in variants()}
        self.assertEqual(rows['one-bounce'][0], 'no-post')
        self.assertEqual(rows['one-bounce'][1], rows['no-post'][1])
        self.assertEqual(rows['no-post'][2], 0)
        self.assertEqual(rows['one-bounce'][2], 1)

    def test_control_graph_preserves_authored_geometry_and_light_settings(self):
        seen = set()
        for name, parent, settings, _ in variants():
            if parent:
                self.assertIn(parent, seen)
            seen.add(name)
            for key in settings:
                self.assertFalse(key.startswith(('camera', 'target', 'light', 'formula', 'DE_')))

    def test_normal_angles_are_normalized_and_orientation_sensitive(self):
        a = np.array([[2., 0., 0.]] * 3)
        b = np.array([[1., 0., 0.], [0., 1., 0.], [-1., 0., 0.]])
        np.testing.assert_allclose(angles(a, b), [0., 90., 180.])
        self.assertEqual(distribution(np.array([1., 2., 3.]))['median'], 2.)


if __name__ == '__main__':
    unittest.main()
