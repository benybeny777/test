import importlib.util
import json
import sys
import tempfile
import unittest
from unittest.mock import patch
from pathlib import Path

import numpy as np
from PIL import Image


MODULE_PATH = Path(__file__).with_name("generate.py")
sys.path.insert(0, str(MODULE_PATH.parent))
SPEC = importlib.util.spec_from_file_location("decompose_generate", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class SemanticDecompositionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        rgba = np.zeros((120, 80, 4), dtype=np.uint8)
        rgba[5:116, 28:52] = (180, 120, 100, 255)
        rgba[30:75, 8:72] = (60, 80, 140, 255)
        rgba[13:15, 31:35, :3] = 20
        rgba[13:15, 45:49, :3] = 20
        rgba[24:26, 37:43, :3] = (100, 20, 30)
        self.source_alpha = rgba[:, :, 3] > 0
        self.input_path = self.root / "input.png"
        Image.fromarray(rgba, mode="RGBA").save(self.input_path)
        self.masks = self.root / "masks"
        self.masks.mkdir()
        boxes = (
            (0, 0, 80, 36),
            (18, 28, 62, 120),
            (0, 28, 32, 85),
            (48, 28, 80, 85),
            (20, 4, 60, 40),
            (18, 4, 62, 25),
            (25, 17, 39, 27),
            (41, 17, 55, 27),
            (33, 28, 47, 34),
        )
        for index, (left, top, right, bottom) in enumerate(boxes):
            mask = np.zeros((120, 80), dtype=np.uint8)
            mask[top:bottom, left:right] = 255
            Image.fromarray(mask, mode="L").save(self.masks / f"{index:02}.png")
        hair = np.zeros((120, 80), dtype=np.uint8)
        hair[5:13,28:52] = 255
        hair[5:29,28:30] = 255
        hair[5:29,50:52] = 255
        Image.fromarray(hair).save(self.masks / 'hair.png')

    def tearDown(self):
        self.temp.cleanup()

    @patch('features.locate_features', return_value={
        'left_eye': [31, 12, 35, 16], 'right_eye': [45, 12, 49, 16],
        'mouth': [37, 23, 43, 27],
    })
    def test_writes_deterministic_manifest_and_clipped_layers(self, _detector):
        output = self.root / "layers"
        manifest_path = MODULE.decompose_image(
            self.input_path, output, candidate_masks_dir=self.masks
        )
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        self.assertEqual(manifest["canvas"], {"width": 80, "height": 120})
        self.assertFalse(Path(manifest["source"]).is_absolute())
        self.assertEqual(
            [part["name"] for part in manifest["parts"]],
            [spec.name for spec in MODULE.PART_SPECS],
        )
        self.assertTrue((output / "source.psd").is_file())
        union = np.zeros_like(self.source_alpha)
        for part in manifest["parts"]:
            with Image.open(output / part["path"]) as opened:
                layer = np.asarray(opened.convert("RGBA"))
            self.assertEqual(layer.shape, (120, 80, 4))
            self.assertFalse((layer[:, :, 3][~self.source_alpha] > 0).any())
            union |= layer[:, :, 3] > 0
        self.assertTrue(np.array_equal(union, self.source_alpha))

        first = manifest_path.read_bytes()
        MODULE.decompose_image(
            self.input_path, output, candidate_masks_dir=self.masks
        )
        self.assertEqual(manifest_path.read_bytes(), first)

    def test_rejects_empty_input(self):
        empty = self.root / "empty.png"
        Image.new("RGBA", (32, 32), (0, 0, 0, 0)).save(empty)
        with self.assertRaisesRegex(ValueError, "被写体"):
            MODULE.decompose_image(
                empty, self.root / "empty-output", candidate_masks_dir=self.masks
            )

    def test_rejects_wrong_candidate_size(self):
        wrong = self.root / "wrong"
        wrong.mkdir()
        Image.new("L", (10, 10), 255).save(wrong / "mask.png")
        with self.assertRaisesRegex(ValueError, "寸法"):
            MODULE.decompose_image(
                self.input_path, self.root / "wrong-output", candidate_masks_dir=wrong
            )


if __name__ == "__main__":
    unittest.main()
