import unittest
from run_mandel_shadow_controls import control_overrides, replace_main_parameters


class ShadowControlTests(unittest.TestCase):
    def test_only_main_parameters_are_replaced(self):
        source = '[main_parameters]\nlight1_enabled false;\n[fractal_1]\nlight1_enabled false;\n'
        result = replace_main_parameters(source, {'light1_enabled': 'true', 'gamma': '1'})
        self.assertEqual(result, '[main_parameters]\nlight1_enabled true;\ngamma 1;\n[fractal_1]\nlight1_enabled false;\n')

    def test_light_control_values_are_typed(self):
        values = control_overrides('[main_parameters]\nlight2_enabled true;\n', True, 5)
        self.assertEqual(values['light1_cast_shadows'], 'true')
        self.assertEqual(values['light1_penetrating'], 'true')
        self.assertEqual(values['light1_soft_shadow_cone'], '5')
        self.assertEqual(values['light2_enabled'], 'false')
        self.assertEqual(values['mat1_shading'], '1')
        self.assertEqual(values['glow_enabled'], 'false')
        self.assertEqual(values['MC_soft_shadows_enable'], 'false')

    def test_missing_section_rejected(self):
        with self.assertRaises(ValueError):
            replace_main_parameters('[fractal_1]\nformula 1;\n', {'gamma': '1'})
