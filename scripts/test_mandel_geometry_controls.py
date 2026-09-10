import unittest
from pathlib import Path
from run_mandel_geometry_controls import geometry_overrides, headlight_command


class GeometryControlTests(unittest.TestCase):
    def test_control_is_unshadowed_white_camera_facing_lambert(self):
        settings = geometry_overrides('[main_parameters]\nlight1_cast_shadows true;\n')
        self.assertEqual(settings['light1_cast_shadows'], '0')
        self.assertEqual(settings['light1_rotation'], '0 0 0')
        self.assertEqual(settings['light1_relative_position'], '1')
        self.assertEqual(settings['light1_type'], '0')
        self.assertEqual(settings['light1_intensity'], '1')
        self.assertEqual(settings['mat1_shading'], '1')
        self.assertEqual(settings['ambient_occlusion_enabled'], '0')
        self.assertEqual(settings['random_lights_group'], '0')
        self.assertNotIn('random_lights_enabled', settings)

    def test_only_existing_extra_lights_are_disabled(self):
        source = '[main_parameters]\nlight7_is_defined true;\nmat3_shading 0.3;\n[fractal_1]\nlight9_enabled true;\n'
        settings = geometry_overrides(source)
        self.assertEqual(settings['light7_enabled'], '0')
        self.assertNotIn('light2_enabled', settings)
        self.assertNotIn('light9_enabled', settings)
        self.assertEqual(settings['mat3_surface_color'], 'ffff ffff ffff')

    def test_coloured_glow_cannot_contaminate_white_geometry(self):
        source = '[main_parameters]\nglow_enabled true;\nglow_intensity 2.34423;\n'
        self.assertEqual(geometry_overrides(source).get('glow_enabled'), '0')

    def test_chromatic_post_effect_cannot_contaminate_white_geometry(self):
        source = '[main_parameters]\npost_chromatic_aberration_enabled true;\n'
        self.assertEqual(geometry_overrides(source).get('post_chromatic_aberration_enabled'), '0')

    def test_command_does_not_change_source_camera_or_thresholds(self):
        source = '[main_parameters]\ncamera 1 2 3;\nformula_1 225;\nDE_thresh 0.001;\n'
        command = ['mandel', '-O', 'opencl_enabled=0', '-o', 'old.png', 'source.fract']
        result, settings = headlight_command(command, source, Path('/tmp/control'))
        self.assertEqual(command[4], 'old.png')
        self.assertEqual(result[-1], 'source.fract')
        self.assertEqual(result[4], '/tmp/control/scene.png')
        for key in ('camera', 'formula_1', 'DE_thresh'):
            self.assertNotIn(key, settings)


if __name__ == '__main__':
    unittest.main()
