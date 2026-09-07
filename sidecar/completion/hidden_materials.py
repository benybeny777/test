"""比較ツールへ依存せず、原寸配列だけから隠れ素材を作る。"""
from dataclasses import dataclass
import numpy as np
from scipy import ndimage
from hidden_regions import enclosed_hair_regions, hair_boundary_candidates, white_reference, hidden_support

def separate_hair_pixels(pixels, hair, is_hair):
    """独立移動する髪との重複を、顔以外の未分類素材からも除く。"""
    if pixels.shape[:2]!=hair.shape:raise ValueError('髪の所有領域と素材寸法が一致しません')
    result=pixels.copy()
    result[~hair if is_hair else hair,3]=0
    return result


def refine_side_hair(source, edited, face, hair, features, band, gain):
    """横髪除去で明るくなった連続境界だけを髪へ戻し、目口を保護する。"""
    candidate=hair_boundary_candidates(source,face,hair,features,band)
    # 生成へ渡した白合成入力と比較し、半透明の白混合を編集差と誤認しない。
    reference=white_reference(source)
    candidate &= (edited[:,:,:3].astype(float)-reference[:,:,:3]).mean(axis=2)>gain
    return ndimage.binary_propagation(hair,mask=hair|candidate)


def hidden_material(source, generated, face, hair, feature_masks, radius):
    """原画の髪の下へ補完する。中立では描かず、露出時だけ原画アルファで描く。"""
    if source.shape != generated.shape or face.shape != source.shape[:2]:
        raise ValueError('比較素材の寸法が一致しません')
    # 輪郭の黒線・髪色を肌色の基準へ混ぜると、補完帯に灰色の筋が生じる。
    skin=ndimage.binary_erosion(face,iterations=max(1,round(radius/6)))
    for feature in feature_masks:skin &= ~ndimage.binary_dilation(feature,iterations=3)
    skin &= source[:,:,3]>0
    if skin.sum()<16:raise ValueError('色合わせ用の可視肌が不足しています')
    hidden=hidden_support(source,face,hair,feature_masks,radius)
    if not hidden.any():raise ValueError('髪の下に補完領域がありません')
    sigma=max(1,radius)
    denominator=ndimage.gaussian_filter(skin.astype(float),sigma)
    correction=np.zeros((*face.shape,3),dtype=float)
    for channel in range(3):
        delta=(source[:,:,channel].astype(float)-generated[:,:,channel])*skin
        correction[:,:,channel]=ndimage.gaussian_filter(delta,sigma)/np.maximum(denominator,1e-8)
    # 正規化畳み込みで連続した色差を延ばし、最近傍領域の境界を作らない。
    corrected=np.rint(np.clip(generated[:,:,:3].astype(float)+correction,0,255)).astype(np.uint8)
    result=np.zeros_like(source);result[:,:,:3]=corrected;result[:,:,3]=np.where(hidden,source[:,:,3],0)
    return result,hidden


@dataclass(frozen=True)
class HiddenSettings:
    band_ratio: float
    motion_ratio: float
    edge_band_ratio: float
    edge_gain: float


def ear_policy(source_ears, generated_ears, required_contour=False):
    """未検出を『耳が見えていない』と断定せず、可視輪郭を勝手に変えない。"""
    if generated_ears is None or not generated_ears.any():
        if required_contour:
            raise ValueError('生成耳を測定できず、要求された可視輪郭修正を完了できません')
        return {'mode': 'hidden-only', 'status': 'partial',
                'warning': '生成耳を測定できません。可視輪郭は保持し、隠れ下地だけを作成します'}
    if source_ears is None or not source_ears.any():
        if required_contour:
            raise ValueError('原画耳を測定できず、古い輪郭を安全に除去できません')
        return {'mode': 'hidden-only', 'status': 'partial',
                'warning': '原画耳が未検出です。完全遮蔽とは断定せず、可視輪郭を保持します'}
    return {'mode': 'redraw-measured-ears', 'status': 'unverified', 'warning': None}


