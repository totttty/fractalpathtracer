import hashlib
from pathlib import Path
import tempfile
import unittest
import numpy as np

from run_mandel_hybrid_precision import config_values, depth_metrics, import_formula, render_source, vector


class HybridPrecisionTests(unittest.TestCase):
    def test_depth_metrics_preserve_low_term_and_hit_masks(self):
        native=np.zeros((2,16));metal=np.zeros((2,8))
        native[:,14]=.1;native[:,8]=1;native[:,12]=1
        metal[:,0]=1;metal[:,1]=.05;metal[0,2]=1
        result=depth_metrics(native,metal)
        self.assertEqual(result['common_hits'],1)
        self.assertAlmostEqual(result['median_error_in_thresholds'],.5)
        with self.assertRaises(ValueError):
            depth_metrics(native[:,:8],metal)

    def config(self):
        return dict(camera=[.3,-1,.5],target=[.3,0,.5],
            controls=[250,1,10000,1,1,1,13.68766681086107,1],
            sequence=[0,0]+[1 if i%2==0 else 0 for i in range(2,250)],
            axes=[1]*4,starts=[0]*4,stops=[250]*4,foldColor=[0,0],
            scale=[3],limits=[.225,-.075,.025,.2],
            slot0=[100,0,1,0,0,1],slot1=[100,0,1,0,0,1])

    def load(self, cfg):
        with tempfile.TemporaryDirectory() as tmp:
            path=Path(tmp)/'config.tsv'
            path.write_text('\n'.join(k+'\t'+'\t'.join(map(str,v)) for k,v in cfg.items()))
            return config_values(path)

    def test_native_contract_and_bad_sequence(self):
        self.assertEqual(self.load(self.config())['light_direction'],[0,1,0])
        for key, values in [('sequence',[0]*250),('axes',[0]*4),('foldColor',[1,0]),
                            ('slot0',[100,0,1,1,0,1]),('camera',[float('nan')]*3)]:
            cfg=self.config();cfg[key]=values
            with self.subTest(key=key), self.assertRaises(ValueError):
                self.load(cfg)

    def test_four_component_vector_and_directional_light(self):
        self.assertEqual(vector([1,2,3,4]).count('R('),4)
        with self.assertRaises(ValueError):
            vector([1,2])
        cfg=self.load(self.config())
        source=render_source('',cfg,225)
        self.assertNotIn('@',source)
        self.assertNotIn('gradient.Dot(direction)',source)
        self.assertIn('gradient.Dot(V(',source)
        self.assertIn('field(pos+dx)-field(pos-dx)',source)

    def test_formula_import_is_hash_checked_and_preserves_body(self):
        with tempfile.TemporaryDirectory() as tmp:
            path=Path(tmp)/'formula.cpp'
            path.write_text('// external notice\n#include "test.h"\n'
                'void Test::FormulaCode(V &z) { z.x -= fractal->transformCommon.scale3; }')
            sha=hashlib.sha256(path.read_bytes()).hexdigest()
            body=import_formula(path,'Test',sha,0,self.config())
            self.assertIn('// external notice',body)
            self.assertIn('z.x -= R(',body)
            self.assertNotIn('fractal->',body)
            with self.assertRaises(ValueError):
                import_formula(path,'Test','wrong',0,self.config())
