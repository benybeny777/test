import importlib.util
import unittest
from pathlib import Path

import numpy as np


MODULE_PATH = Path(__file__).with_name("generate.py")
SPEC = importlib.util.spec_from_file_location("mesh_generate", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class MeshInputTests(unittest.TestCase):
    def test_isnet_input_preserves_aspect_ratio_and_centers_padding(self):
        image = np.full((200, 100, 3), 255, dtype=np.uint8)
        tensor, placement = MODULE.prepare_isnet_input(MODULE.Image.fromarray(image))
        self.assertEqual(tensor.shape, (1, 3, 1024, 1024))
        self.assertEqual(placement, (0, 256, 1024, 512))
        self.assertEqual(float(tensor[:, :, :, :256].max()), 0.0)
        self.assertEqual(float(tensor[:, :, :, 256:768].min()), 1.0)

    def test_foreground_is_centered_at_requested_ratio(self):
        rgba = np.zeros((100, 80, 4), dtype=np.uint8)
        rgba[10:90, 20:60] = 255
        framed = np.asarray(MODULE.frame_foreground(MODULE.Image.fromarray(rgba), 0.8))
        points = np.argwhere(framed[:, :, 3] > 0)
        height = points[:, 0].max() - points[:, 0].min() + 1
        self.assertEqual(framed.shape, (100, 100, 4))
        self.assertEqual(height / framed.shape[0], 0.8)

    def test_rasterizes_triangle_position_with_uv_y_flip(self):
        vertices = np.array([[0, 0, 1], [1, 0, 2], [0, 1, 3]], dtype=np.float32)
        faces = np.array([[0, 1, 2]])
        uvs = np.array([[0, 0], [1, 0], [0, 1]], dtype=np.float32)
        positions, valid = MODULE.rasterize_position_atlas(vertices, faces, uvs, 5)
        self.assertTrue(valid[3, 1])
        self.assertAlmostEqual(float(positions[3, 1, 2]), 1.75, places=5)

    def test_accepts_centered_full_body_a_pose_silhouette(self):
        alpha = np.zeros((100, 100), dtype=np.uint8)
        alpha[10:95, 40:60] = 255
        alpha[30:65, 25:75] = 255
        metrics = MODULE.validate_full_body_a_pose(alpha)
        self.assertGreater(metrics["height_ratio"], 0.8)
        self.assertGreater(metrics["width_ratio"], 0.4)

    def test_rejects_face_only_input(self):
        alpha = np.zeros((100, 100), dtype=np.uint8)
        alpha[10:55, 25:75] = 255
        with self.assertRaisesRegex(ValueError, "全身"):
            MODULE.validate_full_body_a_pose(alpha)

    def test_rejects_arms_kept_against_body(self):
        alpha = np.zeros((100, 100), dtype=np.uint8)
        alpha[5:95, 43:57] = 255
        with self.assertRaisesRegex(ValueError, "腕"):
            MODULE.validate_full_body_a_pose(alpha)


if __name__ == "__main__":
    unittest.main()
