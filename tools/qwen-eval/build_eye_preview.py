"""承認済みの比較経路で作った閉眼だけを、既存候補へ原寸で追加する。"""
import argparse
from datetime import datetime,timezone
import hashlib
import json
from pathlib import Path
import shutil
import numpy as np
from PIL import Image
from scipy import ndimage
from build_preview import ROOT,baseline_hashes,directory_output,bleed_transparent_rgb
from run import verify_source
from download import digest


def closed_curve(pixels,box,protected,aperture):
    """閉眼の暗い連続まつげを測定し、元の開口列へ対応させる。"""
    l,t,r,b=box
    gray=pixels[t:b,l:r,:3].astype(float).mean(axis=2)
    available=~protected[t:b,l:r]
    if not available.any():raise ValueError('閉眼を測定する領域がありません')
    low,high=np.percentile(gray[available],[1,90])
    if high-low<1:raise ValueError('閉眼の線にコントラストがありません')
    labels,count=ndimage.label((gray<(low+high)/2)&available)
    if not count:raise ValueError('閉眼のまつげを検出できません')
    sizes=np.bincount(labels.ravel());sizes[0]=0;selected=labels==sizes.argmax()
    ys,xs=np.nonzero(selected)
    if np.ptp(xs)<(r-l)*.4 or np.ptp(ys)>(b-t)*.7:
        raise ValueError('閉眼の細長いまつげとして成立しません。原画を置き換えません')
    columns=np.unique(xs)
    measured=np.array([np.nonzero(selected[:,x])[0].mean()+t+.5 for x in columns])
    measured=ndimage.gaussian_filter1d(measured,max(1,len(columns)*.02))
    return np.interp([point[0] for point in aperture],columns+l+.5,measured).tolist()


def extract_lid_ink(pixels,target,weight):
    """肌色を背景として暗い線を透過化し、肌の矩形を持ち運ばない。"""
    if pixels.ndim!=3 or pixels.shape[2]!=4 or target.shape!=pixels[:,:,:3].shape or weight.shape!=pixels.shape[:2]:
        raise ValueError('まぶたの透過化の寸法が一致しません')
    if not np.isfinite(weight).all() or np.any((weight<0)|(weight>1)):raise ValueError('まぶたの領域重みが不正です')
    clean=pixels[:,:,:3].astype(float)
    matte=np.clip(np.max((clean-target)/np.maximum(clean,1),axis=2),0,1)
    ink=np.zeros_like(pixels)
    ink[:,:,:3]=np.rint(np.clip((target-clean*(1-matte[:,:,None]))/np.maximum(matte[:,:,None],1e-8),0,255)).astype(np.uint8)
    ink[:,:,3]=np.rint(matte*weight*pixels[:,:,3]).astype(np.uint8)
    return ink


def validate_base_identity(character,base,rig,source):
    """同じ原画の通常リグ、または署名が一致する補完候補だけを許可する。"""
    if base!=character and rig.get('experimental_hidden',{}).get('source')!=source:
        raise ValueError('候補と閉眼編集の原画・解析が一致しません')


