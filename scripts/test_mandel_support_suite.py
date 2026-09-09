import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from PIL import Image
from run_mandel_support_suite import capture, classify, failure_kind, parameters, resolve_lightmap, validate_resume
from run_release_canaries import image_result


class SupportSuiteTests(unittest.TestCase):
    def test_parameter_section_and_authored_lightmap_resolution(self):
        text='[main_parameters]\nfile_lightmap /usr/share/mandelbulber2/textures/custom.png;\n[fractal_1]\nfile_lightmap wrong.png;\n'
        self.assertEqual(parameters(text)['file_lightmap'],'/usr/share/mandelbulber2/textures/custom.png')
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)
            (root/'textures').mkdir()
            (root/'examples').mkdir()
            source=root/'examples/scene.fract'
            source.write_text(text)
            asset=root/'textures/custom.png'
            Image.new('RGB',(2,2),'red').save(asset)
            resolved=resolve_lightmap(source,root,root/'default.jpg')
            self.assertEqual(resolved['path'],str(asset.resolve()))
            asset.unlink()
            with self.assertRaises(ValueError): resolve_lightmap(source,root,root/'default.jpg')

    def test_independent_modes_do_not_claim_geometry_or_appearance_parity(self):
        row=dict(modes={'geometry':dict(status='ok'),'authored':dict(status='unsupported_contract'),'mandel':dict(status='timeout')})
        classify(row)
        self.assertEqual(row['compilation'],'passed')
        self.assertEqual(row['status'],'incomplete')
        self.assertEqual(row['geometry_fidelity'],'requires_visual_review')
        self.assertNotIn('appearance_difference',row)
        row['modes']['geometry']=dict(status='timeout')
        classify(row)
        self.assertEqual(row['compilation'],'not_established')

    def test_timeout_and_compile_failure_are_distinct(self):
        self.assertEqual(failure_kind(subprocess.TimeoutExpired(['fpt'],1),''),'timeout')
        self.assertEqual(failure_kind(RuntimeError('failed'),'offline Metal compilation failed:'),'compile_failed')
        self.assertEqual(failure_kind(RuntimeError('failed'),'authored AO does not yet support iteration fog'),'unsupported_contract')

    def test_failed_capture_keeps_diagnostic_image_without_reporting_success(self):
        with tempfile.TemporaryDirectory() as temp:
            folder=Path(temp)
            def failed(command,folder,timeout):
                Image.new('RGB',(2,2),'black').save(folder/'scene.png')
                (folder/'stderr.log').write_text('blank or single-colour')
                raise RuntimeError('exit 1')
            result=capture(['fake'],folder,(2,2),1,runner=failed)
            self.assertEqual(result['status'],'image_validation_failed')
            self.assertIn('diagnostic_capture',result)
            self.assertNotIn('capture',result)

    def test_resume_rejects_changed_inputs_or_images(self):
        with tempfile.TemporaryDirectory() as temp:
            folder=Path(temp)
            Image.new('RGB',(2,2),'red').save(folder/'scene.png')
            identity=dict(binary='first',samples=32)
            summary=dict(identity=identity,rows=[dict(modes=dict(geometry=dict(status='ok',capture=image_result(folder,(2,2)))))])
            validate_resume(summary,identity)
            with self.assertRaises(ValueError):validate_resume(summary,dict(binary='second',samples=32))
            Image.new('RGB',(2,2),'blue').save(folder/'scene.png')
            with self.assertRaises(ValueError):validate_resume(summary,identity)


if __name__=='__main__':unittest.main()
