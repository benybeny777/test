"""比較証跡と解析入力の同一性をCPUだけで検査する。"""
import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import numpy as np
from PIL import Image

from analyze import analyze
from run import ROOT,SOURCE_FILES,prepare_input


class AnalyzeSourceTests(unittest.TestCase):
    def setUp(self):
        (ROOT/'temp').mkdir(exist_ok=True)
        self.temporary=tempfile.TemporaryDirectory(dir=ROOT/'temp')
        self.addCleanup(self.temporary.cleanup)
        root=Path(self.temporary.name)
        self.character=root/'character';self.output=root/'comparison'
        for path in (self.character/'source',self.character/'analysis',self.output/'input'):
            path.mkdir(parents=True)
        image=Image.new('RGBA',(32,32),(120,100,90,255))
        for name in ('input','isolated'):image.save(self.character/f'source/{name}.png')
        metadata={'analysis':{'selected':{'face':{'box':[8,4,24,20]},'neck':{'box':[12,20,20,28]}}}}
        (self.character/'analysis/analysis.json').write_text(json.dumps(metadata),encoding='utf-8')
        mask=np.zeros((32,32),bool);mask[8:12,8:12]=True
        np.savez(self.character/'analysis/masks.npz',**{role:mask for role in ('left_eye','right_eye','mouth','neck','collar')})
        _,source=prepare_input(self.character,self.output,32,'head')
        image.save(self.output/'candidate.png')
        self.report={'status':'complete','mode':'edit','source':source,'images':['candidate.png'],'resources':[],'seconds':1}

    def evaluate(self):
        (self.output/'report.json').write_text(json.dumps(self.report),encoding='utf-8')
        with contextlib.redirect_stdout(io.StringIO()):analyze(self.output,self.character)

    def test_matching_source_produces_measurements(self):
        self.evaluate()
        metrics=json.loads((self.output/'metrics.json').read_text(encoding='utf-8'))
        self.assertEqual(metrics['visible_feature_changes']['mouth']['mean_absolute_rgb_0_255'],0)
        self.assertEqual(metrics['quality'],'unverified')

    def test_each_changed_input_is_rejected_before_metrics(self):
        for key,path in SOURCE_FILES.items():
            with self.subTest(key=key):
                target=self.character/path
                original=target.read_bytes()
                try:
                    target.write_bytes(original+b'changed')
                    with self.assertRaisesRegex(ValueError,'一致しません'):self.evaluate()
                    self.assertFalse((self.output/'metrics.json').exists())
                finally:target.write_bytes(original)

    def test_legacy_record_without_each_sha_is_rejected(self):
        for key in SOURCE_FILES:
            with self.subTest(key=key):
                saved=self.report['source'].pop(key)
                try:
                    with self.assertRaisesRegex(ValueError,'旧証跡'):self.evaluate()
                    self.assertFalse((self.output/'metrics.json').exists())
                finally:self.report['source'][key]=saved

    def test_mutation_during_input_preparation_is_rejected(self):
        original_save=Image.Image.save
        def changed_save(image,target,*args,**kwargs):
            original_save(image,target,*args,**kwargs)
            if Path(target).name=='reference.png':
                (self.character/'analysis/analysis.json').write_text('{}',encoding='utf-8')
        with patch.object(Image.Image,'save',changed_save):
            with self.assertRaisesRegex(ValueError,'一致しません'):
                prepare_input(self.character,self.output,32,'head')


if __name__=='__main__':unittest.main()
