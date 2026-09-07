import unittest
import numpy as np
from hidden_materials import HiddenSettings, extract_hidden_roi, extract_ear_repair, ear_policy, validate_neutral
from hidden_assemble import composite, same_visible_rgba
from hidden_jobs import generation_identity


class DraftTests(unittest.TestCase):
    def test_visible_rgba_ignores_only_transparent_rgb(self):
        source=np.array([[[12,34,56,0],[7,8,9,255]]],dtype=np.uint8)
        composed=np.array([[[0,0,0,0],[7,8,9,255]]],dtype=np.uint8)
        self.assertTrue(same_visible_rgba(source,composed))
        alpha=composed.copy();alpha[0,0,3]=1
        self.assertFalse(same_visible_rgba(source,alpha))
        visible=composed.copy();visible[0,1,0]=6
        self.assertFalse(same_visible_rgba(source,visible))

    def test_alpha254_hidden_and_ear_preserve_original_alpha(self):
        source = np.full((80, 80, 4), 180, np.uint8); source[:, :, 3] = 254
        before = source.copy(); generated = source.copy(); generated[:, :, :3] = 120
        face = np.zeros((80, 80), bool); face[20:60, 20:60] = True
        hair = ~face
        features = [np.zeros_like(face) for _ in range(3)]
        features[0][30:34, 29:33] = True; features[1][30:34, 45:49] = True; features[2][48:51, 36:42] = True
        bundle = extract_hidden_roi(source, generated, generated, face, hair, features, 40, HiddenSettings(.08,.35,.015,40))
        hidden = bundle['hidden_mask']
        self.assertTrue(hidden.any())
        self.assertTrue(np.all(bundle['hidden_rgba'][hidden,3] == 254))
        # 最大変位より広い支持帯を確保する。変形後の被覆は描画側で検査する。
        self.assertGreater(bundle['radius_px'], bundle['radius_px']*.35)
        ear = np.zeros_like(face); ear[37:43,18:23] = True
        repair = extract_ear_repair(source, generated, face|hidden,hair,features,ear,ear,3)
        active = repair['allowed_visible_change']; after = source.copy(); after[active] = repair['rgba'][active]
        self.assertGreater(validate_neutral(source,after,active,features),0)
        self.assertTrue(np.array_equal(source,before))
        rig={'canvas':{'width':80,'height':80},'hidden_motion':{'version':1},
             'scene_graph':[{'layer':'base','role':'face'},{'layer':'hidden','role':'hidden_face'}],
             'layers':{'base':{'texture_box':[0,0,80,80]},'hidden':{'texture_box':[0,0,80,80]}}}
        self.assertTrue(np.array_equal(composite(rig,{'base':source,'hidden':bundle['hidden_rgba']}),source))
        after[active,3] = 255
        with self.assertRaises(ValueError):validate_neutral(source,after,active,features)

    def test_hidden_pixels_are_confined_and_input_untouched(self):
        source = np.full((80, 80, 4), 180, np.uint8); source[:, :, 3] = 255
        before = source.copy(); generated = source.copy(); generated[:, :, :3] = 120
        face = np.zeros((80, 80), bool); face[20:60, 20:60] = True
        hair = ~face
        features = [np.zeros_like(face) for _ in range(3)]
        features[0][30:34, 29:33] = True; features[1][30:34, 45:49] = True; features[2][48:51, 36:42] = True
        result = extract_hidden_roi(source, generated, generated, face, hair, features, 40,
                                    HiddenSettings(.08, .35, .015, 40))
        self.assertTrue(np.array_equal(source, before))
        self.assertTrue(result['hidden_mask'].any())
        self.assertFalse((result['hidden_mask'] & face).any())
        self.assertTrue(np.all(result['hidden_rgba'][~hair, 3] == 0))

    def test_missing_original_ears_never_authorizes_visible_redraw(self):
        ear = np.ones((10, 10), bool)
        result = ear_policy(None, ear)
        self.assertEqual(result['mode'], 'hidden-only')
        self.assertEqual(result['status'], 'partial')
        self.assertIsNotNone(result['warning'])
        with self.assertRaises(ValueError):
            ear_policy(None, ear, required_contour=True)

    def test_neutral_guard_rejects_unrelated_or_feature_edits(self):
        before = np.full((10, 10, 4), 255, np.uint8); after = before.copy()
        allowed = np.zeros((10, 10), bool); allowed[2:5, 2:5] = True
        feature = np.zeros_like(allowed); feature[3, 3] = True
        after[2, 2, 0] = 0
        self.assertEqual(validate_neutral(before, after, allowed, [feature]), 1)
        after[3, 3, 0] = 0
        with self.assertRaises(ValueError):
            validate_neutral(before, after, allowed, [feature])

    def test_generation_requires_actual_prompt_and_complete_source_identity(self):
        source = {name: 'sha' for name in ('source/input.png', 'source/isolated.png', 'analysis/analysis.json', 'analysis/masks.npz')}
        identity=generation_identity('hidden-face',source,[0,0,512,512],'input',{},{},{},{'prompt':'fixture prompt'})
        self.assertEqual(identity['parameters']['prompt'],'fixture prompt')
        with self.assertRaises(ValueError):
            generation_identity('hidden-face',source,[0,0,512,512],'input',{},{},{},{})
        with self.assertRaises(ValueError):
            generation_identity('hidden-face',{},[0,0,512,512],'input',{},{},{},{'prompt':'fixture'})


if __name__ == '__main__':
    unittest.main()
