"""意味マスクを独立描画用の可視領域へ分ける。未分類領域を胴体に混ぜない。"""
import numpy as np
from scipy import ndimage


SCENE_ORDER=('residual','torso','left_arm','right_arm','neck','face','hair')
PARENTS={'residual':None,'torso':None,'left_arm':'torso','right_arm':'torso',
         'neck':'torso','face':'neck','hair':'face'}


def visible_scene(subject, masks, face_support=None, opaque=None):
    """画素の所有先を一意に決め、描画用だけ隣接原画1画素を重ねる。"""
    regions={'torso':masks['clothes'], **{name:masks[name] for name in SCENE_ORDER[2:]}}
    if opaque is None:opaque=subject
    if opaque.shape!=subject.shape:raise ValueError('不透明領域の寸法が一致しません')
    if face_support is not None:
        if face_support.shape != subject.shape:raise ValueError('表情の編集領域の寸法が一致しません')
        # 顔SAMの穴にある目口も顔が所有する。未分類レイヤーに原画の目を残さない。
        regions['face']=regions['face'] | face_support
    claimed=np.zeros_like(subject);owners={}
    for name in reversed(SCENE_ORDER[1:]):
        if regions[name].shape != subject.shape:raise ValueError('部位マスクの寸法が一致しません')
        owned=subject & regions[name] & ~claimed
        if not owned.any():raise ValueError(f'独立描画の部位が空です: {name}')
        owners[name]=owned;claimed |= owned
    owners['residual']=subject & ~claimed
    # 境界の線形サンプリングで隙間を作らないため、同じ原画の隣接画素だけを共有する。
    # これは隠れた部位の生成ではない。回転で露出する広い領域は別途補完が必要。
    # 半透明の原画画素を複数部位へ重ねるとアルファが増えるため、共有は不透明画素だけ。
    textures={name:mask | (ndimage.binary_dilation(mask,iterations=1)&subject&opaque) for name,mask in owners.items() if mask.any()}
    if not np.array_equal(np.logical_or.reduce(list(owners.values())),subject):
        raise ValueError('部位の分割で原画の前景が欠落しました')
    return owners,textures
