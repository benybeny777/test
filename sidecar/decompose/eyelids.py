"""原画の上まつげを測定し、画素の質感を保った閉眼差分を作る。"""

import numpy as np
from scipy import ndimage


def partition_eye(eye, iris):
    """虹彩と相補領域を重複なしに分ける。隠れた白目の補完とは区別する。"""
    if eye.shape != iris.shape or eye.dtype != bool or iris.dtype != bool:
        raise ValueError('目と虹彩のマスク形式が一致しません')
    measured=eye & iris
    remainder=eye & ~measured
    if not measured.any() or not remainder.any():
        raise ValueError('虹彩と目の残りの領域を分離できません')
    return measured,remainder


def close_eyelid(rgba, clean, bounds, feature, target=None, protected=None, with_parts=False):
    """上まつげの画素を閉眼曲線へ移し、補完肌に合成する。"""
    left,top,right,bottom=feature
    pl,pt,pr,pb=bounds
    original=rgba[top:bottom,left:right]
    gray=original[:,:,:3].astype(float).mean(axis=2)
    height,width=gray.shape
    visible=original[:,:,3]>0
    if target is not None:
        visible &= target[top:bottom,left:right]
    if protected is not None:
        visible &= ~protected[top:bottom,left:right]
    contrast=np.maximum(0,np.percentile(gray,85)-gray)
    rows=np.full(width,np.nan)
    bands=[]
    for x in range(width):
        valid=np.flatnonzero(visible[:,x])
        if not len(valid):
            bands.append([]);continue
        first,last=int(valid[0]),int(valid[-1])
        candidates=valid[valid <= first+max(1,round((last-first)*.45))]
        center=int(candidates[np.argmax(contrast[candidates,x])])
        strength=contrast[center,x]
        if strength<1:
            bands.append([]);continue
        rows[x]=center
        radius=max(1,round(height*.10))
        band=[center]
        for direction in (-1,1):
            for step in range(1,radius+1):
                y=center+direction*step
                if not first<=y<=last or not visible[y,x] or contrast[y,x]<strength*.5:break
                band.append(y)
        bands.append([(y, min(1.,contrast[y,x]/strength)) for y in band])
    columns=np.flatnonzero(np.isfinite(rows))
    if len(columns)<2:
        raise ValueError("原画から上まつげを測定できません")
    first,last=int(columns[0]),int(columns[-1])
    # 抽出列ごとの局所ピークの揺れをならす。素材画像の拡大ではなく、実測輪郭の平滑化。
    # 黒目やまつげの局所ピークをそのまま動かすと半閉眼が波打つ。
    # 原画内で測定した点列へ低次曲線を当て、まぶた全体の弧を保持する。
    coordinates=(np.arange(width)-first)/max(1,last-first)
    coefficients=np.polynomial.polynomial.polyfit(coordinates[columns],rows[columns],min(2,len(columns)-1))
    rows=np.polynomial.polynomial.polyval(coordinates,coefficients)
    baseline=float(np.median(rows[columns]))+height*.45
    thickness=np.array([sum(alpha for _,alpha in band) for band in bands])
    thickness=ndimage.gaussian_filter1d(thickness,max(.85,width*.025))
    accumulated=np.zeros((*clean.shape[:2],3),float)
    coverage=np.zeros(clean.shape[:2],float)
    aperture=[]
    for x in columns:
        u=(x-first)/max(1,last-first)
        closed=baseline+np.sin(np.pi*u)*height*.12
        visible_rows=np.flatnonzero(visible[:,x])
        aperture.append([float(left+x+.5),float(top+rows[x]+.5),float(top+visible_rows[-1]+1),float(top+closed)])
        # 列ごとの切り出し端の段差を残さず、実測した線の面積と色を原寸で再構成する。
        mass=sum(alpha for _,alpha in bands[x])
        color=sum(original[y,x,:3]*alpha for y,alpha in bands[x])/mass
        center=top+closed-pt
        dx=left+x-pl
        radius=thickness[x]/2
        for dy in range(max(0,int(np.floor(center-radius))),min(clean.shape[0],int(np.ceil(center+radius))+1)):
            if protected is not None and protected[pt+dy,pl+dx]:continue
            if not clean[dy,dx,3]:continue
            value=float(np.clip(radius+.5-abs(dy+.5-center),0,1))
            accumulated[dy,dx]+=color*value
            coverage[dy,dx]+=value
    result=clean.copy()
    selected=coverage>0
    if not selected.any():raise ValueError("閉眼の上まつげが空になりました")
    alpha=np.minimum(1,coverage[selected])[:,None]
    color=accumulated[selected]/coverage[selected,None]
    result[selected,:3]=np.rint(result[selected,:3]*(1-alpha)+color*alpha).astype(np.uint8)
    if with_parts:
        lash=np.zeros_like(clean)
        lash[selected,:3]=np.rint(color).astype(np.uint8)
        lash[selected,3]=np.rint(alpha[:,0]*clean[selected,3]).astype(np.uint8)
        return result,lash,aperture
    return result
