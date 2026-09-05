"""原画の局所コントラストから目口を測定し、境界を保った差分を作る。"""

import numpy as np
from PIL import Image
from scipy import ndimage, sparse
from scipy.sparse.linalg import spsolve


def locate_features(rgba, face):
    """顔候補内の線・色差を検出する。測れない入力は成功扱いしない。"""
    ys, xs = np.nonzero(face)
    if not len(xs):
        raise ValueError("顔領域が空です")
    x0, y0, x1, y1 = xs.min(), ys.min(), xs.max()+1, ys.max()+1
    fw, fh = x1-x0, y1-y0
    rgb = rgba[..., :3].astype(float)
    gray = rgb.mean(axis=2)
    contrast = np.maximum(0, ndimage.gaussian_filter(gray, max(1, fw/20))-gray)
    boxes = {}
    # 比率は探索範囲にだけ使い、実際の中心と幅は画素から測定する。
    for name, roi in [('left_eye',(.08,.14,.47,.57)),
                      ('right_eye',(.53,.14,.92,.57)),
                      ('mouth',(.23,.45,.77,.85))]:
        l,t,r,b = [int(v) for v in (x0+fw*roi[0],y0+fh*roi[1],x0+fw*roi[2],y0+fh*roi[3])]
        if name == 'mouth':
            eye_y = max((boxes[key][1]+boxes[key][3])/2 for key in ('left_eye','right_eye'))
            t = max(t, int(eye_y + fw*.22))
            if t >= b:
                raise ValueError("目の下に口を探索できる領域がありません")
        score = contrast[t:b,l:r].copy()
        if name == 'mouth':
            red = rgb[t:b,l:r,0] - (rgb[t:b,l:r,1]+rgb[t:b,l:r,2])/2
            score += np.maximum(0, red-np.median(red))*.65
        score *= face[t:b,l:r]
        rows = ndimage.gaussian_filter1d(score.sum(axis=1), max(1,fh*.012))
        if not rows.size or rows.max() < 4:
            raise ValueError(f"{name}の位置を原画から測定できません")
        cy = int(rows.argmax())
        band = max(2, int(fh*(.035 if name=='mouth' else .045)))
        local = score[max(0,cy-band):min(b-t,cy+band+1)]
        cols = local.sum(axis=0)
        active = np.flatnonzero(cols > cols.max()*.22)
        if len(active) < 3:
            raise ValueError(f"{name}の幅を測定できません")
        left, right = l+int(active[0]), l+int(active[-1])+1
        center = t+cy
        half = max(3, round((right-left)*(.22 if name=='mouth' else .16)))
        boxes[name] = [left,center-half,right,center+half+1]
    if abs(sum(boxes['left_eye'][1::2])/2-sum(boxes['right_eye'][1::2])/2)>fh*.12:
        raise ValueError("左右の目の高さが一致しません。位置確認が必要です")
    if boxes['mouth'][1] <= max(boxes['left_eye'][1],boxes['right_eye'][1]):
        raise ValueError("口と目の上下関係が不正です")
    return boxes


def repair_patch(rgba, box, support=None, target=None, protected=None):
    """楕円内だけを周辺画素の調和補間で埋める。矩形の単色塗りをしない。"""
    l,t,r,b = box
    margin = max(3, round((r-l)*.15))
    vertical = max(2, round((r-l)*.08))
    l,t,r,b = max(0,l-margin),max(0,t-vertical),min(rgba.shape[1],r+margin),min(rgba.shape[0],b+vertical)
    crop = rgba[t:b,l:r].copy()
    h,w = crop.shape[:2]
    yy,xx = np.mgrid[:h,:w]
    mask = ((xx-(w-1)/2)/(w*.46))**2+((yy-(h-1)/2)/(h*.46))**2<1
    if target is not None:
        # 目口の実マスクだけを補完し、矩形内の髪を肌で消さない。
        mask=ndimage.binary_dilation(target[t:b,l:r],iterations=max(1,round((r-l)*.04)))
    if protected is not None:
        mask &= ~protected[t:b,l:r]
    mask[[0,-1],:]=False
    mask[:,[0,-1]]=False
    coords=np.argwhere(mask)
    if not len(coords):
        raise ValueError("表情補完の対象領域が空です")
    samples=crop[:,:,:3].astype(float)
    if support is not None:
        # 暗い髪・まつげを肌の境界条件へ流し込まない。周辺の実画素を使う。
        luminance=samples.mean(axis=2)
        valid=(~mask) & support[t:b,l:r] & (crop[:,:,3]>0)
        if protected is not None:
            valid &= ~protected[t:b,l:r]
        if valid.any():
            values=luminance[valid]
            median=np.median(values)
            cutoff=median-max(12,2.5*np.median(np.abs(values-median)))
            donors=valid & (luminance>=cutoff)
            nearest=ndimage.distance_transform_edt(~donors,return_distances=False,return_indices=True)
            samples=np.where((luminance<cutoff)[...,None],samples[tuple(nearest)],samples)
        else:
            raise ValueError("肌の補完に使える周辺画素がありません")
    ids=np.full(mask.shape,-1,int)
    ids[mask]=np.arange(len(coords))
    rows=[]; cols=[]; vals=[]
    rhs=np.zeros((len(coords),3))
    for i,(y,x) in enumerate(coords):
        rows.append(i);cols.append(i);vals.append(4.)
        for dy,dx in ((-1,0),(1,0),(0,-1),(0,1)):
            ny,nx=y+dy,x+dx
            if mask[ny,nx]:
                rows.append(i);cols.append(ids[ny,nx]);vals.append(-1.)
            else: rhs[i]+=samples[ny,nx]
    matrix=sparse.csr_matrix((vals,(rows,cols)),shape=(len(coords),len(coords)))
    crop[mask,:3]=np.clip(spsolve(matrix,rhs),0,255).astype(np.uint8)
    # 元画像と同じ境界画素を保持するので合成時の矩形境界が生じない。
    return crop,(l,t,r,b)


def expression_patch(rgba, box, kind, support=None, target=None, protected=None, with_parts=False):
    """目口の下地を補完し、原画の線色・唇色を使った局所差分を作る。"""
    crop,bounds=repair_patch(rgba,box,support,target,protected)
    l,t,r,b=bounds
    x0,y0,x1,y1=box
    source=rgba[y0:y1,x0:x1,:3]
    pixels=source.reshape(-1,3)
    dark=pixels[np.argsort(pixels.mean(axis=1))[:max(1,len(pixels)//8)]].mean(axis=0)
    image=Image.fromarray(crop)
    color=tuple(int(v) for v in dark)+(255,)
    if kind=='eye':
        from eyelids import close_eyelid
        closed,lash,aperture=close_eyelid(rgba,crop,bounds,box,target,protected,with_parts=True)
        image=Image.fromarray(closed)
    elif kind != 'mouth':
        raise ValueError(f"未対応の差分種別です: {kind}")
    result=np.zeros_like(rgba)
    result[t:b,l:r]=np.asarray(image)
    if with_parts:
        if kind!='eye':raise ValueError('目以外の素材分割は指定できません')
        base=np.zeros_like(rgba);base[t:b,l:r]=crop
        upper=np.zeros_like(rgba);upper[t:b,l:r]=lash
        return result,bounds,color,base,upper,aperture
    return result,bounds,color
