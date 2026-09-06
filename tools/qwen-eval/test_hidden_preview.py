"""補完は不透明な遮蔽の下だけに限定し、原画を変更しない。"""
import unittest
import tempfile
from pathlib import Path
import numpy as np
from build_preview import hidden_material,baseline_hashes,separate_hair_pixels,refine_side_hair,enclosed_hair_regions,ROOT


class HiddenPreviewTests(unittest.TestCase):
    def test_hidden_skin_matching_excludes_dark_face_outline(self):
        source=np.full((40,40,4),180,dtype=np.uint8);source[:,:,3]=255
        generated=source.copy();generated[:,:,:3]=120
        face=np.zeros((40,40),bool);face[10:30,10:30]=True
        source[10:30,10,:3]=20;source[10:30,29,:3]=20
        source[10,10:30,:3]=20;source[29,10:30,:3]=20
        result,hidden=hidden_material(source,generated,face,~face,[],6)
        self.assertTrue(np.all(result[hidden,:3]==180))

    def test_enclosed_accessory_keeps_whole_shape_and_protects_face_holes(self):
        hair=np.ones((20,20),bool);hair[2:6,2:6]=False;hair[8:18,8:18]=False
        protected=np.zeros_like(hair);protected[10,10]=True
        opaque=np.ones_like(hair);opaque[3,3]=False
        result=enclosed_hair_regions(hair,protected,opaque)
        self.assertEqual(int(result.sum()),15)
        self.assertFalse(result[8:18,8:18].any());self.assertFalse(result[3,3])

    def test_blink_backplate_cannot_restore_replaced_ear(self):
        pixels=np.full((4,4,4),200,dtype=np.uint8)
        hair=np.zeros((4,4),bool);hair[:,0]=True
        ear=np.zeros_like(hair);ear[2:,1]=True
        result=separate_hair_pixels(pixels,hair|ear,False)
        self.assertTrue(np.all(result[hair|ear,3]==0))
        self.assertTrue(np.array_equal(result[~(hair|ear)],pixels[~(hair|ear)]))
        self.assertTrue(np.all(pixels==200))

    def test_refinement_protects_features_and_lower_face(self):
        source=np.full((40,40,4),180,dtype=np.uint8);source[:,:,3]=255
        edited=source.copy();hair=np.zeros((40,40),bool);hair[:,:10]=True
        features=[np.zeros((40,40),bool) for _ in range(3)]
        features[0][8:13,10:15]=True;features[1][8:13,25:30]=True;features[2][28:31,18:22]=True
        source[18,10:12,:3]=20;source[10,10,:3]=20;source[31,10,:3]=20
        original=source.copy();result=refine_side_hair(source,edited,~hair,hair,features,3,40)
        self.assertTrue(result[18,10:12].all());self.assertFalse(result[10,10]);self.assertFalse(result[31,10])
        self.assertTrue(np.array_equal(source,original));self.assertEqual(int((result&~hair).sum()),2)

    def test_boundary_refinement_accepts_residual_without_changing_neutral_colors(self):
        source=np.full((40,40,4),180,dtype=np.uint8);source[:,:,3]=255
        edited=source.copy();hair=np.zeros((40,40),bool);hair[:,:10]=True
        face=np.zeros_like(hair);residual=np.zeros_like(hair);residual[18:21,10:12]=True
        features=[np.zeros_like(hair) for _ in range(3)]
        features[0][8:12,15:19]=True;features[1][8:12,25:29]=True;features[2][28:31,18:22]=True
        source[residual,:3]=20
        refined=refine_side_hair(source,edited,face|residual,hair,features,3,40)
        self.assertTrue(refined[residual].all())
        transferred=separate_hair_pixels(source,refined,True)
        self.assertTrue(np.array_equal(transferred[residual],source[residual]))

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
