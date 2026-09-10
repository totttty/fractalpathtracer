import unittest
import numpy as np
from run_mandel_normal_controls import compare


class NormalControlTests(unittest.TestCase):
    def test_native_signed_normals_are_mapped_once(self):
        native = np.array([[[2, 0, 0, 1], [2, 0, 1, 0]]], dtype=float)
        fpt = np.zeros((1, 2, 20))
        fpt[..., 3], fpt[..., 7] = 2048, 1
        fpt[0, 0, 5], fpt[0, 1, 6] = 1, 1
        metrics, *_ = compare(native, fpt, 1024)
        self.assertEqual(metrics['normal_degrees_depth_matched']['median'], 0)
        self.assertEqual(metrics['normal_degrees_depth_matched']['count'], 2)

    def test_depth_mismatch_is_not_a_normal_regression(self):
        native = np.array([[[2, 1, 0, 0], [2, 1, 0, 0]]], dtype=float)
        fpt = np.zeros((1, 2, 20))
        fpt[..., 3], fpt[..., 7], fpt[..., 4] = 2, 1, 1
        fpt[0, 1, 3], fpt[0, 1, 4] = 3, -1
        metrics, *_ = compare(native, fpt, 1)
        self.assertEqual(metrics['normal_degrees_depth_matched']['count'], 1)
        self.assertEqual(metrics['normal_degrees_depth_matched']['median'], 0)
        self.assertEqual(metrics['normal_degrees_all']['median'], 90)

    def test_old_structural_record_stride_is_rejected(self):
        with self.assertRaises(ValueError):
            compare(np.zeros((1, 1, 4)), np.zeros((1, 1, 16)), 1024)
