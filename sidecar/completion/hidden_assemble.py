"""通常リグと原寸入力から追加素材を返す実装。ファイル公開と推論は行わない。"""
import copy
import numpy as np
from PIL import Image
from scipy import ndimage
from hidden_materials import extract_hidden_roi, extract_ear_repair, separate_hair_pixels, validate_neutral, ear_policy
from hidden_regions import scene_support


def crop_box(layer, size):
    box = layer['texture_box']
    if len(box) != 4 or any(not isinstance(v, int) for v in box):
        raise ValueError('素材の切出し範囲は原寸整数座標である必要があります')
    l, t, r, b = box
    if not 0 <= l < r <= size[0] or not 0 <= t < b <= size[1]:
        raise ValueError('素材の切出し範囲がキャンバス外です')
    return l, t, r, b


def composite(rig, parts):
    size = (rig['canvas']['width'], rig['canvas']['height'])
    result = Image.new('RGBA', size)
    for node in rig['scene_graph']:
        # 製品レンダラーは中立変位で隠れ素材をdiscardする。
        if rig.get('hidden_motion') and node.get('role') == 'hidden_face':
            continue
        name = node['layer']; l, t, r, b = crop_box(rig['layers'][name], size)
        image = parts[name]
        if image.dtype != np.uint8 or image.shape != (b-t, r-l, 4):
            raise ValueError('scene素材の原寸RGBAと定義が一致しません: '+name)
        result.alpha_composite(Image.fromarray(image), (l, t))
    return np.array(result)


def same_visible_rgba(left, right):
    """透明画素の不可視RGBは比較せず、アルファと可視RGBを厳密に比較する。"""
    if left.shape != right.shape or left.dtype != np.uint8 or right.dtype != np.uint8:
        return False
    if not np.array_equal(left[:, :, 3], right[:, :, 3]):
        return False
    visible = left[:, :, 3] > 0
    return np.array_equal(left[:, :, :3][visible], right[:, :, :3][visible])


def put_part(rig, parts, name, full, pivot, z_index):
    image = Image.fromarray(full); box = image.getchannel('A').getbbox()
    if box is None:
        raise ValueError('公開する追加素材が空です: '+name)
    parts[name] = np.array(image.crop(box))
    rig['layers'][name] = {'url': '/assets/rig2d/parts/'+name+'.png', 'bbox': list(box),
                           'texture_box': list(box), 'pivot': copy.deepcopy(pivot), 'z_index': z_index}
    return box


