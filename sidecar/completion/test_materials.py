"""閉眼の線を原寸で測定し、線の無い画像を拒否する。"""
import unittest
import numpy as np
from materials import closed_curve,extract_lid_ink,validate_masked_pixels,reconstruct_closed_skin,replace_closed_backing


class ClosedPreviewTests(unittest.TestCase):
    def test_masked_output_rejects_changes_outside_the_edit(self):
        original=np.full((12,16,3),120,np.uint8);edited=original.copy()
        mask=np.zeros((12,16),np.uint8);mask[4:8,5:11]=255
        edited[mask>0]=20;validate_masked_pixels(edited,original,mask)
        edited[0,0]=123
        with self.assertRaisesRegex(ValueError,'マスク外'):validate_masked_pixels(edited,original,mask)

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

    def test_diagonal_single_pixel_lid_keeps_its_measured_tilt(self):
        pixels=np.full((80,100,4),220,dtype=np.uint8)
        for x in range(15,85):pixels[15+round((x-15)*.65),x,:3]=20
        aperture=[[float(x),10,70,40] for x in range(20,80)]
        result=closed_curve(pixels,[10,10,90,70],np.zeros((80,100),bool),aperture)
        self.assertGreater(result[-1]-result[0],35)
        self.assertLess(np.abs(np.asarray(result)-(15+(np.arange(20,80)-15)*.65+.5)).max(),1)

    def test_long_attached_lash_does_not_make_the_entire_closed_line_thick(self):
        pixels=np.full((80,100,4),220,dtype=np.uint8)
        for x in range(15,85):
            y=56-round((x-15)*.25);pixels[y:y+3,x,:3]=20
        pixels[10:59,17:20,:3]=20
        metrics={};aperture=[[float(x),10,70,40] for x in range(25,80)]
        result=closed_curve(pixels,[10,10,90,70],np.zeros((80,100),bool),aperture,metrics)
        self.assertGreater(metrics['max_column_thickness'],40)
        self.assertGreater(metrics['trimmed_columns'],0)
        self.assertLess(metrics['median_column_thickness'],5)
        self.assertLess(result[-1],result[0]-10)
        self.assertGreaterEqual(metrics['retained_columns']/metrics['component_columns'],.6)

    def test_open_eye_ring_or_large_dark_patch_is_not_a_closed_lid(self):
        yy,xx=np.mgrid[:80,:100]
        ellipse=((xx-50)/32)**2+((yy-40)/22)**2
        for dark in ((ellipse<1)&(ellipse>.72),(xx>=15)&(xx<85)&(yy>=22)&(yy<58)):
            pixels=np.full((80,100,4),220,dtype=np.uint8);pixels[dark,:3]=20
            with self.assertRaisesRegex(ValueError,'細長い'):
                closed_curve(pixels,[10,10,90,70],np.zeros((80,100),bool),[[50,10,70,40]])

    def test_generated_skin_keeps_shading_and_does_not_bake_a_second_closed_line(self):
        yy,xx=np.mgrid[:40,:80];skin=np.stack([150+xx*.4+yy*.2,140+xx*.4+yy*.2,130+xx*.4+yy*.2],axis=2)
        line=np.zeros((40,80),bool);line[19:22,20:60]=True
        edited=skin.copy();edited[line]=20
        allowed=np.zeros((40,80),bool);allowed[10:31,10:71]=True
        original=edited.copy();clean,removed,metrics=reconstruct_closed_skin(edited,line,allowed)
        self.assertLess(np.abs(clean-skin).max(),1e-8)
        self.assertTrue(np.array_equal(edited,original))
        self.assertTrue(np.array_equal(clean[~removed],edited[~removed]))
        self.assertGreater(metrics['line_brightening'],100)
        pixels=np.full((40,80,4),255,np.uint8);pixels[:,:,:3]=np.rint(clean).astype(np.uint8)
        ink=extract_lid_ink(pixels,edited,removed.astype(float))
        self.assertTrue(np.all(ink[~removed,3]==0))
        # 半閉眼で動かすのはinkだけ。静止下地には元の閉眼位置の暗線を残さない。
        self.assertGreater(clean[line].min(),130)
        self.assertGreater(int(ink[line,3].min()),150)

    def test_backing_preserves_protected_rgb_and_does_not_cover_outside_the_mask(self):
        original=np.full((8,10,4),200,np.uint8);original[:,:,3]=254
        clean=np.full((8,10,3),155.);weight=np.zeros((8,10));weight[2:6,2:8]=1
        protected=np.zeros((8,10),bool);protected[3,4]=True
        result=replace_closed_backing(original,clean,weight,protected)
        hidden=(weight==0)|protected
        self.assertTrue(np.array_equal(result[hidden,:3],original[hidden,:3]))
        self.assertTrue(np.all(result[hidden,3]==0))
        self.assertTrue(np.all(result[~hidden,:3]==155))
        self.assertTrue(np.all(result[~hidden,3]==254))
        self.assertTrue(np.all(original[:,:,:3]==200))

    def test_skin_reconstruction_rejects_missing_donors_or_unverified_dark_line(self):
        edited=np.full((12,16,3),150.)
        line=np.zeros((12,16),bool);line[5:7,4:12]=True
        with self.assertRaisesRegex(ValueError,'肌が不足'):
            reconstruct_closed_skin(edited,line,line)
        with self.assertRaisesRegex(ValueError,'分離できません'):
            reconstruct_closed_skin(edited,line,np.ones((12,16),bool))


if __name__ == '__main__':
    unittest.main()
