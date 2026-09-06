"""比較の入力保護と標準ノードの条件を検査する。GPU生成の合格には代用しない。"""
import json
from pathlib import Path
import tempfile
import unittest

import numpy as np
from PIL import Image
from run import graph,prepare_input,ROOT,verify_source,measured_head_region


class CompareTests(unittest.TestCase):
    def test_measured_head_is_native_translation_invariant_and_bounded(self):
        box=measured_head_region([500,200,620,330],[530,320,590,350],(1200,1400),1024)
        shifted=measured_head_region([600,300,720,430],[630,420,690,450],(1200,1400),1024)
        self.assertEqual(shifted,tuple(value+100 for value in box))
        self.assertEqual(box[2]-box[0],256)
        with self.assertRaises(ValueError):measured_head_region([0,0,1000,1000],[0,900,100,1100],(2000,2000),1024)

    def test_original_precision_and_layers(self):
        value=graph('layered',1024,1024,'test',50,777,4)
        self.assertEqual(value['1']['inputs']['weight_dtype'],'default')
        self.assertIn('bf16',value['1']['inputs']['unet_name'])
        self.assertEqual(value['9']['inputs']['layers'],4)
        self.assertEqual(value['11']['class_type'],'LatentCutToBatch')
        self.assertNotIn('LatentCut',[node['class_type'] for node in value.values()])
        for node in value.values():
            for item in node['inputs'].values():
                if isinstance(item,list):self.assertIn(item[0],value)

    def test_edit_is_reference_conditioned(self):
        value=graph('edit',1024,1024,'test',50,777,4)
        self.assertEqual(value['7']['class_type'],'TextEncodeQwenImageEditPlus')
        self.assertEqual(value['7']['inputs']['image1'],['4',0])
        self.assertEqual(value['8']['inputs']['image1'],['4',0])
        self.assertEqual(value['10']['inputs']['seed'],777)

    def test_head_roi_has_no_resampling(self):
        (ROOT/'temp').mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=ROOT/'temp') as path:
            root=Path(path);character=root/'character';output=root/'compare'
            for target in [character/'source',character/'analysis',output/'input']:target.mkdir(parents=True)
            pixels=np.random.default_rng(0).integers(0,256,(1056,1104,4),dtype=np.uint8)
            pixels[:,:,3]=255
            source=Image.fromarray(pixels)
            source.save(character/'source/input.png');source.save(character/'source/isolated.png')
            (character/'analysis/analysis.json').write_text(json.dumps({'analysis':{'selected':{'face':{'box':[400,100,700,500]},'neck':{'box':[450,500,650,650]}}}}))
            np.savez(character/'analysis/masks.npz',face=np.ones((1056,1104),dtype=bool))
            size,metadata=prepare_input(character,output,1024,'head')
            with Image.open(output/'reference.png') as image:reference=np.asarray(image)
            left,top,right,bottom=metadata['source_region']
            np.testing.assert_array_equal(reference,pixels[top:bottom,left:right])
            self.assertEqual(size,(1024,1024))
            self.assertFalse(metadata['upscaled'])
            verify_source(character,metadata)
            np.savez(character/'analysis/masks.npz',face=np.zeros((1056,1104),dtype=bool))
            with self.assertRaisesRegex(ValueError,'マスク'):verify_source(character,metadata)


if __name__=='__main__':unittest.main()