def assemble_hidden(rig, parts, source, masks, bald, side, region, settings,
                    source_ears=None, generated_ears=None, require_visible_contour=True):
    """入力を変更せず、全中立合成を検査して更新リグと素材を返す。"""
    if rig.get('schema_version') != 3 or rig.get('scene_graph_version') != 1:
        raise ValueError('現行の通常リグが必要です')
    if any(node.get('role') == 'hidden_face' for node in rig['scene_graph']):
        raise ValueError('補完済みリグを補完前リグの代わりに使えません')
    if sum(node.get('role') == 'hair' for node in rig['scene_graph']) != 1:
        raise ValueError('現行の単一scene髪所有領域が必要です')
    height, width = source.shape[:2]; size = (width, height)
    if source.dtype != np.uint8 or source.shape != (height, width, 4) or rig['canvas'] != {'width': width, 'height': height}:
        raise ValueError('原画とリグの原寸キャンバスが一致しません')
    if len(region) != 4 or any(not isinstance(v, int) for v in region):
        raise ValueError('原寸ROIは整数である必要があります')
    l, t, r, b = region
    if not 0 <= l < r <= width or not 0 <= t < b <= height:
        raise ValueError('原寸ROIが原画の範囲外です')
    if bald.shape != (b-t, r-l, 4) or side.shape != bald.shape:
        raise ValueError('編集2枚の原寸ROIが一致しません')
    for name in ('hair', 'left_eye', 'right_eye', 'mouth'):
        if masks[name].dtype != bool or masks[name].shape != source.shape[:2]:
            raise ValueError('原寸解析マスクが不正です: '+name)
    for name, pixels in parts.items():
        box = crop_box(rig['layers'][name], size)
        if pixels.dtype != np.uint8 or pixels.shape != (box[3]-box[1], box[2]-box[0], 4):
            raise ValueError('素材の原寸が不正です: '+name)
    before = composite(rig, parts)
    if not same_visible_rgba(before, source):
        raise ValueError('補完前の全中立合成が原画と一致しません')
    updated = copy.deepcopy(rig)
    output = {name: image.copy() for name, image in parts.items()}
    face,surface,hair = scene_support(rig,parts,source,masks)
    fl, ft, fr, fb = crop_box(rig['layers']['scene_face'], size)
    features = [masks[name][t:b, l:r] for name in ('left_eye', 'right_eye', 'mouth')]
    face_box = rig['layers']['face']['bbox']; face_width = face_box[2]-face_box[0]
    bundle = extract_hidden_roi(source[t:b, l:r], bald, side, face[t:b, l:r], hair[t:b, l:r],
                                features, face_width, settings, surface[t:b, l:r])
    hair[t:b, l:r] = bundle['refined_hair']
    full_hidden = np.zeros_like(source); full_hidden[t:b, l:r] = bundle['hidden_rgba']
    policy = ear_policy(source_ears, generated_ears, require_visible_contour)
    redraw = policy['mode'] == 'redraw-measured-ears'
    full_repair = np.zeros_like(source); old_ears = np.zeros((height, width), bool)
    if redraw:
        repair = extract_ear_repair(source[t:b, l:r], side, bundle['face_mask'] | bundle['hidden_mask'],
                                    bundle['refined_hair'], features, source_ears, generated_ears, bundle['radius_px'])
        full_repair[t:b, l:r] = repair['rgba']
        old_ears[t:b, l:r] = repair['remove_original_ear']
        # 可視耳の外形を広げる画素だけを髪から移す。下地の補修範囲全体は露出させない。
        # 原画で見えた耳の輪郭帯だけを直す。生成耳が大きくても横髪全体を剥がさない。
        hair[t:b, l:r][repair['revealed_hair'] & repair['remove_original_ear']] = False
        ear_distance = ndimage.distance_transform_edt(~source_ears)
        full_repair[t:b, l:r][ear_distance > bundle['radius_px'], 3] = 0
        # 大きな生成耳は元の髪の下へ保持する。中立で見える耳を大きくはしない。
        hidden_ears = generated_ears & hair[t:b, l:r] & (source[t:b, l:r, 3] > 0)
        for feature in features:
            hidden_ears &= ~feature
        full_hidden[t:b, l:r][hidden_ears] = side[hidden_ears]
        full_hidden[t:b, l:r, 3][hidden_ears] = source[t:b, l:r, 3][hidden_ears]
        bundle['hidden_mask'] |= hidden_ears
        # 原画alpha254等の髪の下へ可視顔を重ねない。隠れ下地は独立素材で露出時だけ描く。
        full_repair[hair, 3] = 0
    for node in rig['scene_graph']:
        name = node['layer']; sl, st, sr, sb = crop_box(rig['layers'][name], size)
        owned = hair[st:sb, sl:sr]; pixels = parts[name].copy()
        if node['role'] == 'hair':
            pixels[owned] = source[st:sb, sl:sr][owned]
        if redraw:
            pixels[full_repair[st:sb, sl:sr, 3] > 0, 3] = 0
        output[name] = separate_hair_pixels(pixels, owned, node['role'] == 'hair')
        if node['role'] == 'hair':
            # 再所有領域が旧切出し枠から出ても失わない。変形の基準bboxは保持する。
            full_hair = separate_hair_pixels(source, hair, True)
            hair_image = Image.fromarray(full_hair)
            hair_box = hair_image.getchannel('A').getbbox()
            if hair_box is None:
                raise ValueError('再所有後の髪素材が空です')
            output[name] = np.array(hair_image.crop(hair_box))
            updated['layers'][name]['texture_box'] = list(hair_box)
    if redraw:
        completed_pixels = np.zeros_like(source)
        completed_pixels[ft:fb, fl:fr] = output['scene_face']
        active = full_repair[:, :, 3] > 0
        completed_pixels[active] = full_repair[active]
        completed = Image.fromarray(completed_pixels)
        box = completed.getchannel('A').getbbox()
        if box is None:
            raise ValueError('耳修復後の顔素材が空です')
        output['scene_face'] = np.array(completed.crop(box))
        updated['layers']['scene_face']['texture_box'] = list(box)
        excluded = hair | old_ears | (full_repair[:, :, 3] > 0)
        for name in ('left_eye_base', 'right_eye_base', 'left_eye_backplate', 'right_eye_backplate'):
            sl, st, sr, sb = crop_box(rig['layers'][name], size)
            output[name] = separate_hair_pixels(parts[name], excluded[st:sb, sl:sr], False)
    pivot = rig['layers']['face']['pivot']; z_index = rig['layers']['scene_face']['z_index']
    bounds = put_part(updated, output, 'scene_hidden_face', full_hidden, pivot, z_index)
    face_index = next(i for i, node in enumerate(updated['scene_graph']) if node['role'] == 'face')
    updated['scene_graph'].insert(face_index, {'layer': 'scene_hidden_face', 'role': 'hidden_face', 'parent': 'face',
        'source_region': list(bounds), 'hidden_regions': 'partial', 'status': 'unverified',
        'owned_pixels': int(bundle['hidden_mask'].sum())})
    support = np.zeros((height, width), bool)
    support[t:b, l:r] = bundle['face_mask'] | bundle['hidden_mask']
    clearance = ndimage.distance_transform_edt(support); radius = bundle['radius_px']; cols, rows = 64, 96
    weights = [round(min(1, float(clearance[min(height-1, round(y*height/rows)), min(width-1, round(x*width/cols))])/radius), 6)
               for y in range(rows+1) for x in range(cols+1)]
    if not any(weights):
        raise ValueError('補完の支持領域が変位格子に届きません')
    updated['hidden_motion'] = {'version': 1, 'weights': weights, 'angle_limit': 15,
                                 'x_limit_px': radius*settings.motion_ratio,
                                 'y_limit_px': radius*settings.motion_ratio*.5}
    after = composite(updated, output)
    allowed = full_repair[:, :, 3] > 0
    changed = validate_neutral(before, after, allowed, [masks[name] for name in ('left_eye', 'right_eye', 'mouth')])
    for name in ('mouth_closed', 'mouth_open', 'left_eyelid_upper', 'right_eyelid_upper'):
        if name in parts and not np.array_equal(parts[name], output[name]):
            raise ValueError('耳補修が目口の独立素材を変更しました: '+name)
    report = {**policy, 'neutral_changed_pixels': changed, 'hidden_pixels': int(bundle['hidden_mask'].sum()),
              'reassigned_hair_pixels': bundle['reassigned_hair_pixels'], 'raw_source_preserved': True,
              'source_alpha_preserved': True, 'neutral_hidden_coverage': 0,
              'quality': 'unverified'}
    updated['local_hidden_completion'] = report
    return updated, output, report
