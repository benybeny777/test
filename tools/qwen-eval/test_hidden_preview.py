"""補完は不透明な遮蔽の下だけに限定し、原画を変更しない。"""
import unittest
import tempfile
from pathlib import Path
import numpy as np
from build_preview import hidden_material,baseline_hashes,separate_hair_pixels,ROOT


class HiddenPreviewTests(unittest.TestCase):
    def test_hair_pixels_are_not_left_in_residual_or_face(self):
        pixels=np.full((4,4,4),255,dtype=np.uint8)
        hair=np.zeros((4,4),bool);hair[:,0]=True
        for is_hair in [False,True]:
            result=separate_hair_pixels(pixels,hair,is_hair)
            expected=hair if is_hair else ~hair
            self.assertTrue(np.array_equal(result[:,:,3]>0,expected))
            self.assertTrue(np.all(result[:,:,:3]==255))
        self.assertTrue(np.all(pixels==255))

    def test_baseline_fingerprint_includes_part_images(self):
        parent=ROOT/'temp/tests';parent.mkdir(parents=True,exist_ok=True)
        with tempfile.TemporaryDirectory(dir=parent) as folder:
            root=Path(folder);(root/'rig2d/parts').mkdir(parents=True)
            (root/'rig2d/rig.json').write_text('{}')
            part=root/'rig2d/parts/face.png';part.write_bytes(b'first')
            before=baseline_hashes(root);part.write_bytes(b'changed')
            self.assertNotEqual(before,baseline_hashes(root))

    def test_hidden_only_and_source_unchanged(self):
        source=np.full((40,40,4),255,dtype=np.uint8);source[:,:,:3]=180
        generated=source.copy();generated[:,:,:3]=120
        face=np.zeros((40,40),bool);face[10:30,10:30]=True
        hair=~face;source[5,10,3]=128;original=source.copy()
        result,hidden=hidden_material(source,generated,face,hair,[],6)
        self.assertTrue(hidden.any());self.assertFalse(hidden[face].any())
        self.assertFalse(hidden[5,10]);self.assertFalse(hidden[30:].any())
        self.assertTrue(np.array_equal(source,original))
        self.assertTrue(np.all(result[hidden,:3]==180))
        self.assertTrue(np.all(result[~hidden,3]==0))


if __name__=='__main__':unittest.main()
