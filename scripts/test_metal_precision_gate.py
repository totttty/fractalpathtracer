import math
import struct
import unittest

from run_metal_precision_gate import compare, samples, split3


class PrecisionGateTests(unittest.TestCase):
    def test_split_preserves_double_inputs(self):
        for a, b in samples():
            self.assertEqual(math.fsum(split3(a)), a)
            self.assertEqual(math.fsum(split3(b)), b)

    def test_nonfinite_rejected(self):
        for value in [math.inf, -math.inf, math.nan]:
            with self.assertRaises(ValueError):
                split3(value)

    def test_overflow_rejected(self):
        with self.assertRaises(ValueError):
            split3(1e300)

    def test_truncated_output_rejected(self):
        with self.assertRaises(ValueError):
            compare([(1., 1.)], b"\0" * 31)

    def test_nonfinite_output_rejected(self):
        with self.assertRaises(ValueError):
            compare([(1., 1.)], struct.pack("<8f", *([math.nan] * 8)))

    def test_failed_arithmetic_fails_gate(self):
        self.assertFalse(compare([(1., 1.)], b"\0" * 32)["passed"])

    def test_exact_output_passes(self):
        self.assertTrue(compare([(1., 1.)], struct.pack("<8f", 2, 0, 1, 0, 1, 0, 1, 0))["passed"])


if __name__ == "__main__":
    unittest.main()
