import tempfile
import unittest
from pathlib import Path

from summarize_mandel_screening import summarize, publish


class ScreeningSummaryTests(unittest.TestCase):
    def report(self):
        sources=[dict(id=str(i),path=f'{i}.fract',sha256=str(i),collection='a' if i<3 else 'b') for i in range(1,6)]
        rows=[dict(source,modes={m:dict(status='ok',screening=dict(review_flags=[])) for m in ('geometry','authored')}) for source in sources[:4]]
        rows[1]['modes']['authored']['status']='missing_asset'
        rows[3]['modes']['authored']['screening']['review_flags']=['mostly_dark']
        return dict(identity=dict(manifest=dict(root='supplied',scenes=sources),settings=dict(max_axis=96,samples=1)),rows=rows)

    def test_pending_failures_darkness_and_selection_are_separate(self):
        result=summarize(self.report())
        self.assertEqual(result['completed_scenes'],4)
        self.assertEqual(result['execution_passed'],3)
        self.assertEqual(result['flagged_scenes'],1)
        self.assertEqual(result['next_review_ids'],['1','3'])
        self.assertFalse(result['visual_parity_certified'])
        self.assertTrue(result['selection_provisional'])
        self.assertEqual(result['status_counts']['authored'],{'ok':3,'missing_asset':1,'pending':1})

    def test_report_includes_pending_and_never_labels_it_reviewed(self):
        with tempfile.TemporaryDirectory() as temp:
            output=Path(temp)/'review'
            publish(self.report(),output)
            self.assertTrue((output/'screening-01.png').is_file())
            self.assertIn('no native-reference comparison',(output/'README.md').read_text())

    def test_modified_artifact_rejected_before_publication(self):
        report=self.report()
        with tempfile.TemporaryDirectory() as temp:
            source=Path(temp)/'image.png'
            source.write_bytes(b'changed')
            report['rows'][0]['modes']['geometry']['capture']=dict(path=str(source),sha256='old')
            with self.assertRaisesRegex(ValueError,'hash mismatch'):
                publish(report,Path(temp)/'out')
            self.assertFalse((Path(temp)/'out').exists())


if __name__=='__main__':
    unittest.main()
