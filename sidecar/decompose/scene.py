"""意味マスクを独立描画用の可視領域へ分ける。未分類領域を胴体に混ぜない。"""
import numpy as np
from scipy import ndimage


SCENE_ORDER=('residual','torso','left_arm','left_sleeve','left_hand','right_arm','right_sleeve','right_hand','neck','collar','face','hair')
PARENTS={'residual':None,'torso':None,'left_arm':'torso','right_arm':'torso',
         'neck':'torso','collar':'torso','face':'neck','hair':'face',
         'left_sleeve':'left_arm','left_hand':'left_arm','right_sleeve':'right_arm','right_hand':'right_arm'}


def visible_scene(subject, masks, face_support=None, opaque=None, optional_report=None):
    """画素の所有先を一意に決め、描画用だけ隣接原画1画素を重ねる。"""
    from optional_limbs import OPTIONAL_ROLES,partition
    base_order=tuple(name for name in SCENE_ORDER if name not in OPTIONAL_ROLES)
    regions={'torso':masks['clothes'], **{name:masks[name] for name in base_order[2:] if name!='collar'}}
    if 'collar' in masks:regions['collar']=masks['collar']
    if opaque is None:opaque=subject
    if opaque.shape!=subject.shape:raise ValueError('不透明領域の寸法が一致しません')
    if face_support is not None:
        if face_support.shape != subject.shape:raise ValueError('表情の編集領域の寸法が一致しません')
        # 顔SAMの穴にある目口も顔が所有する。未分類レイヤーに原画の目を残さない。
        regions['face']=regions['face'] | face_support
    claimed=np.zeros_like(subject);owners={}
    for name in reversed(base_order[1:]):
        if name=='collar' and name not in regions:continue
        if regions[name].shape != subject.shape:raise ValueError('部位マスクの寸法が一致しません')
        owned=subject & regions[name] & ~claimed
        # 襟は任意素材。顔・髪に完全遮蔽された候補を空素材として公開しない。
        if name=='collar' and not owned.any():continue
        if not owned.any():raise ValueError(f'独立描画の部位が空です: {name}')
        owners[name]=owned;claimed |= owned
    owners['residual']=subject & ~claimed
    owners,report=partition(owners,masks)
    if optional_report is not None:optional_report.update(report)
    # 境界の線形サンプリングで隙間を作らないため、同じ原画の隣接画素だけを共有する。
    # これは隠れた部位の生成ではない。回転で露出する広い領域は別途補完が必要。
    # 半透明の原画画素を複数部位へ重ねるとアルファが増えるため、共有は不透明画素だけ。
    textures={name:mask | (ndimage.binary_dilation(mask,iterations=1)&subject&opaque) for name,mask in owners.items() if mask.any()}
    if not np.array_equal(np.logical_or.reduce(list(owners.values())),subject):
        raise ValueError('部位の分割で原画の前景が欠落しました')
    return owners,textures
