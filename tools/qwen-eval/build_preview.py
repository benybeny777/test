"""補完候補を原画固定の独立下地として、比較専用リグへ追加する。"""
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


def hidden_material(source, generated, face, hair, feature_masks, radius):
    """原画の不透明な髪の下だけへ補完する。可視領域は一切上書きしない。"""
    if source.shape != generated.shape or face.shape != source.shape[:2]:
        raise ValueError('比較素材の寸法が一致しません')
    skin=face.copy()
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
    corrected=np.clip(generated[:,:,:3].astype(float)+correction,0,255).astype(np.uint8)
    result=np.zeros_like(source);result[:,:,:3]=corrected;result[:,:,3]=hidden*255
    return result,hidden


def build(character,comparison,band_ratio=.08,motion_ratio=.35):
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
    full_face &= ~full_hair
    face=full_face[top:bottom,left:right]
    face_box=rig['layers']['face']['bbox'];radius=max(1,round((face_box[2]-face_box[0])*band_ratio))
    pixels,hidden=hidden_material(source[top:bottom,left:right],generated,face,hair,features,radius)
    identity={'version':3,'source':report['source'],'generated_sha256':digest(generated_path),
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
    # 独立移動時に二重線となる、不透明な髪/顔の境界共有だけを整理する。
    overrides={}
    for name in ('scene_face','scene_hair'):
        b=rig['layers'][name]['texture_box']
        with Image.open(character/'rig2d/parts'/f'{name}.png') as opened:part=np.array(opened)
        owned=full_hair[b[1]:b[3],b[0]:b[2]]
        if name=='scene_face':part[owned,3]=0
        else:part[~owned,3]=0
        overrides[name]=Image.fromarray(part)
    # 中立で既存合成を変えないことを全画素で検証する。
    def composite(with_hidden):
        result=Image.new('RGBA',(width,height))
        for part in rig['scene_graph']:
            if part['role']=='face' and with_hidden:result=Image.alpha_composite(result,image)
            layer=rig['layers'][part['layer']];b=layer['texture_box']
            with Image.open(character/'rig2d/parts'/f"{part['layer']}.png") as opened:
                result.alpha_composite(overrides.get(part['layer'],opened) if with_hidden else opened,(b[0],b[1]))
        return np.asarray(result)
    before=composite(False);after=composite(True)
    if not np.array_equal(before,after):raise ValueError('補完下地が中立の可視画素を変えました')
    with directory_output(destination) as pending:
        (pending/'source').mkdir();shutil.copy2(character/'source/input.png',pending/'source/input.png')
        shutil.copytree(character/'rig2d',pending/'rig2d')
        for name,part in overrides.items():bleed_transparent_rgb(part).save(pending/f'rig2d/parts/{name}.png')
        name='scene_hidden_face';bleed_transparent_rgb(image.crop(bounds)).save(pending/f'rig2d/parts/{name}.png')
        rig['layers'][name]={'url':f'/assets/rig2d/parts/{name}.png','bbox':list(bounds),'texture_box':list(bounds),
                            'pivot':rig['layers']['face']['pivot'],'z_index':rig['layers']['scene_face']['z_index']}
        index=next(i for i,p in enumerate(rig['scene_graph']) if p['role']=='face')
        rig['scene_graph'].insert(index,{'layer':name,'role':'hidden_face','parent':'face','source_region':list(bounds),
                                        'hidden_regions':'partial','status':'unverified','owned_pixels':int(hidden.sum())})
        rig['experimental_hidden']={**identity,'status':'unapproved','hidden_pixels':int(hidden.sum()),
                                    'neutral_changed_pixels':0,'radius_px':radius}
        rig['hidden_motion']={'version':1,'weights':weights,'angle_limit':15,'x_limit_px':radius*motion_ratio,'y_limit_px':radius*motion_ratio*.5}
        rig['material_readiness']['note']='比較専用。髪の下の顔を局所補完。首/襟・全身独立関節は未完成。'
        (pending/'rig2d/rig.json').write_text(json.dumps(rig,ensure_ascii=False,indent=2),encoding='utf-8')
        state={'schemaVersion':1,'characterId':identifier,'displayName':'原画固定・補完比較',
               'stages':{'rig2d':{'status':'complete','updatedAtIso':datetime.now(timezone.utc).isoformat()}},
               'experimental':identity,'baselineCharacterId':character.name}
        (pending/'character.json').write_text(json.dumps(state,ensure_ascii=False,indent=2),encoding='utf-8')
        verify_source(character,report['source'])
        if baseline_hashes(character)!=baseline or digest(generated_path)!=identity['generated_sha256']:
            raise ValueError('候補の作成中に原リグまたは補完画像が変わりました')
    print(json.dumps({'character_id':identifier,'hidden_pixels':int(hidden.sum()),'neutral_changed_pixels':0,'directory':str(destination)}))


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--character',type=Path,required=True);parser.add_argument('--comparison',type=Path,required=True)
    parser.add_argument('--band-ratio',type=float,default=.08);parser.add_argument('--motion-ratio',type=float,default=.35)
    args=parser.parse_args();build(args.character,args.comparison,args.band_ratio,args.motion_ratio)
