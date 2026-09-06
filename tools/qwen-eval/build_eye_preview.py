"""承認済みの比較経路で作った閉眼だけを、既存候補へ原寸で追加する。"""
import argparse
from datetime import datetime,timezone
import hashlib
import json
from pathlib import Path
import shutil
import sys
import numpy as np
from PIL import Image
from scipy import ndimage
from build_preview import ROOT,baseline_hashes,directory_output,bleed_transparent_rgb,separate_hair_pixels
from run import verify_source
from download import digest


def closed_curve(pixels,box,protected,aperture,measurements=None,line_mask=None):
    """閉眼の暗い連続まつげを測定し、元の開口列へ対応させる。"""
    l,t,r,b=box
    gray=pixels[t:b,l:r,:3].astype(float).mean(axis=2)
    available=~protected[t:b,l:r]
    if not available.any():raise ValueError('閉眼を測定する領域がありません')
    low,high=np.percentile(gray[available],[1,90])
    if high-low<1:raise ValueError('閉眼の線にコントラストがありません')
    # 傾いた1画素の線も連続成分として扱う。髪の保護領域は引き続き除外する。
    labels,count=ndimage.label((gray<(low+high)/2)&available,structure=np.ones((3,3)))
    if not count:raise ValueError('閉眼のまつげを検出できません')
    candidates=[]
    for identifier in range(1,count+1):
        selected=labels==identifier
        columns=np.flatnonzero(selected.any(axis=0))
        if len(columns)<2 or np.ptp(columns)<(r-l)*.4:continue
        thickness=np.array([np.ptp(np.flatnonzero(selected[:,x]))+1 for x in columns])
        # 成分全体の縦幅は顔の傾きと長いまつげにも反応するため、列ごとの厚さで検査する。
        # 太い瞳/影を閉眼と誤認しないよう、細い列が続く横幅と元成分の保持率も要求する。
        thin=columns[thickness<=(b-t)*.35]
        if len(thin)<2:continue
        runs=np.split(thin,np.flatnonzero(np.diff(thin)>1)+1)
        for run in runs:
            if len(run)<2 or np.ptp(run)<(r-l)*.4 or len(run)<len(columns)*.6:continue
            candidates.append((len(run),int(selected[:,run].sum()),identifier,run,columns,thickness))
    if not candidates:
        raise ValueError('閉眼の細長いまつげとして成立しません。原画を置き換えません')
    _,_,identifier,columns,all_columns,thickness=max(candidates,key=lambda item:item[:2])
    selected=labels==identifier
    if line_mask is not None:
        if line_mask.shape!=pixels.shape[:2] or line_mask.dtype!=bool:raise ValueError('閉眼線の領域寸法が不正です')
        line_mask[t:b,l:r]=selected
    measured=np.array([np.nonzero(selected[:,x])[0].mean()+t+.5 for x in columns])
    measured=ndimage.gaussian_filter1d(measured,max(1,len(columns)*.02))
    if measurements is not None:
        ys,xs=np.nonzero(selected)
        measurements.update({'threshold':float((low+high)/2),'component_span':[int(np.ptp(xs)),int(np.ptp(ys))],
                             'median_column_thickness':float(np.median(thickness)),'max_column_thickness':int(thickness.max()),
                             'retained_columns':len(columns),'component_columns':len(all_columns),
                             'measured_x_span':int(np.ptp(columns)),'measured_y_span':float(np.ptp(measured)),
                             'trimmed_columns':int(len(all_columns)-len(columns))})
    return np.interp([point[0] for point in aperture],columns+l+.5,measured).tolist()


