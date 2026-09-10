import unittest

from run_mandel_displacement_controls import neutral_perlin_color_overrides


class PerlinControlTests(unittest.TestCase):
    def test_only_existing_enabled_noise_materials_change_colour(self):
        source = ('[main_parameters]\nmat1_perlin_noise_enable true;\n'
                  'mat2_perlin_noise_enable false;\nmat3_shading 0.5;\n'
                  '[fractal_1]\nmat4_perlin_noise_enable true;\n')
        self.assertEqual(neutral_perlin_color_overrides(source), {
            'mat1_perlin_noise_color_enable': '1',
            'mat1_perlin_noise_color_intensity': '0',
        })

    def test_control_never_changes_displacement_or_enables_noise(self):
        source = ('[main_parameters]\nmat1_perlin_noise_enable true;\n'
                  'mat1_perlin_noise_displacement_enable true;\n'
                  'mat1_perlin_noise_displacement_intensity 0.1;\n')
        result = neutral_perlin_color_overrides(source)
        self.assertFalse(any('displacement' in key for key in result))
        self.assertNotIn('mat1_perlin_noise_enable', result)
        self.assertEqual(neutral_perlin_color_overrides('[main_parameters]\n'), {})


if __name__ == '__main__':
    unittest.main()
