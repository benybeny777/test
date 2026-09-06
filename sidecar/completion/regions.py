"""DINO/SAMの実測結果から原寸の編集領域を作る。"""
import math
import numpy as np
from scipy import ndimage

def eye_edit_mask(eyes,hair,alpha,margin_ratio):
    """目を中心に編集可能領域を作り、髪・透明背景への描き出しを抑える。"""
    if not 0<margin_ratio<=.5:raise ValueError('目の編集余白が範囲外です')
    region=np.zeros(alpha.shape,dtype=float)
    for eye in eyes:
        ys,xs=np.nonzero(eye)
        if not xs.size:raise ValueError('目の編集マスクが空です')
        margin=max(2,round((int(xs.max())-int(xs.min())+1)*margin_ratio))
        expanded=ndimage.binary_dilation(eye,iterations=margin)&~hair&(alpha>0)
        if not np.any(expanded&eye):raise ValueError('可視の目が編集範囲にありません')
        region=np.maximum(region,np.clip(ndimage.distance_transform_edt(expanded)/max(1,margin/2),0,1))
    return np.rint(region*255).astype(np.uint8)


def measured_head_region(face,neck,size,limit):
    """原画の顔と首の寸法から、拡縮しない頭部比較範囲を求める。"""
    if any(len(box)!=4 or not all(math.isfinite(v) for v in box) or box[2]<=box[0] or box[3]<=box[1] for box in (face,neck)):
        raise ValueError('頭部比較の実測座標が不正です')
    fw,fh=face[2]-face[0],face[3]-face[1]
    left=face[0]-.4*fw;right=face[2]+.4*fw
    top=face[1]-.55*fh;bottom=max(face[3],neck[3])+.15*fh
    extent=max(256,math.ceil(max(right-left,bottom-top)/16)*16)
    if extent>limit or extent>min(size):raise ValueError('原寸の頭部範囲が比較上限に収まりません。縮小せず条件を見直してください')
    x=max(0,min(size[0]-extent,round((left+right-extent)/2)))
    y=max(0,min(size[1]-extent,round((top+bottom-extent)/2)))
    return x,y,x+extent,y+extent
