import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("generate.py")
SPEC = importlib.util.spec_from_file_location("rig2d_generate", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class RigCreationTests(unittest.TestCase):
    def test_writes_draw_order(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest = root / "manifest.json"
            parts_dir = root / "parts"
            parts_dir.mkdir()
            parts = [
                {
                    "name": name,
                    "z_index": index,
                    "pivot": [0.5, 0.5],
                    "path": f"parts/{name}.png",
                }
                for index, name in enumerate(sorted(MODULE.REQUIRED_PARTS))
            ]
            for part in parts:
                (root / part["path"]).write_bytes(b"png")
            manifest.write_text(
                json.dumps({"canvas": {"width": 80, "height": 120}, "parts": parts}),
                encoding="utf-8",
            )
            output = MODULE.create_rig(manifest, root / "output" / "rig.json")
            rig = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(rig["profile"], "lvs-anime25d-v1")
            self.assertEqual(set(rig["draw_order"]), MODULE.REQUIRED_PARTS)
            self.assertFalse(Path(rig["layers_manifest"]).is_absolute())
            self.assertTrue((root / "output" / "parts" / "mouth_open.png").is_file())

    def test_rejects_missing_parts(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest = root / "manifest.json"
            manifest.write_text(
                json.dumps({"canvas": {"width": 1, "height": 1}, "parts": []}),
                encoding="utf-8",
            )
            with self.assertRaisesRegex(ValueError, "必須部位"):
                MODULE.create_rig(manifest, root / "rig.json")


if __name__ == "__main__":
    unittest.main()
