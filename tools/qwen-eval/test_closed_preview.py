"""閉眼の線を原寸で測定し、線の無い画像を拒否する。"""
import unittest
import numpy as np
from build_eye_preview import closed_curve,extract_lid_ink,validate_base_identity


class ClosedPreviewTests(unittest.TestCase):
    def test_normal_and_derived_base_must_share_source_identity(self):
        source={'source_sha256':'source','analysis_sha256':'analysis'}
        validate_base_identity('original','original',{},source)
        validate_base_identity('original','derived',{'experimental_hidden':{'source':source}},source)
        for rig in ({},{'experimental_hidden':{'source':{'source_sha256':'other'}}}):
            with self.assertRaises(ValueError):validate_base_identity('original','other',rig,source)

    def test_ink_reconstructs_line_without_copying_skin_or_outside_region(self):
        pixels=np.full((8,8,4),220,dtype=np.uint8);pixels[:,:,3]=255
        target=pixels[:,:,:3].astype(float);target[3,2:6]=20
        weight=np.ones((8,8));weight[3,2]=0
        ink=extract_lid_ink(pixels,target,weight)
        self.assertEqual(int((ink[:,:,3]>0).sum()),3)
        alpha=ink[:,:,3:4]/255
        composite=pixels[:,:,:3]*(1-alpha)+ink[:,:,:3]*alpha
        self.assertLessEqual(np.abs(composite[3,3:6]-target[3,3:6]).max(),1)
        self.assertTrue(np.all(pixels[:,:,:3]==220))

    def test_thin_curve_is_measured_without_moving_input(self):
        pixels=np.full((80,100,4),220,dtype=np.uint8);pixels[:,:,3]=255
        for x in range(15,85):
            y=35+round(5*np.sin((x-15)/69*np.pi));pixels[y:y+3,x,:3]=20
        before=pixels.copy();aperture=[[float(x),20,60,50] for x in range(10,90)]
        result=closed_curve(pixels,[10,10,90,70],np.zeros((80,100),bool),aperture)
        self.assertEqual(len(result),len(aperture));self.assertTrue(np.isfinite(result).all())
        self.assertGreater(result[40],result[0]);self.assertTrue(np.array_equal(pixels,before))

    def test_blank_or_protected_image_is_rejected(self):
        pixels=np.full((80,100,4),220,dtype=np.uint8)
        for protected in (np.zeros((80,100),bool),np.ones((80,100),bool)):
            with self.assertRaises(ValueError):closed_curve(pixels,[10,10,90,70],protected,[[20,20,60,50]])