def extract_hidden_roi(source, bald, side, face, hair, features, face_width, settings, refinement_surface=None):
    """配列を変更せず、同一原寸ROIの2編集から下地と髪所有領域を求める。"""
    if source.dtype != np.uint8 or source.ndim != 3 or source.shape[2] != 4:
        raise ValueError('原画は原寸RGBAである必要があります')
    if bald.shape != source.shape or side.shape != source.shape:
        raise ValueError('2編集と原画の原寸ROIが一致しません')
    if len(features) != 3 or any(mask.dtype != bool or mask.shape != source.shape[:2] for mask in [face, hair, *features]):
        raise ValueError('原寸の顔・髪・左右目・口マスクが必要です')
    if not 0 < settings.band_ratio <= .15 or not 0 < settings.motion_ratio <= .4:
        raise ValueError('補完帯または変位範囲が不正です')
    if not 0 < settings.edge_band_ratio <= .05 or not 0 < settings.edge_gain <= 255:
        raise ValueError('境界再分類設定が不正です')
    if face_width <= 0 or not np.isfinite(face_width):
        raise ValueError('顔幅は実測値である必要があります')
    protected = np.logical_or.reduce(features)
    refined = hair | enclosed_hair_regions(hair, protected, source[:, :, 3] > 0)
    surface = face if refinement_surface is None else refinement_surface
    if surface.dtype != bool or surface.shape != face.shape:
        raise ValueError('髪境界の再分類領域が不正です')
    refined = refine_side_hair(source, side, surface, refined, features,
                              max(1, round(face_width * settings.edge_band_ratio)), settings.edge_gain)
    visible_face = face & ~refined
    eye_rows = np.nonzero(features[0] | features[1])[0]
    if not eye_rows.size:
        raise ValueError('左右目の位置を測定できません')
    blend = np.clip((np.arange(source.shape[0]) - eye_rows.min()) /
                    max(1, int(eye_rows.max() - eye_rows.min())), 0, 1)
    blend = (blend * blend * (3 - 2 * blend))[:, None, None]
    generated = np.rint(bald * (1 - blend) + side * blend).astype(np.uint8)
    radius = max(1, round(face_width * settings.band_ratio))
    pixels, hidden = hidden_material(source, generated, visible_face, refined, features, radius)
    if np.any((pixels[:, :, 3] > 0) & (~refined | (source[:, :, 3] == 0))):
        raise ValueError('隠れ素材が原画の髪の外へ出ました')
    if not np.array_equal(pixels[:, :, 3][hidden], source[:, :, 3][hidden]):
        raise ValueError('隠れ素材の原画アルファを変更しました')
    if np.any((pixels[:, :, 3] > 0) & protected):
        raise ValueError('隠れ素材が目口へ侵入しました')
    return {'hidden_rgba': pixels, 'hidden_mask': hidden, 'refined_hair': refined,
            'face_mask': visible_face, 'radius_px': radius,
            'status': 'unverified', 'reassigned_hair_pixels': int((refined & ~hair).sum())}


def extract_ear_repair(source, side, face, hair, features, source_ears, generated_ears, radius):
    """可視耳の描き直しは両入力の原寸耳マスクが揃う場合だけ行う。"""
    policy = ear_policy(source_ears, generated_ears, required_contour=True)
    if any(mask.dtype != bool or mask.shape != face.shape for mask in (source_ears, generated_ears)):
        raise ValueError('原画耳・生成耳マスクの原寸が不一致です')
    old = ndimage.binary_dilation(source_ears, iterations=1)
    extension = ndimage.binary_dilation(generated_ears, iterations=1) & hair
    distance = ndimage.distance_transform_edt(~hair)
    alpha = np.clip((radius - distance) / (radius * .5), 0, 1) * face * (source[:, :, 3] > 0)
    eyes = np.nonzero(features[0] | features[1])[0]
    mouth = np.nonzero(features[2])[0]
    if not eyes.size or not mouth.size:
        raise ValueError('耳の補修を制限する目口の実測領域がありません')
    alpha[:int(eyes.min())] = 0
    alpha[mouth.min():] = 0
    alpha[(extension | generated_ears | old) & (source[:, :, 3] > 0)] = 1
    for feature in features:
        alpha[ndimage.binary_dilation(feature, iterations=3)] = 0
    # source-overでは254が255へ変わる。許可画素を顔の単独所有にしRGBだけ混ぜる。
    repair = source.copy()
    repair[:, :, :3] = np.rint(source[:, :, :3]*(1-alpha[:, :, None]) + side[:, :, :3]*alpha[:, :, None]).astype(np.uint8)
    repair[:, :, 3] = np.where(alpha > 0, source[:, :, 3], 0)
    if not repair[:, :, 3].any():
        raise ValueError('耳修復の許可領域が空です')
    return {'rgba': repair, 'allowed_visible_change': alpha > 0,
            'remove_original_ear': old, 'revealed_hair': extension & (alpha > 0), **policy}


def validate_neutral(before, after, allowed, features):
    """出版側が全scene素材を合成し、許可範囲外と目口が不変か検査する。"""
    if before.shape != after.shape or allowed.shape != before.shape[:2]:
        raise ValueError('中立合成の原寸が一致しません')
    changed = np.any(before != after, axis=2)
    if not np.array_equal(before[:, :, 3], after[:, :, 3]):
        raise ValueError('中立合成で原画アルファを変更しました')
    if np.any(changed & ~allowed) or np.any(changed & np.logical_or.reduce(features)):
        raise ValueError('耳修復の許可範囲外または目口を変更しました')
    return int(changed.sum())
