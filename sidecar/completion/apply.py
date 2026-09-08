"""原寸の閉眼画像を通常リグの独立素材へ適用する。"""
import numpy as np
from PIL import Image
from scipy import ndimage
from materials import closed_curve, reconstruct_closed_skin, reconstruct_eye_skin, extract_lid_ink, replace_closed_backing


def apply_closed_eyes(character,base,rig,edited,source,mask,bounds):
    l,t,r,b=bounds
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
            eye_width=feature[2]-feature[0]
            expansion=max(2,round(eye_width*.18))
            feather=max(2,round(eye_width*.06))
            expanded_eye=ndimage.binary_dilation(eye,iterations=expansion)
            # 目の意味領域に接する睫毛は髪マスクへ混入し得る。内側半分は開眼素材として
            # 下地から除き、外側の前髪だけを保護する。
            protected_hair=hair&~ndimage.binary_dilation(eye,iterations=max(1,expansion//2))
            region=expanded_eye&~protected_hair&(source[:,:,3]>0)&(mask>0)
            face=masks['face'][t:b,l:r]
            rgb=source[:,:,:3].astype(np.int16)
            skin_color=(rgb[:,:,0]>rgb[:,:,2]+5)&(rgb[:,:,0]>=rgb[:,:,1]-8)&(rgb.mean(axis=2)>100)
            donors=face&~hair&skin_color&~ndimage.binary_dilation(eye,iterations=expansion+feather)&(source[:,:,3]>0)
            ring=region&~eye
            if not ring.any():raise ValueError('閉眼の肌色を照合する周辺がありません')
            correction=np.median(source[:,:,:3].astype(float)[ring]-edited[:,:,:3].astype(float)[ring],axis=0)
            corrected=np.rint(np.clip(edited[:,:,:3].astype(float)+correction,0,255))
            weight=np.clip(ndimage.distance_transform_edt(region)/feather,0,1)
            weight*=mask/255
            clean,removed,skin_measurements=reconstruct_closed_skin(corrected,line_mask,(weight>0)&~hair)
            eye_skin,eye_skin_measurements=reconstruct_eye_skin(source[:,:,:3].astype(float),region,donors)
            ys=slice(y0-t,y1-t);xs=slice(x0-l,x1-l)
            with Image.open(base/f'parts/{name}.png') as opened:pixels=np.array(opened)
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
                with Image.open(base/f'parts/{material}.png') as opened:backing=np.array(opened.convert('RGBA'))
                protected=masks['hair'][mt:mb,ml:mr]
                if protected.shape!=backing.shape[:2]:raise ValueError('目の下地と髪の所有マスクが一致しません')
                count=int(((backing[:,:,3]>0)&protected).sum());protected_count+=count
                # 髪の意味マスクに漏れがあっても、目の編集範囲外へ肌を貼らない。
                backing[protected,3]=0
                local_mask=mask[mt-t:mb-t,ml-l:mr-l]
                if local_mask.shape!=backing.shape[:2]:raise ValueError('目の下地が局所編集範囲外です')
                backing[:,:,3]=np.rint(backing[:,:,3].astype(float)*local_mask/255).astype(np.uint8)
                if material==name:
                    # 白目のeye_backplateは肌に変えず、閉眼を覆うeye_baseだけ更新する。
                    backing=replace_closed_backing(pixels,eye_skin[ys,xs],weight[ys,xs],protected)
                results[material]=Image.fromarray(backing)
            measured[side]={'ink_pixels':int((ink[:,:,3]>0).sum()),'skin_correction':correction.tolist(),'expansion_px':expansion,'feather_px':feather,'protected_hair_pixels':protected_count,'curve_detection':curve_measurements,'skin_reconstruction':skin_measurements,'eye_skin_reconstruction':eye_skin_measurements}
    return results,measured
