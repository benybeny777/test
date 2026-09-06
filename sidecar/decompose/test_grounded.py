"""意味候補の包含関係と未検出時の扱いを固定する。"""
import unittest
import numpy as np
from grounded import select_boxes, iris_white_points, coarse_pupil_box, eye_context, semantic_components


class GroundedSelectionTests(unittest.TestCase):
    def test_disconnected_clothing_is_not_discarded(self):
        mask=np.zeros((20,30),bool);mask[3:15,2:9]=True;mask[3:14,20:26]=True
        retained,info=semantic_components(mask,'clothes')
        np.testing.assert_array_equal(retained,mask)
        self.assertEqual(info['discarded_pixels'],0)
        single,info=semantic_components(mask,'face')
        self.assertEqual(int(single.sum()),84)
        self.assertEqual(info['discarded_pixels'],66)
        with self.assertRaises(ValueError):semantic_components(np.zeros_like(mask),'clothes')

    def test_eye_context_preserves_source_coordinates(self):
        self.assertEqual(eye_context([20,30,40,50],(100,100),.5),[10,20,50,60])
        self.assertEqual(eye_context([27,41,47,61],(107,111),.5),[17,31,57,71])
        self.assertEqual(eye_context([0,0,20,20],(25,25),.5),[0,0,25,25])
        for margin in (0,float('nan'),3):
            with self.assertRaises(ValueError):eye_context([20,30,40,50],(100,100),margin)

    def test_fine_pupil_does_not_erase_white_highlights(self):
        self.assertFalse(coarse_pupil_box([15,10,25,25],[10,10,30,25]))
        self.assertTrue(coarse_pupil_box([10,10,30,25],[10,10,30,25]))

    def test_white_exclusion_points_follow_the_measured_eye(self):
        image=np.full((30,60,4),(40,30,20,255),np.uint8)
        eye=np.zeros((30,60),bool);eye[8:22,10:50]=True
        image[13,13,:3]=245;image[13,46,:3]=245
        points=iris_white_points(image,eye)
        self.assertEqual(points,[[13.,13.],[46.,13.]])
        shifted=iris_white_points(np.pad(image,((7,0),(5,0),(0,0))),np.pad(eye,((7,0),(5,0))))
        self.assertEqual(shifted,[[18.,20.],[51.,20.]])
        with self.assertRaisesRegex(ValueError,'空'):
            iris_white_points(image,np.zeros_like(eye))

    def test_whole_person_mouth_is_rejected(self):
        records={'mouth':[{'box':[0,0,100,300],'score':.99},
                          {'box':[40,55,60,60],'score':.4}]}
        self.assertEqual(select_boxes(records,'mouth',[20,10,80,80])[0]['box'],[40,55,60,60])

    def test_missing_is_not_filled(self):
        self.assertEqual(select_boxes({},'mouth',[20,10,80,80]),[])

    def test_eye_order_does_not_depend_on_detection_order(self):
        eyes=[{'box':[55,25,70,35],'score':.9},{'box':[30,25,45,35],'score':.7}]
        self.assertEqual(select_boxes({'eyes':eyes},'eyes',[20,10,80,80]),list(reversed(eyes)))

    def test_translation_preserves_selection(self):
        original={'box':[40,55,60,60],'score':.4}
        shifted={'box':[77,74,97,79],'score':.4}
        self.assertEqual(select_boxes({'mouth':[shifted]},'mouth',[57,29,117,99]),[shifted])
        self.assertEqual(select_boxes({'mouth':[original]},'mouth',[20,10,80,80]),[original])

    def test_same_eye_duplicates_do_not_become_two_eyes(self):
        eye={'box':[30,25,45,35],'score':.7}
        self.assertEqual(len(select_boxes({'eyes':[eye,eye]},'eyes',[20,10,80,80])),1)

    def test_pupils_choose_small_inner_regions_on_both_sides(self):
        left={'box':[33,28,40,35],'score':.3}
        right={'box':[60,28,67,35],'score':.3}
        records={'pupils':[{'box':[0,0,100,100],'score':.99},
                           {'box':[28,24,45,38],'score':.8},right,left,
                           {'box':[55,24,72,38],'score':.8}]}
        self.assertEqual(select_boxes(records,'pupils',[20,10,80,80]),[left,right])
        self.assertEqual(select_boxes({'pupils':[left,left]},'pupils',[20,10,80,80]),[left])