def reconstruct_closed_skin(edited,line_mask,allowed):
    """検出した暗線だけを周囲の生成済み肌から調和補間する。領域外は変更しない。"""
    from scipy import sparse
    from scipy.sparse.linalg import spsolve
    if edited.ndim!=3 or edited.shape[2]!=3 or line_mask.shape!=edited.shape[:2] or allowed.shape!=line_mask.shape:
        raise ValueError('閉眼肌の補間寸法が一致しません')
    if not np.isfinite(edited).all():raise ValueError('閉眼肌に不正な画素があります')
    core=line_mask&allowed
    if not core.any():raise ValueError('除去する閉眼線がありません')
    # 閾値を下回らない線の半透明縁も含める。髪や編集マスク外には広げない。
    remove=ndimage.binary_dilation(core,iterations=2)&allowed
    labels,count=ndimage.label(remove)
    for label in range(1,count+1):
        ring=ndimage.binary_dilation(labels==label)&allowed&~remove
        if ring.sum()<4:raise ValueError('閉眼線の周囲に補間用の肌が不足しています')
    ys,xs=np.nonzero(remove);indices=np.full(remove.shape,-1,dtype=np.int32);indices[ys,xs]=np.arange(len(xs))
    rows=[];columns=[];values=[];rhs=np.zeros((len(xs),3));h,w=remove.shape
    for index,(y,x) in enumerate(zip(ys,xs)):
        degree=0
        for ny,nx in ((y-1,x),(y+1,x),(y,x-1),(y,x+1)):
            if not (0<=ny<h and 0<=nx<w and allowed[ny,nx]):continue
            degree+=1
            if remove[ny,nx]:rows.append(index);columns.append(int(indices[ny,nx]));values.append(-1.)
            else:rhs[index]+=edited[ny,nx]
        rows.append(index);columns.append(index);values.append(float(degree))
    matrix=sparse.csr_matrix((values,(rows,columns)),shape=(len(xs),len(xs)))
    solved=spsolve(matrix,rhs)
    if not np.isfinite(solved).all():raise ValueError('閉眼肌の補間が成立しません')
    clean=edited.astype(float).copy();clean[ys,xs]=np.clip(solved,0,255)
    brightening=float((clean[core]-edited[core]).mean())
    if brightening<5:raise ValueError('閉眼の暗線を肌から分離できません。下地へ焼き込みません')
    return clean,remove,{'removed_line_pixels':int(core.sum()),'interpolated_pixels':int(remove.sum()),'line_brightening':brightening}


def replace_closed_backing(backing,clean,weight,protected):
    """生成済みの肌を編集範囲だけへ置き、保護領域では元の顔を透過表示する。"""
    if clean.shape!=backing[:,:,:3].shape or weight.shape!=backing.shape[:2] or protected.shape!=weight.shape:
        raise ValueError('閉眼下地の領域寸法が一致しません')
    if not np.isfinite(weight).all() or np.any((weight<0)|(weight>1)):raise ValueError('閉眼下地の領域重みが不正です')
    result=backing.copy();visible=(weight>0)&~protected
    result[visible,:3]=np.rint(np.clip(clean[visible],0,255)).astype(np.uint8)
    result[:,:,3]=np.rint(backing[:,:,3].astype(float)*weight*~protected).astype(np.uint8)
    return result


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


def validate_masked_pixels(edited,original,mask):
    """マスク外の移動や描き直しを、色の丸め誤差以外は拒否する。"""
    if edited.shape[:2]!=original.shape[:2] or mask.shape!=edited.shape[:2]:raise ValueError('局所編集の検証寸法が一致しません')
    outside=mask==0
    if not outside.any():raise ValueError('マスク外の保護領域がありません')
    if np.any(np.abs(edited[:,:,:3].astype(np.int16)-original[:,:,:3].astype(np.int16))[outside]>1):
        raise ValueError('編集マスク外の画素が変わっています。構図不一致の候補を公開しません')


