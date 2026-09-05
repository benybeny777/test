"""意味候補の包含関係と未検出時の扱いを固定する。"""
import unittest
from grounded import select_boxes


class GroundedSelectionTests(unittest.TestCase):
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
