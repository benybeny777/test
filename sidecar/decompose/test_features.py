"""画像固有の座標に依存しない局所検出と差分境界の回帰検査。"""
import unittest
import numpy as np
from features import locate_features, repair_patch


class FeatureTests(unittest.TestCase):
    def fixture(self):
        image = np.full((200, 180, 4), (215, 183, 170, 255), dtype=np.uint8)
        face = np.zeros((200, 180), dtype=bool)
        face[20:180, 20:160] = True
        image[75:83, 45:70, :3] = 35
        image[75:83, 110:135, :3] = 35
        image[132:136, 75:106, :3] = (130, 40, 55)
        return image, face

    def test_translation_and_repeatability(self):
        image, face = self.fixture()
        expected = locate_features(image, face)
        self.assertEqual(expected, locate_features(image.copy(), face.copy()))
        shifted = np.pad(image, ((37, 19), (53, 11), (0, 0)))
        mask = np.pad(face, ((37, 19), (53, 11)))
        actual = locate_features(shifted, mask)
        for name, box in expected.items():
            self.assertEqual(actual[name], [box[0]+53, box[1]+37, box[2]+53, box[3]+37])

    def test_patch_keeps_boundary_and_alpha(self):
        image, face = self.fixture()
        box = locate_features(image, face)['mouth']
        patch, (l,t,r,b) = repair_patch(image, box)
        original = image[t:b,l:r]
        np.testing.assert_array_equal(patch[[0,-1]], original[[0,-1]])
        np.testing.assert_array_equal(patch[:,[0,-1]], original[:,[0,-1]])
        np.testing.assert_array_equal(patch[:,:,3], original[:,:,3])
        self.assertGreater(int(patch[:,:,:3].sum()), int(original[:,:,:3].sum()))

    def test_featureless_input_is_not_success(self):
        image, face = self.fixture()
        image[:,:,:3] = 180
        with self.assertRaisesRegex(ValueError, '測定できません'):
            locate_features(image, face)

    def test_mouth_tracks_changed_art_instead_of_fixed_canvas_position(self):
        image, face = self.fixture()
        before = locate_features(image, face)['mouth']
        image[132:136, 75:106, :3] = (215, 183, 170)
        image[145:149, 70:111, :3] = (130, 40, 55)
        after = locate_features(image, face)['mouth']
        self.assertAlmostEqual((after[1]+after[3]-before[1]-before[3])/2, 13, delta=1)
        self.assertGreater(after[2]-after[0], before[2]-before[0])


if __name__ == '__main__':
    unittest.main()
