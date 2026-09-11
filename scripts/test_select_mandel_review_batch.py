import copy
import unittest

from select_mandel_review_batch import select


def fixture():
    scenes=[dict(id=f'{i:03d}',path=f'scene{i}.fract',sha256=f'{i:064x}',collection='A' if i<4 else 'B') for i in range(1,7)]
    catalog=dict(scenes=[dict(r,status='experimental') for r in scenes])
    report=dict(identity=dict(manifest=dict(root='test',scenes=scenes),settings={}),rows=[dict(r,modes={m:dict(status='ok') for m in ('geometry','authored')}) for r in scenes])
    return catalog,report


class SelectionTests(unittest.TestCase):
    def test_deferred_references_are_excluded_without_changing_catalog_status(self):
        cat,report=fixture()
        original=copy.deepcopy(cat)
        deferred=dict(rows=[cat['scenes'][0]])
        result=select(cat,report,3,[deferred])
        self.assertEqual([r['id'] for r in result['scenes']],['002','004','003'])
        self.assertEqual(result['deferred_ids'],['001'])
        self.assertEqual(cat,original)
        for mutation in ('id','sha256'):
            bad=copy.deepcopy(deferred)
            bad['rows'][0][mutation]='999' if mutation=='id' else 'f'*64
            with self.subTest(mutation=mutation),self.assertRaises(ValueError):
                select(cat,report,3,[bad])

    def test_balanced_exclusion_and_determinism(self):
        cat,report=fixture()
        cat['scenes'][0]['status']='reviewed'
        cat['scenes'][3]['visual_decision']=dict(decision='needs-work')
        report['rows'][1]['modes']['authored']['screening']=dict(review_flags=['mostly_dark'])
        result=select(cat,report,3)
        self.assertEqual([r['id'] for r in result['scenes']],['003','005','006'])
        self.assertEqual(result,select(cat,report,3))

    def test_insufficient_or_stale_screening_rejected(self):
        cat,report=fixture()
        for variant in ('missing','changed','pending','count'):
            candidate=copy.deepcopy(report)
            if variant=='missing':candidate['rows'].pop()
            elif variant=='changed':candidate['rows'][0]['sha256']='f'*64
            elif variant=='pending':candidate['rows'][0]['modes'].pop('geometry')
            with self.subTest(variant=variant),self.assertRaises(ValueError):
                select(cat,candidate,7 if variant=='count' else 3)


if __name__=='__main__':unittest.main()
