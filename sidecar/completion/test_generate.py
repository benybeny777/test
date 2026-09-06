"""補完の署名、入力保持、破損拒否、排他をCPUだけで検証する。"""
import json
from pathlib import Path
import tempfile
import unittest

from generate import cache_valid, tree_hashes, generation_lock, source_hashes
from regions import eye_edit_mask, measured_head_region
import numpy as np


class CompletionTests(unittest.TestCase):
    def setUp(self):
        root = Path(__file__).resolve().parents[2]/'temp'
        root.mkdir(exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(dir=root)
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def test_cache_rejects_missing_or_modified_materials(self):
        output = self.root/'result'; output.mkdir()
        (output/'rig.json').write_text('{}', encoding='utf-8')
        identity = {'version': 1, 'source': 'first'}
        record = {'identity': identity, 'outputs': tree_hashes(output)}
        (output/'completion.json').write_text(json.dumps(record), encoding='utf-8')
        self.assertTrue(cache_valid(output, identity))
        self.assertFalse(cache_valid(output, {'version': 1, 'source': 'different'}))
        (output/'rig.json').write_text('{"changed":true}', encoding='utf-8')
        with self.assertRaisesRegex(ValueError, '改変'):
            cache_valid(output, identity)
        (output/'rig.json').unlink()
        with self.assertRaisesRegex(ValueError, '欠落'):
            cache_valid(output, identity)

    def test_input_identity_includes_analysis_not_only_original(self):
        for name in ('source/input.png', 'source/isolated.png', 'analysis/analysis.json', 'analysis/masks.npz'):
            path = self.root/name; path.parent.mkdir(exist_ok=True)
            path.write_bytes(b'original')
        before = source_hashes(self.root)
        (self.root/'analysis/masks.npz').write_bytes(b'new mask')
        after = source_hashes(self.root)
        self.assertEqual(before['source/input.png'], after['source/input.png'])
        self.assertNotEqual(before, after)

    def test_two_characters_cannot_generate_simultaneously(self):
        a = self.root/'a'; b = self.root/'b'
        a.mkdir(); b.mkdir()
        with generation_lock(a):
            with self.assertRaises(OSError):
                with generation_lock(b):
                    self.fail('補完の排他が効いていません')
        with generation_lock(b):
            pass

    def test_native_roi_never_resizes_to_fit_limit(self):
        bounds = measured_head_region([300, 250, 500, 450], [340, 440, 470, 510], (1000, 1200), 1024)
        self.assertEqual((bounds[2]-bounds[0]) % 16, 0)
        with self.assertRaisesRegex(ValueError, '原寸'):
            measured_head_region([300, 250, 500, 450], [340, 440, 470, 510], (1000, 1200), 256)

    def test_edit_mask_preserves_hair_and_transparency(self):
        eye = np.zeros((40, 60), bool); eye[15:25, 20:40] = True
        hair = np.zeros_like(eye); hair[:, 20] = True
        alpha = np.full(eye.shape, 255, np.uint8); alpha[:, 39] = 0
        mask = eye_edit_mask([eye], hair, alpha, .2)
        self.assertTrue(np.any(mask > 0))
        self.assertTrue(np.all(mask[hair | (alpha == 0)] == 0))


if __name__ == '__main__':
    unittest.main()
