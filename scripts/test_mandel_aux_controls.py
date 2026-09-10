import unittest
from run_mandel_aux_controls import auxiliary_overrides


class AuxiliaryControlsTests(unittest.TestCase):
    def test_light2_keeps_authored_rotation_but_not_other_lights(self):
        values=auxiliary_overrides('[main_parameters]\nlight2_rotation 69,59 7,14 0;\nlight3_enabled true;\n','no-shadows')
        self.assertEqual(values['light1_enabled'],'false')
        self.assertEqual(values['light2_enabled'],'true')
        self.assertEqual(values['light2_rotation'],'69,59 7,14 0')
        self.assertEqual(values['light2_color'],'ffff ffff ffff')
        self.assertEqual(values['light2_cast_shadows'],'false')
        self.assertEqual(values['light3_enabled'],'false')
        self.assertEqual(values['fake_lights_enabled'],'false')

    def test_soft_directional_control_uses_its_own_cone(self):
        values=auxiliary_overrides('[main_parameters]\n','soft')
        self.assertEqual(values['light2_soft_shadow_cone'],'5')
        self.assertEqual(values['light2_penetrating'],'true')
