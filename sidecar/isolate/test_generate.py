import importlib.util
import sys
import unittest
from pathlib import Path

import numpy as np
from PIL import Image


MODULE_PATH = Path(__file__).with_name("generate.py")
SPEC = importlib.util.spec_from_file_location("isolate_generate", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class IsolateInputTests(unittest.TestCase):
    def test_preserves_aspect_ratio_and_centers_padding(self):
        image = np.full((200, 100, 3), 255, dtype=np.uint8)
        tensor, placement = MODULE.prepare_isnet_input(Image.fromarray(image))
        self.assertEqual(tensor.shape, (1, 3, 1024, 1024))
        self.assertEqual(placement, (0, 256, 1024, 512))
        self.assertEqual(float(tensor[:, :, :, :256].max()), 0.0)
        self.assertEqual(float(tensor[:, :, :, 256:768].min()), 1.0)


if __name__ == "__main__":
    unittest.main()
