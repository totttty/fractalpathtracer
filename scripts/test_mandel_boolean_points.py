import unittest

from run_mandel_boolean_points import distance_errors


class BooleanPointTests(unittest.TestCase):
    def test_reports_threshold_units_not_relative_distance(self):
        result = distance_errors([0, 4, 8], [1, 5, 12], [.5, 2, 4])
        self.assertEqual(result['median_error_in_thresholds'], 1)
        self.assertEqual(result['over_one_threshold'], 1)

    def test_scale_invariance_and_zero_distance(self):
        baseline = distance_errors([0, 1], [0, 2], [.1, .5])
        self.assertEqual(baseline, distance_errors([0, 1024], [0, 2048], [102.4, 512]))

    def test_invalid_inputs_rejected(self):
        for args in [([], [], []), ([0], [0, 1], [1]),
                     ([0], [float('nan')], [1]), ([0], [1], [0]),
                     ([0], [1], [-1]), ([[0]], [[1]], [[1]])]:
            with self.subTest(args=args), self.assertRaises(ValueError):
                distance_errors(*args)
