"""通常生成の閉眼素材を原寸の局所領域から分離する。"""
import numpy as np
from scipy import ndimage

# 肌の明部推定と線縁の余白は、既存の曲線測定・補間と同じ基準を共有する。
SKIN_REFERENCE_PERCENTILE=90
INK_EDGE_PIXELS=2


def closed_curve(pixels,box,protected,aperture,measurements=None,line_mask=None):
    """閉眼の暗い連続まつげを測定し、元の開口列へ対応させる。"""
    l,t,r,b=box
    gray=pixels[t:b,l:r,:3].astype(float).mean(axis=2)
    available=~protected[t:b,l:r]
    if not available.any():raise ValueError('閉眼を測定する領域がありません')
    low,high=np.percentile(gray[available],[1,SKIN_REFERENCE_PERCENTILE])
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
    if line_mask.dtype!=bool or allowed.dtype!=bool:raise ValueError('閉眼線と許可域は真偽マスクが必要です')
    if not np.isfinite(edited).all():raise ValueError('閉眼肌に不正な画素があります')
    core=line_mask&allowed
    if not core.any():raise ValueError('除去する閉眼線がありません')
    # 主曲線は変更せず、測定した列厚さの近傍だけから薄い睫毛片を分離する。
    cy,cx=np.nonzero(core)
    thickness=np.median([np.ptp(cy[cx==x])+1 for x in np.unique(cx)])
    radius=max(INK_EDGE_PIXELS,int(np.ceil(thickness)))
    near=(ndimage.distance_transform_edt(~core)<=radius)&allowed
    background=ndimage.grey_closing(edited.astype(float),size=(radius*2+1,radius*2+1,1))
    difference=(background-edited).mean(axis=2)
    reference=ndimage.binary_dilation(near,iterations=radius)&allowed&~near
    if reference.sum()<4:raise ValueError('閉眼線の周囲にノイズ推定用の肌が不足しています')
    noise=float(np.percentile(difference[reference],SKIN_REFERENCE_PERCENTILE))
    # 1階調以下は画像量子化の差であり、暗線として採用しない。
    extra=near&(difference>max(noise,1))
    remove=ndimage.binary_dilation(core|extra,iterations=INK_EDGE_PIXELS)&allowed
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
    return clean,remove,{'removed_line_pixels':int((core|extra).sum()),'interpolated_pixels':int(remove.sum()),'line_brightening':brightening,
                         'extra_pixels':int((extra&~core).sum()),'radius':radius,'noise':noise}


def reconstruct_eye_skin(source, region, donors):
    """原画の開眼画素を意味解析済みの周辺肌から補間し、閉眼用の下地を作る。"""
    if source.ndim!=3 or source.shape[2]!=3 or region.shape!=source.shape[:2]:
        raise ValueError('閉眼下地の補間寸法が一致しません')
    if donors.shape!=region.shape:raise ValueError('閉眼下地の肌領域寸法が一致しません')
    if region.dtype!=bool or donors.dtype!=bool:raise ValueError('閉眼下地の領域は真偽マスクが必要です')
    if not np.isfinite(source).all():raise ValueError('閉眼下地に不正な画素があります')
    if not region.any():raise ValueError('閉眼下地の補間領域がありません')
    usable=donors&~region
    if usable.sum()<4:raise ValueError('閉眼下地の周囲に補間用の肌が不足しています')
    sigma=max(2.,np.sqrt(region.sum())/5.)
    weights=ndimage.gaussian_filter(usable.astype(float),sigma=sigma,mode='nearest')
    if np.any(weights[region]<1e-8):raise ValueError('閉眼下地の肌補間が成立しません')
    field=np.empty_like(source,dtype=float)
    for channel in range(3):
        values=ndimage.gaussian_filter(source[:,:,channel]*usable,sigma=sigma,mode='nearest')
        field[:,:,channel]=values/np.maximum(weights,1e-8)
    result=source.astype(float).copy();result[region]=np.clip(field[region],0,255)
    return result,{'filled_eye_pixels':int(region.sum()),'donor_skin_pixels':int(usable.sum()),'blend_sigma':float(sigma)}


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


def validate_masked_pixels(edited,original,mask):
    """マスク外の移動や描き直しを、色の丸め誤差以外は拒否する。"""
    if edited.shape[:2]!=original.shape[:2] or mask.shape!=edited.shape[:2]:raise ValueError('局所編集の検証寸法が一致しません')
    outside=mask==0
    if not outside.any():raise ValueError('マスク外の保護領域がありません')
    if np.any(np.abs(edited[:,:,:3].astype(np.int16)-original[:,:,:3].astype(np.int16))[outside]>1):
        raise ValueError('編集マスク外の画素が変わっています。構図不一致の候補を公開しません')
