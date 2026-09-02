import json
import tempfile
import unittest
from pathlib import Path

import generate


class BackgroundWorkflowTests(unittest.TestCase):
    def test_workflow_is_native_16_by_9_and_uses_standard_nodes(self):
        repo = Path(__file__).resolve().parents[2]
        template = json.loads((repo / "workflows/background-txt2img-api.json").read_text(encoding="utf-8"))
        self.assertEqual(template["4"]["inputs"]["width"], 1344)
        self.assertEqual(template["4"]["inputs"]["height"], 756)
        allowed = {"CheckpointLoaderSimple", "CLIPTextEncode", "EmptyLatentImage", "KSampler", "VAEDecode", "SaveImage"}
        self.assertTrue({node["class_type"] for node in template.values()} <= allowed)
        workflow = generate.prepare_workflow(template, "moonlit studio", 42)
        self.assertIn("moonlit studio", workflow["2"]["inputs"]["text"])
        self.assertIn("no people", workflow["2"]["inputs"]["text"])
        self.assertEqual(workflow["5"]["inputs"]["seed"], 42)

    def test_atomic_replace_leaves_no_part_file(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, destination = root / "source.png", root / "background.png"
            source.write_bytes(b"png")
            generate.replace_atomic(source, destination)
            self.assertEqual(destination.read_bytes(), b"png")
            self.assertFalse(list(root.glob("*.part")))


if __name__ == "__main__":
    unittest.main()
