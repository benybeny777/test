"""可視部位の所有・未分類領域・原画の被覆を検査する。"""
import unittest
import numpy as np
from scene import visible_scene


class SceneTests(unittest.TestCase):
    def test_expression_holes_belong_to_face_not_residual(self):
        subject=np.ones((30,30),bool);masks={}
        for index,name in enumerate(('clothes','left_arm','right_arm','neck','face','hair')):
            mask=np.zeros_like(subject);mask[index*4:(index+1)*4,5:25]=True;masks[name]=mask
        support=np.zeros_like(subject);support[18,27]=True
        owners,_=visible_scene(subject,masks,support)
        self.assertTrue(owners['face'][18,27]);self.assertFalse(owners['residual'][18,27])

    def test_unclassified_pixels_do_not_become_torso(self):
        subject=np.ones((30,30),bool);masks={}
        for index,name in enumerate(('clothes','left_arm','right_arm','neck','face','hair')):
            mask=np.zeros_like(subject);mask[index*4:(index+1)*4,5:25]=True;masks[name]=mask
        owners,textures=visible_scene(subject,masks)
        self.assertTrue(owners['residual'][29,29])
        self.assertFalse(owners['torso'][29,29])
        self.assertTrue(np.all(sum(mask.astype(int) for mask in owners.values())==1))
        self.assertTrue(np.logical_or.reduce(list(textures.values())).all())

    def test_missing_part_is_not_silently_skipped(self):
        subject=np.ones((4,4),bool)
        masks={name:subject for name in ('clothes','left_arm','right_arm','neck','face','hair')}
        with self.assertRaisesRegex(ValueError,'空'):
            visible_scene(subject,masks)
