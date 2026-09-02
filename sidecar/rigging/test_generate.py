import importlib.util
import sys
import unittest
from pathlib import Path

import numpy as np


MODULE_PATH = Path(__file__).with_name("generate.py")
SPEC = importlib.util.spec_from_file_location("rig_generate", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class RiggingTests(unittest.TestCase):
    def test_rejects_non_a_pose(self):
        vertices = np.array(
            [[-0.1, 0, 0], [0.1, 0, 0], [-0.1, 1.7, 0], [0.1, 1.7, 0]],
            dtype=np.float32,
        )
        with self.assertRaisesRegex(ValueError, "腕幅"):
            MODULE.validate_humanoid_pose(vertices)

    def test_accepts_repeated_hand_extrema(self):
        vertices = np.repeat(
            np.array(
            [
                [-0.7, 0.82, 0], [-0.7, 0.82, 0], [0.7, 0.82, 0], [0.7, 0.82, 0],
                [-0.2, 0, 0], [0.2, 0, 0], [-0.2, 1.7, 0], [0.2, 1.7, 0],
            ],
            dtype=np.float32,
            ),
            4,
            axis=0,
        )
        metrics = MODULE.validate_humanoid_pose(vertices)
        self.assertAlmostEqual(metrics["left_hand_height_ratio"], 0.4824)
        self.assertAlmostEqual(metrics["right_hand_height_ratio"], 0.4824)

    def test_template_has_all_required_vrm_bones_in_range(self):
        rng = np.random.default_rng(42)
        torso = np.column_stack(
            (rng.uniform(-0.18, 0.18, 500), rng.uniform(0.0, 1.7, 500), rng.uniform(-0.1, 0.1, 500))
        )
        arms = np.array(
            [
                [0.2, 1.32, 0], [0.45, 1.1, 0], [0.7, 0.9, 0],
                [-0.2, 1.32, 0], [-0.45, 1.1, 0], [-0.7, 0.9, 0],
            ],
            dtype=np.float32,
        )
        vertices = np.vstack((torso, np.repeat(arms, 10, axis=0)))
        bones = MODULE.estimate_bones(vertices)
        names = {bone.name for bone in bones}
        self.assertTrue(set(MODULE.REQUIRED_BONES).issubset(names))
        for bone in bones:
            self.assertGreaterEqual(float(bone.position[1]), 0.0)
            self.assertLessEqual(float(bone.position[1]), 1.7)

    def test_heat_weights_are_four_influences_and_normalized(self):
        vertices = np.array(
            [[0, 0, 0], [1, 0, 0], [0, 1, 0], [1, 1, 0]], dtype=np.float32
        )
        faces = np.array([[0, 1, 2], [1, 3, 2]])
        bones = [
            MODULE.Bone(str(i), None, np.array([i / 5, 0, 0]), np.array([i / 5, 1, 0]))
            for i in range(5)
        ]
        joints, weights = MODULE.automatic_heat_weights(vertices, faces, bones, iterations=2)
        self.assertEqual(joints.shape, (4, 4))
        self.assertEqual(weights.shape, (4, 4))
        np.testing.assert_allclose(weights.sum(axis=1), 1.0, atol=1e-6)

    def test_vrm_axis_normalization_is_y_up_and_1_7_meters(self):
        source = np.array([[0, -0.2, -0.5], [0.1, 0.2, 0.5]], dtype=np.float32)
        normalized, _ = MODULE.normalize_to_vrm_axes(source)
        self.assertAlmostEqual(float(np.ptp(normalized[:, 1])), 1.7, places=5)
        self.assertEqual(float(normalized[:, 1].min()), 0.0)

    def test_uv_origin_is_converted_for_gltf(self):
        source = np.array([[0.25, 0.1], [0.75, 0.9]], dtype=np.float32)
        converted = MODULE.uv_to_gltf(source)
        np.testing.assert_allclose(converted, [[0.25, 0.9], [0.75, 0.1]], atol=1e-7)
        np.testing.assert_allclose(source, [[0.25, 0.1], [0.75, 0.9]])


if __name__ == "__main__":
    unittest.main()
