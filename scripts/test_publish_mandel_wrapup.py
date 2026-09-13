import copy
import json
from pathlib import Path
import tempfile
import unittest

from PIL import Image

from publish_mandel_wrapup import categories, checked_asset, compact, draw_card, reusable_fpt
from run_release_canaries import sha256


class WrapupTests(unittest.TestCase):
    def test_scene_95_is_only_user_deferred(self):
        self.assertEqual(categories(dict(id='095',note='dark missing geometry',geometry='needs-work')), ['user-deferred'])

    def test_grouping_is_nonexclusive_and_uncertainty_is_preserved(self):
        groups=categories(dict(id='663',note='Dark orbit-trap illumination and fog',geometry='uncertain'))
        self.assertIn('volume-atmosphere',groups)
        self.assertIn('missing-illumination',groups)
        self.assertIn('geometry-control-needed',groups)

    def test_timeout_never_becomes_success_from_diagnostic_image(self):
        result=compact(dict(status='timeout',diagnostic_capture=dict(path='unused')), [150,150], 'MC cap 1')
        self.assertNotIn('capture',result)
        self.assertEqual(result['status'],'timeout')

    def test_asset_hash_mismatch_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            p=Path(folder)/'test';p.write_text('x')
            with self.assertRaises(ValueError):checked_asset(dict(capture=dict(path=str(p),sha256='invalid')))

    def test_card_labels_preview_only_and_handles_missing_images(self):
        with tempfile.TemporaryDirectory() as folder:
            p=Path(folder)/'card.jpg'
            draw_card(dict(id='001',path='test.fract',modes={'mandel':{'status':'timeout'}}),p)
            with Image.open(p) as image:self.assertEqual(image.size,(900,245))

    def test_fpt_reuse_requires_hashes_settings_and_exact_camera_command(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder).resolve();binary=root/'fpt-metal';binary.write_bytes(b'binary')
            scene=root/'test.fract';scene.write_text('[main_parameters]\nimage_width 300;\nimage_height 225;\n')
            capture=root/'capture.png';Image.new('RGB',(300,225),'red').save(capture)
            meta=root/'capture.render.json';meta.write_text(json.dumps(dict(scene_sha256=sha256(scene),samples=32,width=300,height=225)))
            command=[str(binary),'render',str(scene),'--mandelbulber-root',str(root),'--width','300','--height','225','--samples','32','--sdf-accumulation','chunked','--sdf-chunk-samples','1','--mandel-appearance','authored-path','--out',str(root)]
            command_path=root/'command.json';command_path.write_text(json.dumps(command))
            source=dict(id='001',path='test.fract',sha256=sha256(scene))
            row=dict(source,size=[300,225],modes={'authored':dict(status='ok',capture=dict(path=str(capture),sha256=sha256(capture)),metadata=dict(path=str(meta),sha256=sha256(meta)))})
            report=dict(identity=dict(executables={str(binary):sha256(binary)},settings=dict(samples=32,max_axis=300,aspect='authored'),environment={'FPT_MANDEL_TILED_DISPATCH':'1','FPT_MANDEL_TILE_ROWS':'8'}))
            self.assertTrue(reusable_fpt(report,row,source,binary,root,root,'authored')['reused'])
            for mutation in ('binary','samples','source','size','environment','camera-command','metadata','capture'):
                changed=copy.deepcopy(report); changed_row=copy.deepcopy(row)
                command_path.write_text(json.dumps(command))
                if mutation=='binary':changed['identity']['executables'][str(binary)]='bad'
                elif mutation=='samples':changed['identity']['settings']['samples']=1
                elif mutation=='source':changed_row['sha256']='bad'
                elif mutation=='size':changed_row['size']=[150,113]
                elif mutation=='environment':changed['identity']['environment']={}
                elif mutation=='camera-command':command_path.write_text(json.dumps(command+['--camera-position','1,2,3']))
                elif mutation=='metadata':changed_row['modes']['authored']['metadata']['sha256']='bad'
                else:changed_row['modes']['authored']['capture']['sha256']='bad'
                with self.subTest(mutation=mutation),self.assertRaises(ValueError):
                    reusable_fpt(changed,changed_row,source,binary,root,root,'authored')


if __name__=='__main__':unittest.main()
