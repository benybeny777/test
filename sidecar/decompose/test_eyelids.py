"""原画由来の閉眼素材の位置・画素・保護領域を検証する。"""
import unittest
import numpy as np
from eyelids import close_eyelid, partition_eye, fit_upper_lid


class EyelidTests(unittest.TestCase):
    def test_fitted_arc_does_not_cross_short_edge_columns(self):
        x=np.linspace(0,1,8);measured=np.array([4,3,1,1,1,1,3,4.])
        bottom=np.array([4.1,10,10,10,10,10,10,4.1])
        coefficients=fit_upper_lid(x,measured,bottom)
        result=np.polynomial.polynomial.polyval(x,coefficients)
        self.assertTrue((result<=bottom+1e-7).all())
        self.assertTrue((result>=0).all())

    def test_iris_partition_preserves_every_eye_pixel_without_overlap(self):
        eye=np.zeros((12,20),bool);eye[2:10,2:18]=True
        iris=np.zeros_like(eye);iris[:,8:12]=True
        measured,remainder=partition_eye(eye,iris)
        np.testing.assert_array_equal(measured | remainder,eye)
        self.assertFalse((measured & remainder).any())
        for invalid in (np.zeros_like(eye),eye):
            with self.assertRaisesRegex(ValueError,'分離'):
                partition_eye(eye,invalid)

    def fixture(self):
        image=np.full((40,60,4),(240,210,190,255),dtype=np.uint8)
        for x in range(10,50):
            y=12+round(3*((x-30)/20)**2)
            image[y:y+2,x,:3]=(20+x,30,40)
        return image

    def test_preserves_varying_source_color_and_translation(self):
        image=self.fixture();clean=np.full_like(image,(240,210,190,255))
        result=close_eyelid(image,clean,[0,0,60,40],[10,10,50,28])
        self.assertGreater(len(np.unique(result[:,:,:3].reshape(-1,3),axis=0)),5)
        np.testing.assert_array_equal(result[:,:,3],image[:,:,3])
        np.testing.assert_array_equal(result[:10],image[:10])
        shifted=np.pad(image,((5,0),(7,0),(0,0)))
        moved=close_eyelid(shifted,clean,[7,5,67,45],[17,15,57,33])
        np.testing.assert_array_equal(result,moved)

    def test_hair_is_not_painted(self):
        image=self.fixture();hair=np.zeros(image.shape[:2],bool);hair[:,25:28]=True
        image[hair,:3]=5
        clean=np.full_like(image,(240,210,190,255));clean[hair]=image[hair]
        result=close_eyelid(image,clean,[0,0,60,40],[10,10,50,28],protected=hair)
        np.testing.assert_array_equal(result[hair],image[hair])

    def test_no_lash_is_an_error(self):
        image=np.full((40,60,4),255,dtype=np.uint8)
        with self.assertRaisesRegex(ValueError,'測定'):
            close_eyelid(image,image.copy(),[0,0,60,40],[10,10,50,28])
