import copy
import json
import tempfile
import unittest
from pathlib import Path
from PIL import Image

from mandel_reference_cache import digest, reference_contract, load_reference, store_reference, verify_native_log
from run_release_canaries import image_result, lightmap_asset, mandel_reference_command


class ReferenceCacheTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name)
        self.textures=self.root/'deploy/share/mandelbulber2/textures'
        self.textures.mkdir(parents=True)
        self.map=self.textures/'lightmap.jpg'
        Image.new('RGB',(2,2),'red').save(self.map)
        self.scene=self.root/'scene.fract'
        self.scene.write_text('[main_parameters]\ncamera 1 2 3;\n')
        self.binary=self.root/'mandel'
        self.binary.write_text('binary')
        self.folder=self.root/'fresh'
        self.folder.mkdir()
        self.command=mandel_reference_command(self.binary,self.scene,(2,2),self.folder/'scene.png',self.map)
        self.contract,self.reason=self.make_contract()
        Image.new('RGB',(2,2),'blue').save(self.folder/'scene.png')
        (self.folder/'command.json').write_text(json.dumps(self.command))
        for name in ('stdout.log','stderr.log'):(self.folder/name).write_text('CPU')
        self.result=dict(status='ok',capture=dict(image_result(self.folder,(2,2)),wall_seconds=9.0))
        self.cache=self.root/'cache'

    def make_contract(self):
        return reference_contract(self.command,self.scene,(2,2),lightmap_asset(self.map),self.root)

    def test_round_trip_retains_image_and_original_timing(self):
        store_reference(self.cache,self.contract,self.folder,self.result)
        result=load_reference(self.cache,self.contract,self.root/'reused')
        self.assertEqual(result['capture']['sha256'],self.result['capture']['sha256'])
        self.assertEqual(result['capture']['wall_seconds'],0)
        self.assertEqual(result['reference_cache']['original_wall_seconds'],9)

    def test_different_resolution_camera_binary_asset_or_settings_misses(self):
        store_reference(self.cache,self.contract,self.folder,self.result)
        for key in ('dimensions','scene_sha256','binary_sha256','assets','command','environment_sha256','native_settings','resource_roots'):
            candidate=copy.deepcopy(self.contract)
            candidate[key]='changed'
            self.assertIsNone(load_reference(self.cache,candidate,self.root/'unused'))
        self.scene.write_text('[main_parameters]\ncamera 3 2 1;\n')
        self.assertNotEqual(self.make_contract()[0],self.contract)
        self.scene.write_text('[main_parameters]\ncamera 1 2 3;\n')
        self.binary.write_text('new binary')
        self.assertNotEqual(self.make_contract()[0],self.contract)
        self.binary.write_text('binary')
        Image.new('RGB',(2,2),'green').save(self.map)
        self.assertNotEqual(self.make_contract()[0],self.contract)

    def test_output_location_and_timeout_are_not_render_settings(self):
        self.command[self.command.index('-o')+1]='/another/output.png'
        self.assertEqual(self.make_contract()[0],self.contract)

    def test_corruption_unknown_assets_and_failures_are_not_reused(self):
        store_reference(self.cache,self.contract,self.folder,dict(status='timeout'))
        self.assertFalse(self.cache.exists())
        store_reference(self.cache,self.contract,self.folder,self.result)
        (self.cache/digest(self.contract)/'scene.png').write_bytes(b'corrupt')
        self.assertIsNone(load_reference(self.cache,self.contract,self.root/'unused'))
        self.scene.write_text('[main_parameters]\nmat1_file_color_texture /unknown.png;\n')
        contract,reason=self.make_contract()
        self.assertIsNone(contract)
        self.assertIn('external asset',reason)

    def test_incomplete_legacy_entry_cannot_be_promoted(self):
        entry=self.cache/digest(self.contract)
        entry.mkdir(parents=True)
        (entry/'entry.json').write_text(json.dumps(dict(result=self.result)))
        self.assertIsNone(load_reference(self.cache,self.contract,self.root/'unused'))

    def test_actual_native_paths_must_match_fingerprinted_dependencies(self):
        for text in ('Settings file: /unknown/settings.ini',
                     'sharePath directory: /unknown/resources', 'OpenCl - rendering'):
            (self.folder/'stdout.log').write_text(text)
            with self.assertRaises(ValueError): verify_native_log(self.contract,self.folder)

    def test_tampered_log_and_command_cannot_be_reused(self):
        store_reference(self.cache,self.contract,self.folder,self.result)
        (self.cache/digest(self.contract)/'command.json').write_text('[]')
        self.assertIsNone(load_reference(self.cache,self.contract,self.root/'unused'))


if __name__=='__main__':unittest.main()
