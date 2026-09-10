import argparse
import copy
import json
from pathlib import Path
import tempfile
import unittest

import mandel_catalog as catalog


def fixture():
    def scene(number):
        return dict(id=f'{number:02d}', path=f'scene{number}.fract', sha256=f'{number:064x}')
    a, b, c = [scene(n) for n in (1, 51, 52)]
    def review(s):
        return dict(id=s['id'], source=s['path'], source_sha256=s['sha256'], note='review note',
            captures={m:dict(sha256='a'*64) for m in ('mandel','geometry','authored')})
    def screen(s, status):
        return dict(s, modes={m:dict(status=status, screening=dict(review_flags=[])) for m in catalog.MODES})
    return dict(ranked=dict(scenes=[a]), remaining=dict(scenes=[b,c]),
        gallery=dict(rows=[review(a)], upstream_revision='a'*40, production_sha256='b'*64),
        additional=dict(rows=[review(b)], upstream_revision='a'*40),
        screening=dict(rows=[screen(b,'ok'),screen(c,'missing_asset')],
            identity=dict(manifest=dict(scenes=[b,c]), settings=dict(max_axis=96,samples=1), executables={'/local/fpt':'b'*64})))


class CatalogTests(unittest.TestCase):
    def test_status_does_not_promote_execution_or_extra_review(self):
        result = catalog.build_catalog(**fixture())
        self.assertEqual([r['status'] for r in result['scenes']], ['reviewed','experimental','blocked'])
        self.assertEqual(result['scenes'][1]['review'], 'additional-reference-review')
        self.assertFalse(result['scenes'][1]['gallery_published'])
        self.assertTrue(all(r['naadf_cvox_validation']=='not-established-by-this-catalogue' for r in result['scenes']))
        self.assertNotIn('/local/', json.dumps(result))

    def test_missing_and_stale_evidence_rejected(self):
        for change in ('missing', 'duplicate', 'source', 'pending', 'revision'):
            data = fixture()
            if change == 'missing':
                data['screening']['rows'].pop()
            elif change == 'duplicate':
                data['screening']['rows'].append(copy.deepcopy(data['screening']['rows'][0]))
            elif change == 'source':
                data['additional']['rows'][0]['source_sha256'] = 'c'*64
            elif change == 'pending':
                data['screening']['rows'][0]['modes'].pop('geometry')
            else:
                data['additional']['upstream_revision'] = 'b'*40
            with self.subTest(change=change), self.assertRaises(ValueError):
                catalog.build_catalog(**data)

    def test_resolver_rejects_escape_hash_drift_and_symlinks(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)/'examples'
            root.mkdir()
            scene = root/'test.fract'
            scene.write_text('[main_parameters]\n')
            row = dict(id='01', path='test.fract', sha256=catalog.sha256(scene))
            self.assertEqual(catalog.resolve_scene(row, root), scene.resolve())
            scene.write_text('changed')
            with self.assertRaisesRegex(ValueError, 'hash mismatch'):
                catalog.resolve_scene(row, root)
            for bad in ('../outside.fract', '/outside.fract', '..\\outside.fract'):
                with self.subTest(bad=bad), self.assertRaises(ValueError):
                    catalog.resolve_scene(dict(row,path=bad), root)
            outside = Path(folder)/'outside.fract'
            outside.write_text('outside')
            (root/'link.fract').symlink_to(outside)
            with self.assertRaisesRegex(ValueError, 'outside'):
                catalog.resolve_scene(dict(row,path='link.fract',sha256=catalog.sha256(outside)), root)

    def test_render_preserves_aspect_and_requires_blocked_override(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            scene = root/'deploy/share/mandelbulber2/examples/scene with spaces.fract'
            scene.parent.mkdir(parents=True)
            scene.write_text('[main_parameters]\nimage_width 800;\nimage_height 400;\n')
            row = dict(id='01', path=scene.name,sha256=catalog.sha256(scene),status='blocked',blockers=['missing_asset'])
            args = argparse.Namespace(allow_blocked=False, samples=32, max_axis=300,
                mandelbulber_root=root, fpt=root/'fpt-metal', mode='authored', out=root/'output')
            with self.assertRaisesRegex(ValueError, 'blocked scene'):
                catalog.render_command(row,args)
            args.allow_blocked = True
            command = catalog.render_command(row,args)
            self.assertEqual(command[2], str(scene.resolve()))
            self.assertEqual(command[command.index('--width')+1], '300')
            self.assertEqual(command[command.index('--height')+1], '150')
            self.assertEqual(command[command.index('--mandel-appearance')+1], 'authored-path')
            args.samples = 0
            with self.assertRaisesRegex(ValueError, 'positive'):
                catalog.render_command(row,args)

    def test_committed_catalogue_complete_and_portable(self):
        if not catalog.CATALOG.exists():
            self.skipTest('catalogue not yet generated')
        data = json.loads(catalog.CATALOG.read_text())
        sources = catalog.source_index(data['scenes'])
        self.assertEqual(set(sources), set(range(1,747)))
        self.assertEqual(len({r['sha256'] for r in sources.values()}),746)
        self.assertEqual(data['counts']['statuses'],dict(reviewed=48,experimental=675,blocked=23))
        for key in (46,48):
            self.assertEqual(sources[key]['status'],'blocked')
            self.assertTrue(sources[key]['gallery_published'])
        self.assertEqual(catalog.markdown(data),catalog.CATALOG.with_name('scenes.md').read_text())
        for machine_path in ('/Volumes/','/Users/'):
            self.assertNotIn(machine_path, json.dumps(data))


if __name__ == '__main__':
    unittest.main()
