"""見えている白目から虹彩の背後を補完する。原画の可視部分は変更しない。"""
import numpy as np
from scipy import ndimage, sparse
from scipy.sparse.linalg import spsolve


def eye_backplate(rgba, eye, iris):
    """虹彩の穴を調和補間する。まつげを含む残りの目と純粋な白目は区別する。"""
    if eye.shape != rgba.shape[:2] or iris.shape != eye.shape:
        raise ValueError('白目補完の入力寸法が一致しません')
    target=eye & iris
    remaining=eye & ~iris
    if not target.any() or not remaining.any():
        raise ValueError('虹彩または白目の参照領域がありません')
    ys,xs=np.nonzero(eye)
    left,right=max(0,int(xs.min())-1),min(rgba.shape[1],int(xs.max())+2)
    top,bottom=max(0,int(ys.min())-1),min(rgba.shape[0],int(ys.max())+2)
    rgb=rgba[top:bottom,left:right,:3].astype(float)
    mask=target[top:bottom,left:right]
    reference=remaining[top:bottom,left:right]
    brightness=rgb.mean(axis=2)-(rgb.max(axis=2)-rgb.min(axis=2))
    donors=reference & (brightness>=np.percentile(brightness[reference],75))
    nearest=ndimage.distance_transform_edt(~donors,return_distances=False,return_indices=True)
    samples=rgb[tuple(nearest)]
    coords=np.argwhere(mask);ids=np.full(mask.shape,-1,int);ids[mask]=np.arange(len(coords))
    rows=[];columns=[];values=[];rhs=np.zeros((len(coords),3))
    for index,(y,x) in enumerate(coords):
        rows.append(index);columns.append(index);values.append(4.)
        for dy,dx in ((-1,0),(1,0),(0,-1),(0,1)):
            ny,nx=y+dy,x+dx
            if 0<=ny<mask.shape[0] and 0<=nx<mask.shape[1] and mask[ny,nx]:
                rows.append(index);columns.append(ids[ny,nx]);values.append(-1.)
            else:
                rhs[index]+=samples[min(max(ny,0),mask.shape[0]-1),min(max(nx,0),mask.shape[1]-1)]
    solved=spsolve(sparse.csr_matrix((values,(rows,columns)),shape=(len(coords),len(coords))),rhs)
    if not np.isfinite(solved).all():raise ValueError('白目補完の解が不正です')
    result=rgba.copy();result[~eye]=0
    result[top:bottom,left:right,:3][mask]=np.rint(np.clip(solved,0,255)).astype(np.uint8)
    return result