def build(character,base,comparison,allow_unmasked=False):
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
    if report['source'].get('edit_region')=='eyes':
        mask_path=comparison/'input/eye-mask.png'
        if digest(mask_path)!=report['source'].get('edit_mask_sha256'):raise ValueError('編集マスクの署名が一致しません')
        with Image.open(mask_path) as opened:mask=np.array(opened.convert('L'))
        with Image.open(comparison/'input/input.png') as opened:original=np.array(opened.convert('RGB'))
        validate_masked_pixels(edited,original,mask)
    elif not allow_unmasked:
        raise ValueError('通常の閉眼比較には目の局所編集が必要です。非限定の旧比較は目視確認後に明示的に許可してください')
    l,t,r,b=report['source']['source_region']
    if edited.shape[:2]!=(b-t,r-l):raise ValueError('閉眼画像は原寸である必要があります')
    with Image.open(character/'source/isolated.png') as opened:source=np.array(opened.convert('RGBA').crop((l,t,r,b)))
    identity={'version':7,'base_assets':baseline,'source':report['source'],'edited_sha256':fingerprint}
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
            curve_measurements={}
            line_mask=np.zeros(edited.shape[:2],dtype=bool)
            curve=np.array(closed_curve(edited,local,hair,aperture,curve_measurements,line_mask))+t
            eye=masks[side+'_eye'][t:b,l:r]
            margin=max(1,round((feature[2]-feature[0])*.08))
            region=ndimage.binary_dilation(eye,iterations=margin)&~hair&(source[:,:,3]>0)
            ring=region&~eye
            if not ring.any():raise ValueError('閉眼の肌色を照合する周辺がありません')
            correction=np.median(source[:,:,:3].astype(float)[ring]-edited[:,:,:3].astype(float)[ring],axis=0)
            corrected=np.rint(np.clip(edited[:,:,:3].astype(float)+correction,0,255))
            weight=np.clip(ndimage.distance_transform_edt(region)/margin,0,1)
            if report['source'].get('edit_region')=='eyes':weight*=mask/255
            clean,removed,skin_measurements=reconstruct_closed_skin(corrected,line_mask,(weight>0)&~hair)
            ys=slice(y0-t,y1-t);xs=slice(x0-l,x1-l)
            with Image.open(base/f'rig2d/parts/{name}.png') as opened:pixels=np.array(opened)
            # 肌を重ねると半閉眼で閉じた線が瞳の下へ残る。暗い線だけを透過素材にする。
            clean_pixels=pixels.copy();clean_pixels[:,:,:3]=np.rint(clean[ys,xs]).astype(np.uint8)
            ink=extract_lid_ink(clean_pixels,corrected[ys,xs],weight[ys,xs]*removed[ys,xs])
            if not ink[:,:,3].any():raise ValueError('閉眼の線の透過素材が空です')
            if np.any((ink[:,:,3]>0)&(weight[ys,xs]==0)):raise ValueError('目の周辺以外に線を作りました')
            layer['closed_curve']=curve.tolist();layer['closed_material']=True
            # 描画器が参照する閉眼位置と、新素材の線の位置を一致させる。
            for point,value in zip(layer['eye_aperture'],curve):point[3]=float(value)
            upper_name=side+'_eyelid_upper';upper=rig['layers'][upper_name]
            upper['texture_box']=box.copy();upper['bbox']=box.copy();upper['eye_aperture']=layer['eye_aperture']
            results[upper_name]=Image.fromarray(ink)
            protected_count=0
            for material in (name,side+'_eye_backplate'):
                ml,mt,mr,mb=rig['layers'][material]['texture_box']
                with Image.open(base/f'rig2d/parts/{material}.png') as opened:backing=np.array(opened.convert('RGBA'))
                protected=masks['hair'][mt:mb,ml:mr]
                if protected.shape!=backing.shape[:2]:raise ValueError('目の下地と髪の所有マスクが一致しません')
                count=int(((backing[:,:,3]>0)&protected).sum());protected_count+=count
                # 髪の意味マスクに漏れがあっても、目の編集範囲外へ肌を貼らない。
                backing=separate_hair_pixels(backing,protected,False)
                if report['source'].get('edit_region')=='eyes':
                    local_mask=mask[mt-t:mb-t,ml-l:mr-l]
                    if local_mask.shape!=backing.shape[:2]:raise ValueError('目の下地が局所編集範囲外です')
                    backing[:,:,3]=np.rint(backing[:,:,3].astype(float)*local_mask/255).astype(np.uint8)
                if material==name:
                    # 白目のeye_backplateは肌に変えず、閉眼を覆うeye_baseだけ更新する。
                    backing=replace_closed_backing(pixels,clean[ys,xs],weight[ys,xs],protected)
                if count or report['source'].get('edit_region')=='eyes':results[material]=Image.fromarray(backing)
                if material==name:results[material]=Image.fromarray(backing)
            measured[side]={'ink_pixels':int((ink[:,:,3]>0).sum()),'skin_correction':correction.tolist(),'feather_px':margin,'protected_hair_pixels':protected_count,'curve_detection':curve_measurements,'skin_reconstruction':skin_measurements}
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
    sys.stdout.reconfigure(encoding='utf-8');sys.stderr.reconfigure(encoding='utf-8')
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--character',type=Path,required=True);parser.add_argument('--base',type=Path,required=True)
    parser.add_argument('--comparison',type=Path,required=True)
    parser.add_argument('--allow-unmasked-comparison',action='store_true',help='原寸と構図を目視確認した旧比較だけを許可する')
    args=parser.parse_args();build(args.character,args.base,args.comparison,args.allow_unmasked_comparison)
