import tempfile
import unittest
from pathlib import Path

from prepare_mandel_remaining import inventory
from run_release_canaries import sha256


class RemainingInventoryTests(unittest.TestCase):
    def test_same_basename_with_different_content_remains_distinct(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)
            (root/'reviewed.fract').write_text('reviewed')
            for collection in ('author-a','author-b'):
                (root/collection).mkdir()
                (root/collection/'scene.fract').write_text(collection)
            reviewed=dict(version=1,scenes=[dict(id='50',path='reviewed.fract',sha256=sha256(root/'reviewed.fract'))])
            result=inventory(root,reviewed)
            self.assertEqual([r['path'] for r in result['scenes']],['author-a/scene.fract','author-b/scene.fract'])
            self.assertEqual(result['counts']['remaining_unique_scenes'],2)

    def test_deduplicate_and_exclude_reviewed_content_not_just_filename(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)
            (root/'reviewed.fract').write_text('reviewed')
            (root/'reviewed-alias.fract').write_text('reviewed')
            text='[main_parameters]\nformula_1 42;\nfile_background image.png;\nclouds_enable true;\n'
            (root/'new.fract').write_text(text)
            (root/'new-alias.fract').write_text(text)
            reviewed=dict(version=1,scenes=[dict(id='50',path='reviewed.fract',sha256=sha256(root/'reviewed.fract'))])
            result=inventory(root,reviewed)
            self.assertEqual(result['counts'],dict(source_files=4,unique_hashes=2,reviewed_paths=1,remaining_paths=3,remaining_unique_scenes=1))
            self.assertEqual(result['scenes'][0]['id'],'051')
            self.assertEqual(result['scenes'][0]['path'],'new-alias.fract')
            self.assertEqual(result['scenes'][0]['aliases'],['new.fract'])
            self.assertEqual(result['scenes'][0]['formula_parameters'],{'formula_1':'42'})
            self.assertEqual(result['scenes'][0]['declared_file_parameters'],{'file_background':'image.png'})
            self.assertEqual(result,inventory(root,reviewed))
            (root/'reviewed.fract').write_text('mutated')
            with self.assertRaises(ValueError): inventory(root,reviewed)


if __name__=='__main__':
    unittest.main()
