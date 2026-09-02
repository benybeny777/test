import json
import tempfile
import unittest
from pathlib import Path

import generate
from PIL import Image


class ExpressionWorkflowTests(unittest.TestCase):
    def setUp(self):
        repo = Path(__file__).resolve().parents[2]
        self.template = json.loads(
            (repo / "workflows" / "expression-inpaint-api.json").read_text(
                encoding="utf-8"
            )
        )

    def test_contract_generates_n_plus_six_layers(self):
        self.assertEqual(len(generate.EYE_EXPRESSIONS), 5)
        self.assertEqual(len(generate.VOWELS), 6)
        plan = generate.generation_plan()
        self.assertEqual(len(plan), 11)
        self.assertEqual(sum(kind == "eyes" for kind, *_ in plan), 5)
        self.assertEqual(sum(kind == "mouth" for kind, *_ in plan), 6)

    def test_workflow_uses_only_bundled_standard_nodes(self):
        allowed = {
            "LoadImage",
            "LoadImageMask",
            "CheckpointLoaderSimple",
            "CLIPTextEncode",
            "Canny",
            "ControlNetLoader",
            "ControlNetApplyAdvanced",
            "VAEEncode",
            "SetLatentNoiseMask",
            "KSampler",
            "VAEDecode",
            "ImageCompositeMasked",
            "SaveImage",
        }
        self.assertTrue(
            {node["class_type"] for node in self.template.values()} <= allowed
        )

    def test_controlnet_can_be_disabled_without_custom_nodes(self):
        workflow = generate.prepare_workflow(
            self.template, "smile, happy", "a", 1, 0.65, 0.0, "blue hair"
        )
        self.assertFalse({"12", "13", "14"} & workflow.keys())
        self.assertEqual(workflow["7"]["inputs"]["positive"], ["4", 0])
        self.assertIn("same identity", workflow["4"]["inputs"]["text"])
        self.assertIn("mirrored", workflow["5"]["inputs"]["text"])

    def test_custom_expression_extends_eye_layers_with_stable_ascii_key(self):
        custom = generate.parse_custom_expressions(["e_1234abcd=half-lidded eyes"])
        plan = generate.generation_plan({**generate.EYE_EXPRESSIONS, **custom})
        self.assertIn(("eyes", "e_1234abcd", "half-lidded eyes", "close"), plan)
        with self.assertRaises(ValueError):
            generate.parse_custom_expressions(["日本語=invalid"])

    def test_eye_and_mouth_masks_do_not_overlap(self):
        with tempfile.TemporaryDirectory() as directory:
            eyes_path = Path(directory) / "eyes.png"
            mouth_path = Path(directory) / "mouth.png"
            generate.make_mask(eyes_path, "eyes")
            generate.make_mask(mouth_path, "mouth")
            eyes = Image.open(eyes_path).convert("L")
            mouth = Image.open(mouth_path).convert("L")
            self.assertGreater(eyes.getpixel((400, 480)), 200)
            self.assertLess(eyes.getpixel((512, 650)), 2)
            self.assertGreater(mouth.getpixel((512, 650)), 200)
            self.assertLess(mouth.getpixel((400, 480)), 2)


if __name__ == "__main__":
    unittest.main()
