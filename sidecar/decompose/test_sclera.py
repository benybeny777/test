"""白目の隠れ領域だけを補完する条件を固定する。"""
import unittest
import numpy as np
from sclera import eye_backplate


class ScleraTests(unittest.TestCase):
    def test_preserves_visible_pixels_alpha_and_translation(self):
        image=np.full((20,32,4),(220,215,210,255),np.uint8)
        eye=np.zeros((20,32),bool);eye[4:16,4:28]=True
        iris=np.zeros_like(eye);iris[5:15,12:20]=True
        image[iris,:3]=(20,110,40)
        result=eye_backplate(image,eye,iris)
        np.testing.assert_array_equal(result[eye & ~iris],image[eye & ~iris])
        np.testing.assert_array_equal(result[iris,:3],np.tile([220,215,210],(iris.sum(),1)))
        self.assertFalse(result[~eye].any())
        np.testing.assert_array_equal(result[:,:,3],image[:,:,3]*eye)
        shifted=eye_backplate(np.pad(image,((7,0),(9,0),(0,0))),np.pad(eye,((7,0),(9,0))),np.pad(iris,((7,0),(9,0))))
        np.testing.assert_array_equal(shifted[7:,9:],result)

    def test_missing_white_reference_is_explicit_failure(self):
        image=np.full((5,5,4),255,np.uint8);eye=np.ones((5,5),bool)
        with self.assertRaisesRegex(ValueError,'参照'):
            eye_backplate(image,eye,eye)
