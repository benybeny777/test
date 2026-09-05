"""唇の分割をキャラ座標ではなく原画画素から測る。"""
import unittest
import numpy as np
from lips import trace_lip_seam


class LipSeamTests(unittest.TestCase):
    def test_follows_curved_dark_seam_and_translation(self):
        image=np.full((40,60,4),255,dtype=np.uint8)
        for x in range(10,50):
            y=16+round(3*(1-((x-30)/20)**2))
            image[y,x,:3]=30
        first=trace_lip_seam(image,[10,10,50,28])
        for x,y in first[1:-1]:
            expected=16+round(3*(1-((x-.5-30)/20)**2))+.5
            self.assertLess(abs(y-expected),1)
        shifted=np.pad(image,((5,0),(7,0),(0,0)),constant_values=255)
        second=trace_lip_seam(shifted,[17,15,57,33])
        np.testing.assert_allclose(np.array(second)-[7,5],first)

    def test_featureless_patch_is_not_a_success(self):
        with self.assertRaisesRegex(ValueError,'測定'):
            trace_lip_seam(np.full((20,20,4),255,dtype=np.uint8),[3,3,15,15])
