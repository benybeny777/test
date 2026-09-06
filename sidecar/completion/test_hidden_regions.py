"""原寸の限定境界だけを編集し、白合成を編集差と誤認しないことを検査する。"""
import unittest
import numpy as np
from hidden_regions import hidden_edit_masks,hair_boundary_candidates,white_reference,scene_support
from hidden_materials import refine_side_hair

class BoundaryTests(unittest.TestCase):
    def fixture(self):
        source=np.full((64,64,4),60,np.uint8);source[:,:,3]=80
        hair=np.zeros((64,64),bool);hair[:,10:20]=True
        face=np.zeros_like(hair);face[:,20:50]=True
        features=[np.zeros_like(hair) for _ in range(3)]
        features[0][20:23,26:29]=True;features[1][20:23,42:45]=True;features[2][43:46,30:35]=True
        return source,face,hair,features
    def test_candidate_is_editable_without_expanding_to_features_or_transparency(self):
        source,face,hair,features=self.fixture();source[30,21,3]=0;before=source.copy()
        candidate=hair_boundary_candidates(source,face,hair,features,3)
        self.assertTrue(candidate.any());self.assertFalse((candidate&hair).any())
        masks=hidden_edit_masks(source,face,hair,features,np.zeros_like(hair),3)
        for mask in masks.values():
            self.assertTrue(np.all(mask[candidate]==255))
            self.assertTrue(np.all(mask[np.logical_or.reduce(features)]==0))
            self.assertTrue(np.all(mask[source[:,:,3]==0]==0))
            self.assertFalse(((mask>0)&~(hair|candidate)).any())
        self.assertTrue(np.array_equal(source,before))
    def test_no_edit_on_partial_alpha_never_becomes_reclassified_hair(self):
        source,face,hair,features=self.fixture();edited=white_reference(source)
        self.assertTrue(np.any(edited[:,:,:3].astype(float)-source[:,:,:3]>40))
        result=refine_side_hair(source,edited,face,hair,features,3,40)
        self.assertTrue(np.array_equal(result,hair))
    def test_native_geometry_is_translation_invariant(self):
        source,face,hair,features=self.fixture();ear=np.zeros_like(hair)
        original=hidden_edit_masks(source,face,hair,features,ear,3)
        pad=((9,11),(7,13));shifted=hidden_edit_masks(np.pad(source,(*pad,(0,0))),np.pad(face,pad),np.pad(hair,pad),
                   [np.pad(mask,pad) for mask in features],np.pad(ear,pad),3)
        for key in original:self.assertTrue(np.array_equal(original[key],shifted[key][9:73,7:71]))
    def test_scene_hair_boundary_is_shared_not_only_semantic_hair(self):
        source,face,hair,features=self.fixture();scenehair=hair.copy();scenehair[30,20]=True
        parts={}
        for name,mask in [('scene_face',face),('scene_hair',scenehair)]:
            pixels=source.copy();pixels[~mask,3]=0;parts[name]=pixels
        rig={'canvas':{'width':64,'height':64},'layers':{name:{'texture_box':[0,0,64,64]} for name in parts},
             'scene_graph':[{'role':'hair','layer':'scene_hair'}]}
        _,surface,owned=scene_support(rig,parts,source,{'hair':hair})
        self.assertTrue(owned[30,20]);self.assertTrue(np.array_equal(surface,face))
        parts['scene_hair']=parts['scene_hair'][:-1]
        with self.assertRaises(ValueError):scene_support(rig,parts,source,{'hair':hair})

if __name__=='__main__':unittest.main()
