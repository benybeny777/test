"""推論を呼ばず、合成fixtureで2ジョブと耳解析の独立再利用を検証する。"""
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
from unittest.mock import patch
import unittest
import numpy as np
from PIL import Image
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'sidecar'))
from hidden_controller import hidden_edits, ear_analysis, assert_result_sources
from ear_runtime import choose_ears
from hidden_jobs import JOBS
from raw_reuse import sha
import hidden_bridge


class ControllerTests(unittest.TestCase):
    def setUp(self):
        folder=tempfile.TemporaryDirectory(dir=Path(__file__).resolve().parents[2]/'temp')
        self.addCleanup(folder.cleanup)
        self.root=Path(folder.name)
        self.args=SimpleNamespace(character=self.root,steps=50,seed=777,fast_disk=True,
            hidden_prompt='fixture hidden edit',side_prompt='fixture side edit')
        self.prepared=Image.new('RGB',(64,64),(180,160,140))
        self.source={name:sha(name.encode()) for name in ('source/input.png','source/isolated.png','analysis/analysis.json','analysis/masks.npz')}
        self.workflow={key:{'inputs':{}} for key in ('7','9','10','13')}
        self.args.overlay=Path(__file__).resolve().parents[2]/'workflows/qwen-edit-eyes-overlay.json'
        mask=np.zeros((64,64),np.uint8);mask[10:50,10:50]=255
        self.masks={job:mask.copy() for job in JOBS}
        self.calls=[];self.fail=None
        (self.root/'completion-source').mkdir()
        (self.root/'completion-source/edited.png').write_bytes(b'closed-eye-sentinel')

    def infer(self,args,work,graph):
        job=graph['13']['inputs']['filename_prefix'];self.calls.append(job)
        if self.fail==job:raise RuntimeError('fixture推論失敗')
        output=work/'output'/f'{job}.png';self.prepared.save(output)
        return output

    def run_edits(self):
        return hidden_edits(self.args,self.prepared,self.source,[0,0,64,64],{'models':'fixed'}, {'runtime':'fixed'},
            self.workflow,lambda:None,self.infer,lambda:None,lambda *a,**k:None,self.masks)

    def test_mask_change_reuses_other_job_and_outside_change_rejected(self):
        self.run_edits()
        self.masks['side-ears'][10,10]=0
        self.run_edits()
        self.assertEqual(self.calls,['hidden-face','side-ears','side-ears'])
        original_infer=self.infer
        def broken(args,work,graph):
            path=original_infer(args,work,graph)
            with Image.open(path) as image:
                pixels=image.copy()
            pixels.putpixel((0,0),(0,0,0));pixels.save(path)
            return path
        self.infer=broken;self.args.seed+=1
        with self.assertRaisesRegex(ValueError,'マスク外'):self.run_edits()

    def test_normal_bridge_parses_original_before_generation_and_generated_after(self):
        events=[]
        model=self.root/'models';model.mkdir();(model/'config.json').write_text('{}')
        from hidden_controller import digest
        models={name:{'config.json':digest(model/'config.json')} for name in ('detector','sam')}
        analysis=self.root/'analysis';analysis.mkdir()
        (analysis/'analysis.json').write_text(json.dumps({'identity':{'models':models}}))
        self.args.grounding_model=self.args.sam_model=model
        self.args.grounding_threshold=.3;self.args.ear_context=.5
        self.args.hidden_band_ratio=.08;self.args.hidden_motion_ratio=.35
        self.args.hair_edge_band_ratio=.015;self.args.hair_edge_gain=40
        self.args.workflow=self.root/'workflow.json';self.args.workflow.write_text(json.dumps(self.workflow))
        original=self.root/'original.png';self.prepared.save(original)
        mask=np.zeros((64,64),bool);mask[10:50,10:20]=True
        masks={name:np.zeros_like(mask) for name in ('left_eye','right_eye','mouth')};masks['hair']=mask
        masks['left_eye'][20:22,22:24]=True;masks['right_eye'][20:22,40:42]=True;masks['mouth'][40:42,30:34]=True
        base=self.root/'rig2d-base';(base/'parts').mkdir(parents=True)
        self.args.base_rig=base/'rig.json'
        rig={'canvas':{'width':64,'height':64},'scene_graph':[{'role':'hair','layer':'scene_hair'}],
             'layers':{name:{'texture_box':[0,0,64,64]} for name in ('scene_face','scene_hair')}}
        rig['layers']['face']={'bbox':[16,16,48,48]}
        self.args.base_rig.write_text(json.dumps(rig))
        self.prepared.convert('RGBA').save(base/'parts/scene_face.png')
        hair_pixels=np.array(self.prepared.convert('RGBA'));hair_pixels[~mask,3]=0
        Image.fromarray(hair_pixels).save(base/'parts/scene_hair.png')
        (self.root/'source').mkdir();self.prepared.convert('RGBA').save(self.root/'source/isolated.png')
        def segment(image,detector,sam,threshold,context,source_reference,emit):
            events.append('original' if source_reference else 'generated')
            if source_reference and threshold > .3:
                return [mask.copy(),np.zeros_like(mask)],{'status':'partial'}
            if not source_reference:
                return [np.zeros_like(mask),mask.copy()],{'status':'partial'}
            return [mask.copy(),mask.copy()],{'status':'complete'}
        def infer(*values):
            events.append('edit');return self.infer(*values)
        generation={'runtime':{},'comfy_code':{},'workflow':'sha','models':models}
        with patch.object(hidden_bridge,'segment_ear_image',side_effect=segment):
            result=hidden_bridge.prepare_hidden(self.args,self.prepared,[0,0,64,64],self.source,generation,
                    lambda:None,infer,lambda *a,**k:None,original,masks)
            self.assertEqual(events,['original','edit','edit','generated'])
            self.assertTrue(result['ears']['source_ears'].any())
            self.assertTrue(result['ears']['generated_ears'].any())
            hidden_bridge.assert_hidden_sources(self.args,result,lambda:None)
            hidden_bridge.prepare_hidden(self.args,self.prepared,[0,0,64,64],self.source,generation,
                    lambda:None,infer,lambda *a,**k:None,original,masks)
            self.assertEqual(events,['original','edit','edit','generated'])
            self.args.grounding_threshold=.4
            partial=hidden_bridge.prepare_hidden(self.args,self.prepared,[0,0,64,64],self.source,generation,
                    lambda:None,infer,lambda *a,**k:None,original,masks)
            self.assertFalse(partial['ears']['source_ears'].any())
            self.assertFalse(partial['ears']['generated_ears'].any())
            with np.load(self.root/'completion-original-ears/masks.npz') as raw:
                self.assertTrue(raw['left'].any());self.assertFalse(raw['right'].any())
            with self.assertRaisesRegex(ValueError,'耳解析素材'):
                hidden_bridge.assert_hidden_sources(self.args,result,lambda:None)

    def test_two_caches_do_not_touch_closed_eyes_and_reuse_independently(self):
        first=self.run_edits();self.assertEqual(len(self.calls),2)
        second=self.run_edits();self.assertEqual(len(self.calls),2)
        self.assertEqual((self.root/'completion-source/edited.png').read_bytes(),b'closed-eye-sentinel')
        self.args.side_prompt+=' Keep shading.'
        self.run_edits();self.assertEqual(self.calls,['hidden-face','side-ears','side-ears'])
        with self.assertRaisesRegex(ValueError,'SHA不一致'):assert_result_sources(second,lambda:None)

    def test_analysis_only_change_reuses_two_raws_without_rewriting_origin(self):
        first=self.run_edits()
        before={job:(self.root/JOBS[job]['cache_directory']/'manifest.json').read_bytes() for job in JOBS}
        self.source['analysis/analysis.json']=sha(b'new collar parser')
        self.source['analysis/masks.npz']=sha(b'new arm mask')
        second=self.run_edits();self.assertEqual(len(self.calls),2)
        for job in JOBS:
            self.assertEqual((self.root/JOBS[job]['cache_directory']/'manifest.json').read_bytes(),before[job])
            self.assertEqual(second[job][2].origin(),first[job][2].origin())
        assert_result_sources(second,lambda:None)

    def test_second_job_failure_preserves_first_and_retry_skips_first(self):
        self.fail='side-ears'
        with self.assertRaises(RuntimeError):self.run_edits()
        self.assertTrue((self.root/'completion-hidden-source/edited.png').is_file())
        self.assertFalse((self.root/'completion-side-source/edited.png').exists())
        self.fail=None;self.run_edits()
        self.assertEqual(self.calls,['hidden-face','side-ears','side-ears'])

    def test_previous_mask_version_is_not_adopted_as_new_generation(self):
        self.run_edits()
        marker=self.root/'completion-hidden-source/manifest.json'
        record=json.loads(marker.read_text());self.assertEqual(record['identity']['masked_generation_version'],3)
        record['identity']['masked_generation_version']=2;marker.write_text(json.dumps(record))
        self.run_edits();self.assertEqual(self.calls,['hidden-face','side-ears','hidden-face'])

    def test_ear_cache_checks_input_and_releases_engine_before_segment(self):
        path=self.root/'source.png';self.prepared.save(path)
        events=[]
        def segment(image,source_reference):
            events.append('segment')
            return [np.ones((64,64),bool)]*2,{'status':'complete'}
        def run():
            return ear_analysis(self.root/'ear-cache',path,False,{'models':'fixed','threshold':.2},
                segment,lambda:events.append('comfy_stopped'),lambda:None)
        run();run();self.assertEqual(events,['comfy_stopped','segment'])
        Image.new('RGB',(64,64),(90,80,70)).save(path)
        run();self.assertEqual(events,['comfy_stopped','segment','comfy_stopped','segment'])

    def test_source_ear_absence_is_explicit_not_fake_coordinates(self):
        detections={'face':[([10,10,50,50],.9)],'ear':[]}
        self.assertEqual(choose_ears(detections,True),[None,None])
        with self.assertRaisesRegex(ValueError,'一側も'):
            choose_ears(detections,False)

    def test_generated_one_side_is_partial_without_fake_coordinates(self):
        detections={'face':[([10,10,90,90],.8)],'ear':[([75,35,85,55],.5)]}
        boxes=choose_ears(detections,False)
        self.assertIsNone(boxes[0]);self.assertEqual(boxes[1],detections['ear'][0])



if __name__=='__main__':unittest.main()