def build(character,base,comparison):
    character=character.resolve();base=base.resolve();comparison=comparison.resolve()
    if any(not path.is_relative_to(ROOT/'temp') for path in (character,base,comparison)):
        raise ValueError('比較入出力はtemp内に限定します')
    report=json.loads((comparison/'report.json').read_text(encoding='utf-8'))
    if report['status']!='complete' or report['mode']!='edit' or report['source']['view']!='head':
        raise ValueError('完了した原寸頭部の編集が必要です')
    verify_source(character,report['source']);baseline=baseline_hashes(base)
    rig=json.loads((base/'rig2d/rig.json').read_text(encoding='utf-8'))
    validate_base_identity(character,base,rig,report['source'])
    if digest(base/'source/input.png')!=report['source']['source_sha256']:
        raise ValueError('候補の原画が一致しません')
    edited_path=(comparison/report['images'][0]).resolve()
    if not edited_path.is_relative_to(comparison/'output'):raise ValueError('編集画像が範囲外です')
    fingerprint=digest(edited_path)
    with Image.open(edited_path) as opened:edited=np.array(opened.convert('RGBA'))
    l,t,r,b=report['source']['source_region']
    if edited.shape[:2]!=(b-t,r-l):raise ValueError('閉眼画像は原寸である必要があります')
    with Image.open(character/'source/isolated.png') as opened:source=np.array(opened.convert('RGBA').crop((l,t,r,b)))
    identity={'version':2,'base_assets':baseline,'source':report['source'],'edited_sha256':fingerprint}
    identifier='c_'+hashlib.sha256(json.dumps(identity,sort_keys=True).encode()).hexdigest()[:12]
    destination=ROOT/'temp/t7-characters'/identifier
    if destination.exists():raise ValueError('同じ閉眼候補が既にあります。上書きしません')
    results={};measured={}
    with np.load(character/'analysis/masks.npz',allow_pickle=False) as masks:
        hair=masks['hair'][t:b,l:r]
        for side in ['left','right']:
            name=side+'_eye_base';layer=rig['layers'][name]
            box=layer['texture_box'];x0,y0,x1,y1=box
            if not l<=x0<x1<=r or not t<=y0<y1<=b:raise ValueError('閉眼素材が編集範囲外です')
            feature=layer['feature_box'];local=[feature[0]-l,feature[1]-t,feature[2]-l,feature[3]-t]
            aperture=[[point[0]-l,*point[1:]] for point in layer['eye_aperture']]
            curve=np.array(closed_curve(edited,local,hair,aperture))+t
            eye=masks[side+'_eye'][t:b,l:r]
            margin=max(1,round((feature[2]-feature[0])*.08))
            region=ndimage.binary_dilation(eye,iterations=margin)&~hair&(source[:,:,3]==255)
            ring=region&~eye
            if not ring.any():raise ValueError('閉眼の肌色を照合する周辺がありません')
            correction=np.median(source[:,:,:3].astype(float)[ring]-edited[:,:,:3].astype(float)[ring],axis=0)
            corrected=np.rint(np.clip(edited[:,:,:3].astype(float)+correction,0,255))
            weight=np.clip(ndimage.distance_transform_edt(region)/margin,0,1)
            ys=slice(y0-t,y1-t);xs=slice(x0-l,x1-l)
            with Image.open(base/f'rig2d/parts/{name}.png') as opened:pixels=np.array(opened)
            # 肌を重ねると半閉眼で閉じた線が瞳の下へ残る。暗い線だけを透過素材にする。
            ink=extract_lid_ink(pixels,corrected[ys,xs],weight[ys,xs])
            if not ink[:,:,3].any():raise ValueError('閉眼の線の透過素材が空です')
            if np.any((ink[:,:,3]>0)&(weight[ys,xs]==0)):raise ValueError('目の周辺以外に線を作りました')
            layer['closed_curve']=curve.tolist();layer['closed_material']=True
            # 描画器が参照する閉眼位置と、新素材の線の位置を一致させる。
            for point,value in zip(layer['eye_aperture'],curve):point[3]=float(value)
            upper_name=side+'_eyelid_upper';upper=rig['layers'][upper_name]
            upper['texture_box']=box.copy();upper['bbox']=box.copy();upper['eye_aperture']=layer['eye_aperture']
            results[upper_name]=Image.fromarray(ink)
            measured[side]={'ink_pixels':int((ink[:,:,3]>0).sum()),'skin_correction':correction.tolist(),'feather_px':margin}
    with directory_output(destination) as pending:
        shutil.copytree(base/'rig2d',pending/'rig2d');(pending/'source').mkdir()
        shutil.copy2(base/'source/input.png',pending/'source/input.png')
        for name,image in results.items():bleed_transparent_rgb(image).save(pending/f'rig2d/parts/{name}.png')
        rig['experimental_closed_eyes']={**identity,'status':'unapproved','measurements':measured}
        (pending/'rig2d/rig.json').write_text(json.dumps(rig,ensure_ascii=False,indent=2),encoding='utf-8')
        state={'schemaVersion':1,'characterId':identifier,'displayName':'閉眼修正比較','baselineCharacterId':base.name,
               'stages':{'rig2d':{'status':'complete','updatedAtIso':datetime.now(timezone.utc).isoformat()}},'experimental':identity}
        (pending/'character.json').write_text(json.dumps(state,ensure_ascii=False,indent=2),encoding='utf-8')
        verify_source(character,report['source'])
        if baseline_hashes(base)!=baseline or digest(edited_path)!=fingerprint:raise ValueError('比較中に元の素材が変わりました')
    print(json.dumps({'character_id':identifier,'measurements':measured,'directory':str(destination)}),flush=True)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--character',type=Path,required=True);parser.add_argument('--base',type=Path,required=True)
    parser.add_argument('--comparison',type=Path,required=True)
    args=parser.parse_args();build(args.character,args.base,args.comparison)
