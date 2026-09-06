"""同一規則の左右反転・平行移動と曖昧時の所有保持をCPU検査する。"""
from pathlib import Path
import sys,unittest,json,tempfile
from unittest.mock import patch
from PIL import Image
import numpy as np
sys.path.insert(0,str(Path(__file__).parent))
from optional_limbs import select,partition,validate_graph
import grounded


class LimbTests(unittest.TestCase):
    def fixture(self):
        masks={name:np.zeros((32,32),bool) for name in ('face','hair','clothes','left_arm','right_arm')}
        masks['face'][2:8,12:20]=True;masks['hair'][0:2,10:22]=True
        masks['left_arm'][10:30,2:8]=True;masks['right_arm'][10:30,24:30]=True
        masks['clothes'][10:20,2:30]=True
        sleeve=masks['left_arm']&masks['clothes'];hand=np.zeros((32,32),bool);hand[26:30,2:8]=True
        candidates=[{'id':'s','role':'sleeve','score':.8,'mask':sleeve},{'id':'h','role':'hand','score':.8,'mask':hand}]
        owners={name:masks[name].copy() for name in ('left_arm','right_arm','face','hair')}
        owners['residual']=~np.logical_or.reduce(list(owners.values()))
        return masks,candidates,owners

    def test_reflection_and_translation(self):
        masks,candidates,owners=self.fixture()
        proposed,_=select(candidates,masks);baseline,_=partition(owners,proposed)
        self.assertIn('left_hand',baseline)
        for mirror in (False,True):
            def transform(a):return np.pad(np.fliplr(a) if mirror else a,((5,7),(3,9)))
            def key(k):return k.replace('left_','swap_').replace('right_','left_').replace('swap_','right_') if mirror else k
            changed_masks={key(k):transform(v) for k,v in masks.items()}
            changed_candidates=[{**c,'mask':transform(c['mask'])} for c in candidates]
            changed_owners={key(k):transform(v) for k,v in owners.items()}
            proposal,_=select(changed_candidates,changed_masks);result,_=partition(changed_owners,proposal)
            for role,mask in baseline.items():np.testing.assert_array_equal(result[key(role)],transform(mask))

    def test_face_false_positive_rejected(self):
        masks,_,owners=self.fixture()
        candidates=[{'id':'face','role':'hand','score':1.,'mask':masks['face']}]
        proposed,report=select(candidates,masks);self.assertFalse(proposed)
        self.assertIn('face_hair_conflict',report['candidates'][0]['rejections'])
        result,_=partition(owners,proposed)
        for k,v in owners.items():np.testing.assert_array_equal(result[k],v)

    def test_ambiguous_or_empty_parent_preserves_owners(self):
        masks,_,owners=self.fixture()
        for proposed in ({'left_sleeve':owners['left_arm']},
                         {'left_hand':owners['left_arm'],'left_sleeve':owners['left_arm']}):
            result,report=partition(owners,proposed)
            self.assertTrue(any(v.startswith(('would_empty','ambiguous')) for v in report['roles'].values()))
            for k,v in owners.items():np.testing.assert_array_equal(result[k],v)

    def test_unfilled_joint_not_enabled(self):
        graph=[{'role':'left_hand','parent':'left_arm','motion':'inherit_parent_arm'}]
        validate_graph({'scene_left_hand':{}},graph)
        graph[0]['motion']='independent'
        with self.assertRaises(ValueError):validate_graph({'scene_left_hand':{}},graph)

    def test_normal_optional_sampler_keeps_rejections_and_absence(self):
        masks,candidates,_=self.fixture();calls=[]
        records={'hand':[{'box':[1,2,3,4],'score':.9},{'box':[2,3,4,5],'score':.8}]}
        def sample(role,item):
            calls.append(item['box'])
            return (masks['face'] if item['score']==.9 else candidates[1]['mask']),{'mask_policy':'selected_best_mask'}
        selected,status=grounded.analyse_optional_limbs(records,masks,sample)
        self.assertEqual(len(calls),2);self.assertEqual(status['queries']['sleeve'],'not_detected')
        self.assertIn('left_hand',selected)
        self.assertIn('face_hair_conflict',status['candidates'][0]['rejections'])

    def test_cache_version_and_optional_code_invalidate(self):
        temporary=Path.cwd()/'temp';temporary.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=temporary) as directory:
            directory=Path(directory);detector=directory/'detector';sam=directory/'sam'
            detector.mkdir();sam.mkdir()
            (detector/'config.json').write_text('{}');(sam/'config.json').write_text('{}')
            image=Image.new('RGBA',(4,4),(10,20,30,255));cache=directory/'cache'
            sleeve=np.zeros((4,4),bool);sleeve[1:3,0:2]=True
            reason={'queries':{'hand':'not_detected'},'roles':{'left_sleeve':'candidate_needs_review'},
                    'candidates':[{'id':'hand-000','rejections':['face_hair_conflict']}],
                    'ownership':{'roles':{'right_sleeve':'ambiguous_sleeve_hand_overlap'}}}
            value=({'face':np.ones((4,4),bool),'left_sleeve':sleeve},{},{'optional_limbs':reason})
            with patch.object(grounded,'analyse',return_value=value) as mocked:
                invoke=lambda:grounded.analyse_cached(image,detector,sam,.2,lambda *a,**k:None,cache,.5)
                invoke();reloaded=invoke();self.assertEqual(mocked.call_count,1)
                np.testing.assert_array_equal(reloaded[0]['left_sleeve'],sleeve)
                self.assertEqual(reloaded[2]['optional_limbs'],reason)
                saved=json.loads((cache/'analysis.json').read_text(encoding='utf-8'))
                self.assertEqual(saved['identity']['version'],3)
                self.assertEqual(set(saved['identity']['optional_limbs_code']),{'limbs.py','optional_limbs.py'})
                original_digest=grounded._digest
                with patch.object(grounded,'_digest',side_effect=lambda p:'changed' if Path(p).name=='limbs.py' else original_digest(p)):
                    invoke()
                self.assertEqual(mocked.call_count,2)


if __name__=='__main__':unittest.main()
