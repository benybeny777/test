import tempfile
import unittest
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

import import_image


def sample_image() -> Image.Image:
    image = Image.new("RGBA", (1024, 1024), (230, 220, 210, 255))
    draw = ImageDraw.Draw(image)
    draw.ellipse((260, 330, 440, 510), fill=(20, 30, 80, 255))
    draw.rectangle((570, 350, 760, 500), fill=(150, 20, 60, 255))
    draw.polygon(((390, 680), (650, 630), (580, 760)), fill=(30, 20, 20, 255))
    return image


class ImportImageTests(unittest.TestCase):
    def test_detects_alignment_mirroring_and_color_drift(self):
        neutral = sample_image()
        shifted = Image.fromarray(np.roll(np.asarray(neutral), 24, axis=1))
        self.assertIn("位置ずれ", "".join(import_image.inspect_import(neutral, shifted)["warnings"]))
        mirrored = neutral.transpose(Image.Transpose.FLIP_LEFT_RIGHT)
        self.assertIn("左右反転", "".join(import_image.inspect_import(neutral, mirrored)["warnings"]))
        tinted = Image.fromarray(np.clip(np.asarray(neutral, dtype=np.int16) + [40, 0, 0, 0], 0, 255).astype(np.uint8))
        result = import_image.inspect_import(neutral, tinted, color_tolerance=0.02)
        self.assertIn("色差", "".join(result["warnings"]))

    def test_signature_changes_with_imported_pixels(self):
        neutral = sample_image()
        changed = neutral.copy()
        changed.putpixel((512, 512), (0, 0, 0, 255))
        settings = {"alignment_tolerance": 12.0, "color_tolerance": 0.08}
        self.assertNotEqual(
            import_image.cache_signature(neutral, neutral, settings),
            import_image.cache_signature(neutral, changed, settings),
        )

    def test_atomic_export_uses_expected_layout(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "eyes" / "smile.png"
            import_image.save_atomic(sample_image(), path)
            with Image.open(path) as image:
                self.assertEqual(image.size, (1024, 1024))


if __name__ == "__main__":
    unittest.main()
