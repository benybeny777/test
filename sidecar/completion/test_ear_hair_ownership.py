"""耳補修の肌下地によって前髪を消さないことを組立経路で検査する。"""
import unittest
import numpy as np
from scipy import ndimage
from hidden_assemble import assemble_hidden,composite
from hidden_materials import HiddenSettings


class EarHairOwnershipTests(unittest.TestCase):
    def test_repair_under_hair_keeps_neutral_hair_for_254_and_255(self):
        for alpha in (254,255):
            source=np.full((80,80,4),180,np.uint8);source[:,:,3]=alpha
            face=np.zeros((80,80),bool);face[20:60,20:60]=True;hair=~face
            features=[np.zeros_like(face) for _ in range(3)]
            features[0][30:34,29:33]=True;features[1][30:34,45:49]=True;features[2][48:51,36:42]=True
            masks=dict(zip(('left_eye','right_eye','mouth'),features));masks['hair']=hair
            ear=np.zeros_like(face);ear[37:43,18:23]=True
            generated_ear=np.zeros_like(face);generated_ear[35:46,12:23]=True
            generated=source.copy();generated[:,:,:3]=120
            parts={}
            for name,mask in [('scene_face',face),('scene_hair',hair),('face',face)]:
                pixels=source.copy();pixels[~mask]=0;parts[name]=pixels
            for name in ('left_eye_base','right_eye_base','left_eye_backplate','right_eye_backplate'):
                parts[name]=parts['scene_face'].copy()
            rig={'schema_version':3,'scene_graph_version':1,'canvas':{'width':80,'height':80},
                'layers':{name:{'texture_box':[0,0,80,80],'bbox':[20,20,60,60] if name=='face' else [0,0,80,80],
                                'pivot':[.5,.5],'z_index':index} for index,name in enumerate(parts)},
                'scene_graph':[{'layer':'scene_face','role':'face'},{'layer':'scene_hair','role':'hair'}]}
            updated,pixels,_=assemble_hidden(rig,parts,source,masks,generated,generated,[0,0,80,80],
                HiddenSettings(.08,.35,.015,40.),ear,generated_ear)
            after=composite(updated,pixels)
            preserved=hair&~ndimage.binary_dilation(ear,iterations=1)
            np.testing.assert_array_equal(after[preserved],source[preserved])
            np.testing.assert_array_equal(after[:,:,3],source[:,:,3])
            for feature in features:np.testing.assert_array_equal(after[feature],source[feature])
            self.assertTrue(np.any(after[ear]!=source[ear]))
            box=updated['layers']['scene_hidden_face']['texture_box'];l,t,r,b=box
            hidden=np.zeros_like(hair);hidden[t:b,l:r]=pixels['scene_hidden_face'][:,:,3]>0
            self.assertTrue(np.all(hidden[generated_ear&preserved]))

    def test_mirror_translation_preserve_adoption_and_native_generated_ear(self):
        for alpha in (254,255):
            source=np.full((80,80,4),180,np.uint8);source[:,:,3]=alpha
            face=np.zeros((80,80),bool);face[20:60,20:60]=True;hair=~face
            features=[np.zeros_like(face) for _ in range(3)]
            features[0][30:34,29:33]=True;features[1][30:34,45:49]=True;features[2][48:51,36:42]=True
            ear=np.zeros_like(face);ear[37:43,18:23]=True
            generated_ear=np.zeros_like(face);generated_ear[35:46,12:23]=True
            generated=source.copy();yy,xx=np.indices(face.shape)
            # 非一様な生成画素で、耳の再サンプリングや位置変更を検出する。
            generated[:,:,0]=110+xx%11;generated[:,:,1]=120+yy%13;generated[:,:,2]=130+(xx+yy)%17
            originals=[value.copy() for value in (source,generated,face,hair,ear,generated_ear,*features)]

            def run(mirror,padding):
                def transform(value):
                    value=np.fliplr(value) if mirror else value
                    widths=list(padding)+([(0,0)] if value.ndim==3 else [])
                    return np.pad(value,widths)
                picture,edit,skin,strands,old,new,*eyes=[transform(value) for value in originals]
                if mirror:eyes[0],eyes[1]=eyes[1],eyes[0]
                height,width=skin.shape;masks=dict(zip(('left_eye','right_eye','mouth'),eyes));masks['hair']=strands
                ys,xs=np.nonzero(skin);bbox=[int(xs.min()),int(ys.min()),int(xs.max()+1),int(ys.max()+1)]
                parts={}
                for name,mask in [('scene_face',skin),('scene_hair',strands),('face',skin)]:
                    pixels=picture.copy();pixels[~mask]=0;parts[name]=pixels
                for name in ('left_eye_base','right_eye_base','left_eye_backplate','right_eye_backplate'):
                    parts[name]=parts['scene_face'].copy()
                rig={'schema_version':3,'scene_graph_version':1,'canvas':{'width':width,'height':height},
                     'layers':{name:{'texture_box':[0,0,width,height],'bbox':bbox if name=='face' else [0,0,width,height],
                                     'pivot':[.5,.5],'z_index':index} for index,name in enumerate(parts)},
                     'scene_graph':[{'layer':'scene_face','role':'face'},{'layer':'scene_hair','role':'hair'}]}
                updated,pixels,_=assemble_hidden(rig,parts,picture,masks,edit,edit,[0,0,width,height],
                    HiddenSettings(.08,.35,.015,40.),old,new)
                result=composite(updated,pixels);hidden=np.zeros_like(picture)
                l,t,r,b=updated['layers']['scene_hidden_face']['texture_box'];hidden[t:b,l:r]=pixels['scene_hidden_face']
                preserved=strands&~ndimage.binary_dilation(old,iterations=1)
                np.testing.assert_array_equal(result[preserved],picture[preserved])
                np.testing.assert_array_equal(result[:,:,3],picture[:,:,3])
                for eye in eyes:np.testing.assert_array_equal(result[eye],picture[eye])
                distance=ndimage.distance_transform_edt(~old)
                np.testing.assert_array_equal(result[distance>3],picture[distance>3])
                np.testing.assert_array_equal(hidden[new&preserved],edit[new&preserved])
                return result,np.any(result!=picture,axis=2),transform

            baseline,baseline_changed,_=run(False,((0,0),(0,0)))
            for mirror,padding in [(True,((0,0),(0,0))),(False,((7,11),(5,13))),(True,((7,11),(5,13)))]:
                result,changed,transform=run(mirror,padding)
                np.testing.assert_array_equal(result,transform(baseline))
                np.testing.assert_array_equal(changed,transform(baseline_changed))
            for value,saved in zip((source,generated,face,hair,ear,generated_ear,*features),originals):
                np.testing.assert_array_equal(value,saved)


if __name__=='__main__':unittest.main()
