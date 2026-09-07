"""透明色の延長が原画とアルファを変えないことを検査する。"""
import unittest
import numpy as np
from PIL import Image
from texture import bleed_transparent_rgb


class TextureTests(unittest.TestCase):
    def test_keeps_visible_pixels_and_alpha(self):
        pixels=np.zeros((9,11,4),np.uint8)
        pixels[2:7,3:8]=[170,120,90,255]
        pixels[2,3]=[180,130,100,64]
        result=np.array(bleed_transparent_rgb(Image.fromarray(pixels)))
        np.testing.assert_array_equal(result[:,:,3],pixels[:,:,3])
        np.testing.assert_array_equal(result[pixels[:,:,3]>0],pixels[pixels[:,:,3]>0])
        self.assertTrue((result[:,:,:3]>0).all())
        self.assertEqual(result.shape,pixels.shape)

    def test_empty_fails(self):
        with self.assertRaises(ValueError):bleed_transparent_rgb(Image.new('RGBA',(3,3)))
