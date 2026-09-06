"""原画を保持し、補完下地と承認済み耳輪郭を比較専用リグへ追加する。"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import shutil
import sys

import numpy as np
from PIL import Image
from scipy import ndimage

from run import ROOT, verify_source
from download import digest
sys.path.insert(0, str(ROOT/'sidecar'))
sys.path.insert(0, str(ROOT/'sidecar/rig2d'))
from output_transaction import directory_output
from texture import bleed_transparent_rgb


def baseline_hashes(character):
    directory=character/'rig2d'
    return {path.relative_to(directory).as_posix():digest(path)
            for path in [directory/'rig.json',*sorted((directory/'parts').glob('*.png'))]}


def separate_hair_pixels(pixels, hair, is_hair):
    """独立移動する髪との重複を、顔以外の未分類素材からも除く。"""
    if pixels.shape[:2]!=hair.shape:raise ValueError('髪の所有領域と素材寸法が一致しません')
    result=pixels.copy()
    result[~hair if is_hair else hair,3]=0
    return result


def refine_side_hair(source, edited, face, hair, features, band, gain):
    """横髪除去で明るくなった連続境界だけを髪へ戻し、目口を保護する。"""
    distance=ndimage.distance_transform_edt(~hair)
    candidate=(distance>0)&(distance<=band)&face&(source[:,:,3]==255)
    candidate &= (edited[:,:,:3].astype(float)-source[:,:,:3]).mean(axis=2)>gain
    for feature in features:candidate &= ~ndimage.binary_dilation(feature,iterations=band)
    eye_rows=np.nonzero(features[0]|features[1])[0]
    mouth_rows=np.nonzero(features[2])[0]
    if not eye_rows.size or not mouth_rows.size:raise ValueError('横髪境界の保護に必要な目口の領域がありません')
    candidate[:eye_rows.min()]=False;candidate[mouth_rows.min():]=False
    return ndimage.binary_propagation(hair,mask=hair|candidate)


def hidden_material(source, generated, face, hair, feature_masks, radius):
    """原画の不透明な髪の下だけへ補完する。可視領域は一切上書きしない。"""
    if source.shape != generated.shape or face.shape != source.shape[:2]:
        raise ValueError('比較素材の寸法が一致しません')
    # 輪郭の黒線・髪色を肌色の基準へ混ぜると、補完帯に灰色の筋が生じる。
    skin=ndimage.binary_erosion(face,iterations=max(1,round(radius/6)))
    for feature in feature_masks:skin &= ~ndimage.binary_dilation(feature,iterations=3)
    skin &= source[:,:,3]==255
    if skin.sum()<16:raise ValueError('色合わせ用の可視肌が不足しています')
    distance=ndimage.distance_transform_edt(~face)
    hidden=(distance>0)&(distance<=radius)&hair&(source[:,:,3]==255)
    # 検出の下端より下へ顔の肌を延ばして襟を覆わない。
    rows=np.nonzero(face)[0]
    if not rows.size:raise ValueError('原画の顔が空です')
    hidden[rows.max()+1:]=False
    if not hidden.any():raise ValueError('不透明な髪の下に補完領域がありません')
    sigma=max(1,radius)
    denominator=ndimage.gaussian_filter(skin.astype(float),sigma)
    correction=np.zeros((*face.shape,3),dtype=float)
    for channel in range(3):
        delta=(source[:,:,channel].astype(float)-generated[:,:,channel])*skin
        correction[:,:,channel]=ndimage.gaussian_filter(delta,sigma)/np.maximum(denominator,1e-8)
    # 正規化畳み込みで連続した色差を延ばし、最近傍領域の境界を作らない。
    corrected=np.rint(np.clip(generated[:,:,:3].astype(float)+correction,0,255)).astype(np.uint8)
    result=np.zeros_like(source);result[:,:,:3]=corrected;result[:,:,3]=hidden*255
    return result,hidden


def enclosed_hair_regions(hair,protected,opaque):
    """髪の閉領域を一体として回収する。目口を含む穴は領域全体を除外する。"""
    holes=ndimage.binary_fill_holes(hair)&~hair
    labels,_=ndimage.label(holes)
    blocked=np.unique(labels[protected])
    return holes&~np.isin(labels,blocked)&opaque


def build(character,comparison,band_ratio=.08,motion_ratio=.35,side_comparison=None,edge_band_ratio=.015,edge_gain=40,redraw_ear_contour=False):
    character=character.resolve();comparison=comparison.resolve()
    if not character.is_relative_to(ROOT/'temp') or not comparison.is_relative_to(ROOT/'temp'):
        raise ValueError('比較入出力はtemp内に限定します')
    report=json.loads((comparison/'report.json').read_text(encoding='utf-8'))
    if report['status']!='complete' or report['mode']!='edit' or report['source']['view']!='head':
        raise ValueError('完了した原寸頭部Image-Edit比較が必要です')
    verify_source(character,report['source'])
    if not 0<band_ratio<=.15 or not 0<motion_ratio<=.4:raise ValueError('補完/変位範囲が不正です')
    original_rig=character/'rig2d/rig.json'
    baseline=baseline_hashes(character)
    rig=json.loads(original_rig.read_text(encoding='utf-8'))
    left,top,right,bottom=report['source']['source_region']
    with Image.open(character/'source/isolated.png') as image:source=np.asarray(image.convert('RGBA'))
    generated_path=(comparison/report['images'][0]).resolve()
    if not generated_path.is_relative_to(comparison/'output'):raise ValueError('比較画像が範囲外です')
    with Image.open(generated_path) as image:generated=np.asarray(image.convert('RGBA'))
    if generated.shape[:2]!=(bottom-top,right-left):raise ValueError('補完画像は原寸である必要があります')
    full_face=np.zeros(source.shape[:2],dtype=bool)
    box=rig['layers']['scene_face']['texture_box']
    with Image.open(character/'rig2d/parts/scene_face.png') as image:
        full_face[box[1]:box[3],box[0]:box[2]]=np.asarray(image)[:,:,3]>0
    with np.load(character/'analysis/masks.npz',allow_pickle=False) as masks:
        full_hair=masks['hair'].copy();hair=full_hair[top:bottom,left:right]
        features=[masks[name][top:bottom,left:right].copy() for name in ('left_eye','right_eye','mouth')]
    side_identity=None;side_path=None
    if not 0<edge_band_ratio<=.05 or not 0<edge_gain<=255:raise ValueError('境界調整の範囲が不正です')
    if side_comparison is not None:
        side_comparison=side_comparison.resolve()
        if not side_comparison.is_relative_to(ROOT/'temp'):raise ValueError('横髪比較もtemp内に限定します')
        side_report=json.loads((side_comparison/'report.json').read_text(encoding='utf-8'))
        if side_report['status']!='complete' or side_report['mode']!='edit' or side_report['source']!=report['source']:
            raise ValueError('横髪比較は同一の原画・解析・原寸領域で完了している必要があります')
        side_path=(side_comparison/side_report['images'][0]).resolve()
        if not side_path.is_relative_to(side_comparison/'output'):raise ValueError('横髪比較画像が範囲外です')
        with Image.open(side_path) as image:side_pixels=np.asarray(image.convert('RGBA'))
        if side_pixels.shape!=generated.shape:raise ValueError('横髪補完は原寸である必要があります')
        face_box=rig['layers']['face']['bbox'];edge_band=max(1,round((face_box[2]-face_box[0])*edge_band_ratio))
        # 顔だけでなく未分類に残った髪の境界も同じ条件で回収する。
        boundary_surface=full_face.copy();residual=np.zeros(full_face.shape,bool);residual_box=rig['layers']['scene_residual']['texture_box']
        with Image.open(character/'rig2d/parts/scene_residual.png') as image:
            residual[residual_box[1]:residual_box[3],residual_box[0]:residual_box[2]]=np.asarray(image)[:,:,3]>0
        boundary_surface |= residual
        # 飾りが顔と未分類に分かれていても閉領域全体を回収し、境界だけを置き去りにしない。
        enclosed=enclosed_hair_regions(hair,np.logical_or.reduce(features),source[top:bottom,left:right,3]==255)
        enclosed_count=int(enclosed.sum());hair=hair|enclosed;full_hair[top:bottom,left:right]=hair
        refined=refine_side_hair(source[top:bottom,left:right],side_pixels,boundary_surface[top:bottom,left:right],hair,features,edge_band,edge_gain)
        refined_count=int((refined&~hair).sum())
        full_hair[top:bottom,left:right]=refined;hair=refined
        # 実測した目の高さを境に滑らかに切替える。前髪下地へ横髪編集の髪を混ぜない。
        eye_rows=np.nonzero(features[0]|features[1])[0]
        if not eye_rows.size:raise ValueError('横髪補完の切替に必要な目の実測領域がありません')
        blend=np.clip((np.arange(generated.shape[0])-eye_rows.min())/max(1,int(eye_rows.max()-eye_rows.min())),0,1)
        blend=(blend*blend*(3-2*blend))[:,None,None]
        generated=np.rint(generated*(1-blend)+side_pixels*blend).astype(np.uint8)
        side_identity={'generated_sha256':digest(side_path),'source':side_report['source'],'blend':'measured-eye-height',
                       'edge_band_ratio':edge_band_ratio,'edge_gain':edge_gain,'reassigned_pixels':refined_count,'enclosed_hair_pixels':enclosed_count}
    full_face &= ~full_hair
    face=full_face[top:bottom,left:right]
    face_box=rig['layers']['face']['bbox'];radius=max(1,round((face_box[2]-face_box[0])*band_ratio))
    pixels,hidden=hidden_material(source[top:bottom,left:right],generated,face,hair,features,radius)
    if redraw_ear_contour and side_path is None:raise ValueError('耳輪郭の修正には横髪比較が必要です')
    ear_mask=None;ear_mask_path=None;ear_identity=None;old_ear_mask=None;old_ear_path=None
    if redraw_ear_contour:
        ear_report=json.loads((side_comparison/'ears/report.json').read_text(encoding='utf-8'))
        if ear_report['status']!='complete' or ear_report['source_sha256']!=digest(side_path):raise ValueError('耳解析が比較画像と一致しません')
        ear_mask_path=side_comparison/'ears/masks.npz'
        with np.load(ear_mask_path,allow_pickle=False) as masks:ear_mask=masks['left']|masks['right']
        if ear_mask.dtype!=bool or ear_mask.shape!=face.shape or not ear_mask.any():raise ValueError('耳の原寸マスクが不正です')
        ear_identity={'mask_sha256':digest(ear_mask_path),'source_sha256':ear_report['source_sha256']}
        old_report=json.loads((side_comparison/'source-ears/report.json').read_text(encoding='utf-8'))
        if old_report['status']!='complete' or old_report['original_source']!=report['source'] or old_report['source_sha256']!=digest(side_comparison/'reference.png'):
            raise ValueError('元の耳解析が原画と一致しません')
        old_ear_path=side_comparison/'source-ears/masks.npz'
        with np.load(old_ear_path,allow_pickle=False) as masks:old_ear_mask=masks['left']|masks['right']
        if old_ear_mask.dtype!=bool or old_ear_mask.shape!=face.shape or not old_ear_mask.any():raise ValueError('元の可視耳マスクが不正です')
        old_ear_mask=ndimage.binary_dilation(old_ear_mask,iterations=1)
        ear_identity['original_mask_sha256']=digest(old_ear_path)
    identity={'version':19,'source':report['source'],'generated_sha256':digest(generated_path),'side_comparison':side_identity,'redraw_ear_contour':redraw_ear_contour,'ear_segmentation':ear_identity,
              'baseline_assets_sha256':baseline,'band_ratio':band_ratio,'motion_ratio':motion_ratio}
    identifier='c_'+hashlib.sha256(json.dumps(identity,sort_keys=True).encode()).hexdigest()[:12]
    destination=ROOT/'temp/t7-characters'/identifier
    if destination.exists():raise ValueError('同じ比較候補が既にあります。上書きしません')
    support=np.zeros(source.shape[:2],dtype=bool);support[top:bottom,left:right]=face|hidden
    clearance=ndimage.distance_transform_edt(support)
    height,width=source.shape[:2];cols,rows=64,96
    # 共通格子に対応する局所変位重み。補完のない外周や首肩は動かさない。
    weights=[round(min(1,float(clearance[min(height-1,round(y*height/rows)),min(width-1,round(x*width/cols))])/radius),6)
             for y in range(rows+1) for x in range(cols+1)]
    if max(weights)==0:raise ValueError('補完領域が格子に届きません')
    full_hidden=np.zeros_like(source);full_hidden[top:bottom,left:right]=pixels
    image=Image.fromarray(full_hidden);bounds=image.getchannel('A').getbbox()
    if bounds is None:raise ValueError('補完下地が空です')
    repair=None;repair_bounds=None
    if side_path is not None:
        # 通常比較は動作時だけ補修し、承認済み輪郭修正版は中立にも合成する。
        # 目口を保護し、髪に隣接する耳・頬の帯と測定した耳へ限定する。
        distance=ndimage.distance_transform_edt(~hair)
        alpha=np.clip((radius-distance)/(radius*.5 if redraw_ear_contour else radius),0,1)*(face|hidden if redraw_ear_contour else face)*(source[top:bottom,left:right,3]==255)
        if redraw_ear_contour:
            # 色差は背景も拾うため使わない。DINO/SAMで測定した耳の外形だけを採る。
            extension=ndimage.binary_dilation(ear_mask,iterations=1)&hair
            alpha[extension&(source[top:bottom,left:right,3]==255)]=1
        eye_rows=np.nonzero(features[0]|features[1])[0];mouth_rows=np.nonzero(features[2])[0]
        alpha[:int(eye_rows.min()) if redraw_ear_contour else round(float(np.median(eye_rows)))]=0;alpha[mouth_rows.min():]=0
        if redraw_ear_contour:alpha[(extension|ear_mask|old_ear_mask)&(source[top:bottom,left:right,3]==255)]=1
        for feature in features:alpha[ndimage.binary_dilation(feature,iterations=3)]=0
        if redraw_ear_contour:
            # 耳を覆う旧髪の切抜き形状も新しい耳へ合わせ、古い矩形状の穴を残さない。
            revealed=extension&(alpha>0)
            full_hair[top:bottom,left:right][revealed]=False
        repair_pixels=(side_pixels if redraw_ear_contour else pixels).copy();repair_pixels[:,:,3]=np.rint(alpha*255).astype(np.uint8)
        full_repair=np.zeros_like(source);full_repair[top:bottom,left:right]=repair_pixels
        repair=Image.fromarray(full_repair);repair_bounds=repair.getchannel('A').getbbox()
        if repair_bounds is None:raise ValueError('耳の動作時補修領域がありません')
    # 独立移動時に二重線となる、不透明な髪/顔の境界共有だけを整理する。
    overrides={};override_boxes={};full_old_ears=np.zeros(source.shape[:2],bool)
    if redraw_ear_contour:full_old_ears[top:bottom,left:right]=old_ear_mask
    for name in (part['layer'] for part in rig['scene_graph']):
        b=rig['layers'][name]['texture_box']
        with Image.open(character/'rig2d/parts'/f'{name}.png') as opened:part=np.array(opened)
        owned=full_hair[b[1]:b[3],b[0]:b[2]]
        if name=='scene_hair':part[owned]=source[b[1]:b[3],b[0]:b[2]][owned]
        if redraw_ear_contour:part[full_old_ears[b[1]:b[3],b[0]:b[2]],3]=0
        overrides[name]=Image.fromarray(separate_hair_pixels(part,owned,name=='scene_hair'))
    if redraw_ear_contour:
        b=rig['layers']['scene_face']['texture_box'];completed=Image.new('RGBA',(width,height))
        completed.alpha_composite(overrides['scene_face'],(b[0],b[1]));completed.alpha_composite(repair)
        completed_bounds=completed.getchannel('A').getbbox()
        overrides['scene_face']=completed.crop(completed_bounds);override_boxes['scene_face']=list(completed_bounds)
        # まばたき下地が元の耳・髪を顔の上へ描き戻さないよう、同じ所有マスクを適用する。
        excluded=full_hair|full_old_ears|(np.asarray(repair)[:,:,3]>0)
        for name in ['left_eye_base','right_eye_base','left_eye_backplate','right_eye_backplate']:
            b=rig['layers'][name]['texture_box']
            with Image.open(character/'rig2d/parts'/f'{name}.png') as opened:part=np.array(opened)
            overrides[name]=Image.fromarray(separate_hair_pixels(part,excluded[b[1]:b[3],b[0]:b[2]],False))
    # 承認された耳以外の中立合成を変えないことを全画素で検証する。
    def composite(with_hidden):
        result=Image.new('RGBA',(width,height))
        for part in rig['scene_graph']:
            if part['role']=='face' and with_hidden:result=Image.alpha_composite(result,image)
            layer=rig['layers'][part['layer']];b=override_boxes.get(part['layer'],layer['texture_box']) if with_hidden else layer['texture_box']
            with Image.open(character/'rig2d/parts'/f"{part['layer']}.png") as opened:
                result.alpha_composite(overrides.get(part['layer'],opened) if with_hidden else opened,(b[0],b[1]))
        return np.asarray(result)
    before=composite(False);after=composite(True)
    if not np.array_equal(before,source):raise ValueError('元リグの中立合成が原画と一致しません')
    changed=np.any(before!=after,axis=2)
    allowed=np.asarray(repair)[:,:,3]>0 if redraw_ear_contour else np.zeros((height,width),bool)
    if np.any(changed&~allowed):raise ValueError('承認された耳の帯以外の中立画素を変えました')
    if np.any(changed[top:bottom,left:right] & np.logical_or.reduce(features)):
        raise ValueError('耳の修正が目口の可視画素を変えました')
    with directory_output(destination) as pending:
        (pending/'source').mkdir();shutil.copy2(character/'source/input.png',pending/'source/input.png')
        shutil.copytree(character/'rig2d',pending/'rig2d')
        for name,part in overrides.items():bleed_transparent_rgb(part).save(pending/f'rig2d/parts/{name}.png')
        for name,b in override_boxes.items():rig['layers'][name]['texture_box']=b
        name='scene_hidden_face';bleed_transparent_rgb(image.crop(bounds)).save(pending/f'rig2d/parts/{name}.png')
        rig['layers'][name]={'url':f'/assets/rig2d/parts/{name}.png','bbox':list(bounds),'texture_box':list(bounds),
                            'pivot':rig['layers']['face']['pivot'],'z_index':rig['layers']['scene_face']['z_index']}
        index=next(i for i,p in enumerate(rig['scene_graph']) if p['role']=='face')
        rig['scene_graph'].insert(index,{'layer':name,'role':'hidden_face','parent':'face','source_region':list(bounds),
                                        'hidden_regions':'partial','status':'unverified','owned_pixels':int(hidden.sum())})
        rig['experimental_hidden']={**identity,'status':'unapproved','hidden_pixels':int(hidden.sum()),
                                    'neutral_changed_pixels':int(changed.sum()),'radius_px':radius}
        rig['hidden_motion']={'version':1,'weights':weights,'angle_limit':15,'x_limit_px':radius*motion_ratio,'y_limit_px':radius*motion_ratio*.5}
        if repair is not None and not redraw_ear_contour:
            repair_name='scene_ear_repair';bleed_transparent_rgb(repair.crop(repair_bounds)).save(pending/f'rig2d/parts/{repair_name}.png')
            rig['layers'][repair_name]={'url':f'/assets/rig2d/parts/{repair_name}.png','bbox':list(repair_bounds),'texture_box':list(repair_bounds),
                                       'pivot':rig['layers']['face']['pivot'],'z_index':rig['layers']['scene_face']['z_index']}
            rig['hidden_motion']['repair_layer']=repair_name
        rig['material_readiness']['note']='比較専用。髪の下の顔を局所補完。首/襟・全身独立関節は未完成。'
        (pending/'rig2d/rig.json').write_text(json.dumps(rig,ensure_ascii=False,indent=2),encoding='utf-8')
        state={'schemaVersion':1,'characterId':identifier,'displayName':'耳輪郭修正比較' if redraw_ear_contour else '原画固定・補完比較',
               'stages':{'rig2d':{'status':'complete','updatedAtIso':datetime.now(timezone.utc).isoformat()}},
               'experimental':identity,'baselineCharacterId':character.name}
        (pending/'character.json').write_text(json.dumps(state,ensure_ascii=False,indent=2),encoding='utf-8')
        verify_source(character,report['source'])
        if baseline_hashes(character)!=baseline or digest(generated_path)!=identity['generated_sha256']:
            raise ValueError('候補の作成中に原リグまたは補完画像が変わりました')
        if side_path is not None and digest(side_path)!=side_identity['generated_sha256']:
            raise ValueError('候補の作成中に横髪の補完画像が変わりました')
        if ear_mask_path is not None and digest(ear_mask_path)!=ear_identity['mask_sha256']:
            raise ValueError('候補の作成中に耳マスクが変わりました')
        if old_ear_path is not None and digest(old_ear_path)!=ear_identity['original_mask_sha256']:
            raise ValueError('候補の作成中に元の耳マスクが変わりました')
    print(json.dumps({'character_id':identifier,'hidden_pixels':int(hidden.sum()),'neutral_changed_pixels':int(changed.sum()),'directory':str(destination)}))


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--character',type=Path,required=True);parser.add_argument('--comparison',type=Path,required=True)
    parser.add_argument('--band-ratio',type=float,default=.08);parser.add_argument('--motion-ratio',type=float,default=.35)
    parser.add_argument('--side-comparison',type=Path,help='同じ原寸頭部の横髪除去・耳補完の比較結果')
    parser.add_argument('--edge-band-ratio',type=float,default=.015);parser.add_argument('--edge-gain',type=float,default=40)
    parser.add_argument('--redraw-ear-contour',action='store_true',help='利用者が承認した場合だけ、可視の耳境界も局所修正する')
    args=parser.parse_args();build(args.character,args.comparison,args.band_ratio,args.motion_ratio,args.side_comparison,args.edge_band_ratio,args.edge_gain,args.redraw_ear_contour)
