import unittest
import tempfile
from pathlib import Path

from run_mandel_ifs_path import path_source, shifted_camera
from run_mandel_ifs_gallery import sheets


class IfsPathTests(unittest.TestCase):
    def test_camera_shift_preserves_view_and_source(self):
        original=dict(camera=[1.,2.,3.],target=[1.,2.,4.],top=[0.,1.,0.])
        moved,metadata=shifted_camera(original,(.25,-.25))
        self.assertEqual(original['camera'],[1.,2.,3.])
        self.assertEqual([b-a for a,b in zip(moved['camera'],moved['target'])],[0.,0.,1.])
        self.assertEqual(metadata['native_offset'],[-.25,-.25,0.])
        self.assertEqual(shifted_camera(original,(0,0))[0],original)

    def test_camera_shift_rejects_invalid_input(self):
        cfg=dict(camera=[0.,0.,0.],target=[0.,0.,1.],top=[0.,1.,0.])
        for shift in [(1,0),(float('nan'),0),(float('inf'),0),(0,)]:
            with self.assertRaises(ValueError): shifted_camera(cfg,shift)
        with self.assertRaises(ValueError): shifted_camera({**cfg,'target':cfg['camera']},(0,0))

    def test_gallery_rejects_resized_capture(self):
        from PIL import Image
        with tempfile.TemporaryDirectory() as directory:
            out=Path(directory)
            image=out/'wrong.png'; Image.new('RGB',(16,9)).save(image)
            report=dict(size=(32,18),samples=4,views=[dict(name='center',shift=[0,0],
                headlight=dict(native_image=str(image)))])
            with self.assertRaisesRegex(ValueError,'dimensions differ'): sheets(report,out)

    def source(self, arithmetic):
        return path_source(dict(camera=[-1.08473976030058,.115304126733863,.961518346656556],
            controls=[250,1,10000,.75,1,1,13.95392225033704]),
            'void formula(thread V &z, thread Aux &aux) {}\n', arithmetic)

    def test_both_arithmetic_variants_expand(self):
        for arithmetic in ('two','three'):
            source=self.source(arithmetic)
            self.assertNotIn('@',source)
            self.assertEqual(source.count('namespace fpt_ifs {'),1)
            self.assertIn('color_min=minimum(color_min,radius)',source)
            self.assertIn('namespace fpt_precision',source)

    def test_secondary_and_shadow_positions_stay_precise(self):
        source=self.source('two')
        self.assertIn('trace(V origin, V direction',source)
        self.assertIn('field(position+direction*travel)',source)
        self.assertIn('thresholdAt(position,cfg)',source)
        self.assertNotIn('mapSdf(',source)
        self.assertNotIn('userSdf(',source)

    def test_failed_queries_do_not_claim_visibility(self):
        source=self.source('two')
        self.assertIn('return {p,threshold,false,true}',source)
        self.assertIn('if(!(next>travel)) return -1.0f;',source)
        self.assertIn('if(visibility<0.0f) return float3(NAN);',source)


if __name__=='__main__':
    unittest.main()
