import unittest
from audit_mandel_lights import inventory


class LightInventoryTests(unittest.TestCase):
    def test_modern_directional_auxiliary_is_not_a_point_light(self):
        row = inventory('# version 2.33\n[main_parameters]\nlight2_enabled true;\nlight2_type directional;\nlight2_relative_position true;\n')
        light = row['explicit_enabled_auxiliary'][0]
        self.assertEqual(light['type'], 'directional')
        self.assertTrue(light['relative_position'])
        self.assertTrue(row['main']['enabled'])

    def test_legacy_ids_and_implicit_settings_are_separate(self):
        row = inventory('# version 2.13\n[main_parameters]\naux_light_enabled_1 true;\naux_light_intensity_1 0,27;\naux_light_position_2 1 2 3;\n')
        self.assertEqual(row['explicit_enabled_auxiliary'][0]['native_light_id'], 2)
        self.assertEqual(row['legacy_implicit_auxiliary_ids_require_review'], [2])

    def test_disabled_main_does_not_hide_enabled_secondary(self):
        row = inventory('# version 2.25\n[main_parameters]\nlight1_enabled false;\nlight2_enabled true;\n')
        self.assertFalse(row['main']['enabled'])
        self.assertEqual(row['explicit_enabled_auxiliary'][0]['type'], 'point')

    def test_fractal_section_cannot_enable_lights(self):
        row = inventory('# version 2.33\n[main_parameters]\nlight2_enabled false;\n[fractal_1]\nlight2_enabled true;\n')
        self.assertEqual(row['explicit_enabled_auxiliary'], [])

    def test_legacy_world_main_light_is_reported(self):
        row = inventory('# version 2.13\n[main_parameters]\nmain_light_position_relative false;\n')
        self.assertFalse(row['main']['relative_position'])
