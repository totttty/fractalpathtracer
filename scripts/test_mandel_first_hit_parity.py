import unittest

import numpy as np

from run_mandel_first_hit_parity import compare_hits


class FirstHitParityTests(unittest.TestCase):
    def test_coordinate_permutation_and_world_scale(self):
        native = [[1, 1, 2, 3, 4, .01, .5, 7]]
        result = compare_hits(native, [[10, 30, 20, 1]], [0, 0, 0], 10)
        self.assertEqual(result['common_hits'], 1)
        self.assertEqual(result['median_common_error_in_thresholds'], 0)
        self.assertEqual(result['rows'][0]['native_steps'], 7)
        self.assertAlmostEqual(result['rows'][0]['native_travel'], np.sqrt(14))

    def test_hit_miss_buckets_and_threshold_units(self):
        native = np.zeros((4, 8))
        metal = np.zeros((4, 4))
        native[:, 6] = .5
        native[:2, 0] = 1
        metal[[0, 2], 3] = 1
        metal[0, 0] = 20
        result = compare_hits(native, metal, [0, 0, 0], 10)
        for key in ('common_hits', 'native_only', 'metal_only', 'neither'):
            self.assertEqual(result[key], 1)
        self.assertEqual(result['median_common_error_in_thresholds'], 4)
        self.assertEqual(result['common_over_one_threshold'], 1)

    def test_no_common_hits_has_no_median(self):
        result = compare_hits(np.zeros((1, 8)), np.zeros((1, 4)), [0, 0, 0], 1)
        self.assertIsNone(result['median_common_error_in_thresholds'])
        self.assertEqual(result['common_over_one_threshold'], 0)

    def test_invalid_records_rejected(self):
        for native, metal, scale in (
                (np.zeros((1, 7)), np.zeros((1, 4)), 1),
                (np.zeros((1, 8)), np.zeros((2, 4)), 1),
                (np.zeros((1, 8)), np.full((1, 4), np.nan), 1),
                (np.zeros((1, 8)), np.zeros((1, 4)), 0)):
            with self.subTest(scale=scale, shape=native.shape):
                with self.assertRaises(ValueError):
                    compare_hits(native, metal, [0, 0, 0], scale)
