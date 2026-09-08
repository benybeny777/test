"""検出失敗を成功キャッシュへ書かず、左右/幅の根拠を残すCPU検査。"""
import json,tempfile,unittest,os,subprocess,sys
from unittest.mock import patch
from pathlib import Path
from PIL import Image
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from ear_runtime import choose_ears,EarSelectionError
from hidden_controller import ear_analysis


class EarFailureDiagnosticTests(unittest.TestCase):
    def detections(self):return {'face':[([10,10,90,90],.8)],'ear':[([12,35,22,55],.4),([75,35,85,55],.5)]}

    def test_success_and_one_side_partial_rules(self):
        data=self.detections();self.assertEqual(choose_ears(data,False),data['ear'])
        data['ear']=data['ear'][1:]
        self.assertEqual(choose_ears(data,True),[None,data['ear'][0]])
        self.assertEqual(choose_ears(data,False),[None,data['ear'][0]])

    def test_absence_width_side_and_missing_face_are_distinct(self):
        scenarios=[({'ear':[]},'face_not_detected'),({'face':[([10,10,90,90],.8)],'ear':[]},None),
                   ({'face':[([10,10,90,90],.8)],'ear':[([0,30,60,50],.9)]},None)]
        for data,reason in scenarios:
            with self.assertRaises(EarSelectionError) as caught:choose_ears(data,False)
            report=caught.exception.ear_diagnostics
            if reason:self.assertEqual(report['reason'],reason)
            elif not data['ear']:self.assertEqual(report['sides']['left']['candidates'],[])
            else:
                self.assertIn('width_out_of_range',report['sides']['left']['candidates'][0]['rejections'])
                self.assertIsNone(report['sides']['right']['selected_index'])

    def test_failure_persists_diagnostics_without_touching_success_cache(self):
        temporary=Path.cwd()/'temp';temporary.mkdir(exist_ok=True)
        for existing in (False,True):
            with tempfile.TemporaryDirectory(dir=temporary) as directory:
                root=Path(directory);cache=root/'completion-generated-ears';image=root/'edited.png'
                Image.new('RGB',(100,100),'white').save(image)
                if existing:
                    cache.mkdir();(cache/'manifest.json').write_text('{"identity":{"old":true}}')
                    (cache/'masks.npz').write_bytes(b'old-mask')
                before={p.name:p.read_bytes() for p in cache.iterdir()} if existing else {}
                def segment(*args):
                    data=self.detections();data['ear']=[];choose_ears(data,False)
                with self.assertRaises(EarSelectionError) as caught:
                    ear_analysis(cache,image,False,{'threshold':.2},segment,lambda:None,lambda:None)
                after={p.name:p.read_bytes() for p in cache.iterdir()} if cache.exists() else {}
                self.assertEqual(before,after)
                files=list((root/'temp').glob('ear-analysis-failure-*/failure.json'));self.assertEqual(len(files),1)
                data=json.loads(files[0].read_text(encoding='utf-8'))
                self.assertEqual(data['status'],'failed');self.assertEqual(data['identity']['parser']['threshold'],.2)
                self.assertEqual(data['diagnostics']['sides']['left']['status'],'missing_after_selection')
                self.assertEqual(caught.exception.ear_diagnostic_path,str(files[0]))
                self.assertIn(str(files[0]),str(caught.exception))

    def test_diagnostic_io_failure_keeps_original_error_as_cause(self):
        temporary=Path.cwd()/'temp';temporary.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=temporary) as directory:
            root=Path(directory);image=root/'edited.png';Image.new('RGB',(100,100),'white').save(image)
            def segment(*args):choose_ears({'face':[([10,10,90,90],.8)],'ear':[]},False)
            with patch('hidden_controller.save_ear_failure',side_effect=OSError('fixture write denied')):
                with self.assertRaises(RuntimeError) as caught:
                    ear_analysis(root/'cache',image,False,{},segment,lambda:None,lambda:None)
            self.assertIsInstance(caught.exception.__cause__,EarSelectionError)
            self.assertIn('fixture write denied',str(caught.exception))
            self.assertIn('生成耳を一側も検出できません',str(caught.exception))
            self.assertFalse((root/'cache').exists())

    @unittest.skipUnless(os.name=='nt','Windowsの実junction検査')
    def test_real_junction_in_temp_or_ancestor_rejects_diagnostic_write(self):
        temporary=Path.cwd()/'temp';temporary.mkdir(exist_ok=True)
        for location in ('temp','ancestor'):
            with tempfile.TemporaryDirectory(dir=temporary) as directory:
                root=Path(directory);target=root/'target';target.mkdir()
                image=root/'edited.png';Image.new('RGB',(100,100),'white').save(image)
                link=root/('temp' if location=='temp' else 'linked-character')
                environment=dict(os.environ,LVS_EARDIAG_LINK=str(link),LVS_EARDIAG_TARGET=str(target))
                result=subprocess.run(['pwsh','-NoProfile','-Command',
                    'New-Item -ItemType Junction -Path $env:LVS_EARDIAG_LINK -Target $env:LVS_EARDIAG_TARGET -ErrorAction Stop | Out-Null'],
                    env=environment,capture_output=True,text=True,timeout=15)
                self.assertEqual(result.returncode,0,result.stderr)
                try:
                    self.assertTrue(link.is_junction())
                    cache=(root if location=='temp' else link)/'cache'
                    def segment(*args):choose_ears({'face':[([10,10,90,90],.8)],'ear':[]},False)
                    with self.assertRaises(RuntimeError) as caught:
                        ear_analysis(cache,image,False,{},segment,lambda:None,lambda:None)
                    self.assertIsInstance(caught.exception.__cause__,EarSelectionError)
                    self.assertEqual(list(target.iterdir()),[])
                    self.assertFalse(cache.exists())
                finally:
                    # 自分が作ったjunctionそのものだけを外す。参照先は削除しない。
                    link.rmdir()


if __name__=='__main__':unittest.main()
