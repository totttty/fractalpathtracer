import copy
import unittest
from retry_mandel_mono_reference import replace_reference


class MonoReferenceTests(unittest.TestCase):
    def test_control_preserves_base_failure_and_fpt_captures(self):
        report=dict(rows=[dict(id='572',modes=dict(
            geometry=dict(status='ok',token='unchanged-geometry'),
            authored=dict(status='timeout',token='unchanged-authored'),
            mandel=dict(status='execution_failed',error='double width')))])
        before=copy.deepcopy(report)
        result=dict(status='ok',capture=dict(path='mono.png'))
        derived=replace_reference(report,'572',result)
        self.assertEqual(report,before)
        row=derived['rows'][0]
        self.assertEqual(row['modes']['geometry'],report['rows'][0]['modes']['geometry'])
        self.assertEqual(row['modes']['authored'],report['rows'][0]['modes']['authored'])
        self.assertEqual(row['modes']['mandel']['previous_reference']['error'],'double width')
        self.assertEqual(row['reference_overrides'],{'stereo_enabled':False})
        self.assertEqual(row['status'],'incomplete')
        self.assertNotIn('previous_reference',result)


if __name__=='__main__':
    unittest.main()
