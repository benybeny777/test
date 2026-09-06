"""生成マスクと抽出の原寸所有領域を同じ入力から求める。"""
import numpy as np
from PIL import Image
from scipy import ndimage


def white_reference(source):
    if source.dtype!=np.uint8 or source.ndim!=3 or source.shape[2]!=4:
        raise ValueError('白合成の原寸RGBAが不正です')
    result=Image.new('RGBA',(source.shape[1],source.shape[0]),'white')
    result.alpha_composite(Image.fromarray(source))
    return np.array(result)


def enclosed_hair_regions(hair,protected,visible):
    holes=ndimage.binary_fill_holes(hair)&~hair
    labels,_=ndimage.label(holes)
    return holes&~np.isin(labels,np.unique(labels[protected]))&visible


def hair_boundary_candidates(source,surface,hair,features,band):
    if source.dtype!=np.uint8 or source.ndim!=3 or source.shape[2]!=4:
        raise ValueError('原寸RGBAが必要です')
    if len(features)!=3 or any(mask.dtype!=bool or mask.shape!=source.shape[:2] for mask in [surface,hair,*features]):
        raise ValueError('髪境界の原寸マスクが不正です')
    if not isinstance(band,int) or band<1:raise ValueError('境界幅が不正です')
    distance=ndimage.distance_transform_edt(~hair)
    candidate=(distance>0)&(distance<=band)&surface&(source[:,:,3]>0)
    for feature in features:candidate &= ~ndimage.binary_dilation(feature,iterations=band)
    eyes=np.nonzero(features[0]|features[1])[0];mouth=np.nonzero(features[2])[0]
    if not eyes.size or not mouth.size:raise ValueError('髪境界に必要な目口の実測領域がありません')
    candidate[:eyes.min()]=False;candidate[mouth.min():]=False
    return candidate


def scene_support(rig,parts,source,masks):
    height,width=source.shape[:2]
    if rig['canvas']!={'width':width,'height':height}:raise ValueError('sceneと原画の原寸が不一致です')
    def owned(name):
        box=rig['layers'][name]['texture_box']
        if len(box)!=4 or any(not isinstance(v,int) for v in box):raise ValueError('sceneの原寸切出し範囲が不正です')
        l,t,r,b=box
        if not 0<=l<r<=width or not 0<=t<b<=height:raise ValueError('sceneが原画外です')
        pixels=parts[name]
        if pixels.dtype!=np.uint8 or pixels.shape!=(b-t,r-l,4):raise ValueError('scene素材が原寸ではありません')
        result=np.zeros((height,width),bool);result[t:b,l:r]=pixels[:,:,3]>0
        return result
    face=owned('scene_face');surface=face.copy()
    if 'scene_residual' in rig['layers']:surface |= owned('scene_residual')
    hair=masks['hair'].copy()
    if hair.dtype!=bool or hair.shape!=(height,width):raise ValueError('意味髪マスクが原寸ではありません')
    nodes=[node for node in rig['scene_graph'] if node['role']=='hair']
    if len(nodes)!=1:raise ValueError('単一のscene髪所有が必要です')
    hair |= owned(nodes[0]['layer'])
    return face,surface,hair


def hidden_support(source,face,hair,features,radius):
    """原寸の隠れ下地支持を、生成マスクと抽出で共有する。"""
    if not isinstance(radius,int) or radius<1:raise ValueError('隠れ補完の半径が不正です')
    if source.dtype!=np.uint8 or source.ndim!=3 or source.shape[2]!=4:
        raise ValueError('隠れ補完の原寸RGBAが必要です')
    if len(features)!=3 or any(mask.dtype!=bool or mask.shape!=source.shape[:2] for mask in [face,hair,*features]):
        raise ValueError('隠れ補完の原寸マスクが不正です')
    rows=np.nonzero(face)[0]
    if not rows.size:raise ValueError('原画の顔が空です')
    distance=ndimage.distance_transform_edt(~face)
    result=(distance>0)&(distance<=radius)&hair&(source[:,:,3]>0)
    result &= ~np.logical_or.reduce(features)
    result[rows.max()+1:]=False
    return result


def hidden_edit_masks(source,surface,hair,features,ears,band,face,radius):
    protected=np.logical_or.reduce(features)
    hair=hair|enclosed_hair_regions(hair,protected,source[:,:,3]>0)
    boundary=hair_boundary_candidates(source,surface,hair,features,band)
    if ears.dtype!=bool or ears.shape!=hair.shape:raise ValueError('原画耳が原寸ではありません')
    allowed=(source[:,:,3]>0)&~protected
    # 再分類は顔を減らすだけ。旧髪上の新支持は元支持の部分集合であり、新髪はboundary内。
    support=hidden_support(source,face&~hair,hair,features,radius)
    return {'hidden-face':np.where((support|boundary)&allowed,255,0).astype(np.uint8),
            'side-ears':np.where((hair|boundary|ears)&allowed,255,0).astype(np.uint8)}
