"""Grounding DINOの意味領域からSAM2の原寸部位マスクを生成する。"""
import gc
import os
import hashlib
import json
import importlib.metadata
from pathlib import Path
import numpy as np
from PIL import Image
from scipy import ndimage


def _digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream,'sha256').hexdigest()


def analyse_cached(image, detector_path, sam_path, threshold, emit, directory, eye_context_margin):
    """意味解析と素材補完を分離し、同じ解析をGPUで繰り返さない。"""
    directory=Path(directory)
    identity={'version':3,'eye_context_margin':eye_context_margin,'image':hashlib.sha256(image.tobytes()).hexdigest(),
              'size':image.size,'threshold':threshold,'code':_digest(__file__),
              'transformers':importlib.metadata.version('transformers'),
              'torch':importlib.metadata.version('torch'),
              'optional_limbs_code':{name:_digest(Path(__file__).with_name(name)) for name in ('limbs.py','optional_limbs.py')},
              'models':{}}
    for label,path in [('detector',detector_path),('sam',sam_path)]:
        if not path.is_dir():
            raise ValueError(f'解析モデルの保存先がありません: {path}')
        identity['models'][label]={item.name:_digest(item) for item in sorted(path.iterdir())
                                  if item.is_file() and item.suffix in ('.json','.txt','.safetensors')}
    signature=hashlib.sha256(json.dumps(identity,sort_keys=True).encode()).hexdigest()
    index=directory/'analysis.json';archive=directory/'masks.npz'
    if index.is_file() and archive.is_file():
        saved=json.loads(index.read_text(encoding='utf-8'))
        if saved['signature']==signature and saved['masks_sha256']==_digest(archive):
            with np.load(archive,allow_pickle=False) as stored:
                masks={name:stored[name].astype(bool) for name in stored.files}
            if any(mask.shape!=(image.height,image.width) for mask in masks.values()):
                raise ValueError('解析キャッシュの寸法が不正です')
            emit('progress',stage='analysis_cache',progress=.8)
            return masks,saved['features'],saved['analysis']
    result=analyse(image,detector_path,sam_path,threshold,emit,eye_context_margin)
    directory.mkdir(parents=True,exist_ok=True)
    staging=directory/'masks.npz.part'
    with staging.open('wb') as stream:
        np.savez_compressed(stream,**result[0]);stream.flush();os.fsync(stream.fileno())
    os.replace(staging,archive)
    data={'signature':signature,'identity':identity,'masks_sha256':_digest(archive),
          'features':result[1],'analysis':result[2]}
    staging=directory/'analysis.json.part'
    with staging.open('w',encoding='utf-8') as stream:
        json.dump(data,stream,ensure_ascii=False);stream.flush();os.fsync(stream.fileno())
    os.replace(staging,index)
    return result


def select_boxes(records, role, face=None):
    """包含・接続関係で候補を限定する。未検出は座標で補わない。"""
    candidates=records.get(role, [])
    if face is not None:
        fl,ft,fr,fb=face;fw=fr-fl;fh=fb-ft
        def valid(candidate):
            l,t,r,b=candidate['box'];cx=(l+r)/2;cy=(t+b)/2
            if role in ('eyes','mouth','pupils'):
                return fl<=cx<=fr and ft<=cy<=fb and r-l<fw*.7 and b-t<fh*.45
            if role=='neck':
                return fl<=cx<=fr and cy>ft+fh*.6 and r-l<fw*1.1 and b-t<fh
            if role=='collar':
                return fl<=cx<=fr and t>ft+fh*.7 and fw*.5<r-l<fw*2.5
            return True
        candidates=[item for item in candidates if valid(item)]
    if not candidates:return []
    if role=='collar':
        # 襟の一部分だけの低得点候補を最小面積という理由で選ばない。
        # 同点は面積降順・座標昇順とし、検出結果の返却順に依存させない。
        return [min(candidates,key=lambda item:(-item['score'],
                    -(item['box'][2]-item['box'][0])*(item['box'][3]-item['box'][1]),tuple(item['box'])))]
    if role in ('face','hair','clothes'):
        return [max(candidates,key=lambda item:item['score'])]
    candidates=sorted(candidates,key=lambda item:(item['box'][2]-item['box'][0])*(item['box'][3]-item['box'][1]),reverse=role=='mouth')
    if role in ('eyes','arms','pupils') and face is not None:
        center=(face[0]+face[2])/2
        groups=[[item for item in candidates if ((item['box'][0]+item['box'][2])/2<center)==left]
                for left in (True,False)]
        return [group[0] for group in groups if group]
    chosen=candidates[:2 if role in ('eyes','arms') else 1]
    if role in ('eyes','arms'):chosen.sort(key=lambda item:item['box'][0])
    return chosen


def iris_white_points(rgba, eye):
    """検出された目の左右端から白目候補を測る。原画の固定座標は使わない。"""
    ys,xs=np.nonzero(eye)
    if not len(xs):raise ValueError('白目の参照領域が空です')
    left,right=xs.min(),xs.max();top,bottom=ys.min(),ys.max()
    yy,xx=np.indices(eye.shape)
    middle=(yy>=top+(bottom-top)*.35)&(yy<=top+(bottom-top)*.75)
    rgb=rgba[:,:,:3].astype(float)
    whiteness=rgb.mean(axis=2)-(rgb.max(axis=2)-rgb.min(axis=2))
    points=[]
    for region in (xx<=left+(right-left)*.25,xx>=right-(right-left)*.25):
        selected=eye & middle & region
        if not selected.any():raise ValueError('左右の白目参照点を測定できません')
        y,x=np.unravel_index(np.where(selected,whiteness,-np.inf).argmax(),eye.shape)
        points.append([float(x),float(y)])
    return points


def coarse_pupil_box(pupil, eye):
    """虹彩候補が目全体の大部分を占める場合だけ追加の絞り込み対象にする。"""
    def area(box):return max(0,box[2]-box[0])*max(0,box[3]-box[1])
    return area(pupil)>=area(eye)*.8


def eye_context(box,size,margin):
    """原寸の目周辺を局所解析する。検出矩形以外の固定座標へ降格しない。"""
    if not np.isfinite(margin) or not .1<=margin<=2:raise ValueError('目の解析余白が範囲外です')
    l,t,r,b=box
    if not all(np.isfinite(box)) or not 0<=l<r<=size[0] or not 0<=t<b<=size[1]:
        raise ValueError('目の検出矩形が原画範囲外です')
    mx=(r-l)*margin;my=(b-t)*margin
    return [max(0,int(np.floor(l-mx))),max(0,int(np.floor(t-my))),min(size[0],int(np.ceil(r+mx))),min(size[1],int(np.ceil(b+my)))]


def semantic_components(mask,role):
    """衣服は離れた複数部品を持つ。単一物体の島除去を流用しない。"""
    labels,count=ndimage.label(mask)
    if not count:raise ValueError(f'部位のマスクが空です: {role}')
    sizes=np.bincount(labels.ravel());sizes[0]=0
    retained=mask.copy() if role=='clothes' else labels==sizes.argmax()
    return retained,{'components_count':count,'component_policy':'all_clothing' if role=='clothes' else 'largest_connected',
                     'discarded_pixels':int(mask.sum()-retained.sum())}


def analyse_optional_limbs(records,masks,sample):
    """通常SAMと同じインスタンスで全袖/手候補を採取し、意味所有で選別する。"""
    from optional_limbs import select
    candidates=[];observations=[]
    for role in ('sleeve','hand'):
        # 信頼度同点の候補も座標順に固定し、検出の返却順で採否が変わらないようにする。
        items=sorted(records.get(role,[]),key=lambda item:(-item['score'],tuple(item['box'])))
        for index,item in enumerate(items):
            mask,metadata=sample(role,item)
            key=f'{role}-{index:03d}'
            candidates.append({'id':key,'role':role,'score':item['score'],'mask':mask})
            observations.append({'id':key,'box':item['box'],'detector_score':item['score'],**metadata})
    selected,report=select(candidates,masks)
    report['sampling']=observations
    report['queries']={role:'detected' if records.get(role) else 'not_detected' for role in ('sleeve','hand')}
    return selected,report


def analyse(image, detector_path, sam_path, threshold, emit, eye_context_margin):
    """2モデルを逐次ロードし、画像と同じ座標系の意味情報を返す。"""
    os.environ['HF_HUB_OFFLINE']='1';os.environ['TRANSFORMERS_OFFLINE']='1'
    import torch
    from transformers import AutoProcessor,AutoModelForZeroShotObjectDetection,Sam2VideoModel,Sam2Processor
    if not torch.cuda.is_available():raise RuntimeError('部位解析にはCUDA対応GPUが必要です')
    if not detector_path.is_dir():raise ValueError('Grounding DINOがありません。cargo xtask setup grounding を実行してください')
    rgba=np.asarray(image);subject=rgba[:,:,3]>=128
    ys,xs=np.nonzero(subject)
    if not len(xs):raise ValueError('前景が空です')
    l,t,r,b=int(xs.min()),int(ys.min()),int(xs.max()+1),int(ys.max()+1)
    rgb=Image.alpha_composite(Image.new('RGBA',image.size,(128,128,128,255)),image).convert('RGB')
    crops={'full':(0,0,image.width,image.height)}
    records={}
    processor=AutoProcessor.from_pretrained(detector_path,local_files_only=True)
    detector=AutoModelForZeroShotObjectDetection.from_pretrained(detector_path,local_files_only=True).to('cuda').eval()
    try:
        queries=('face','eyes','mouth','neck','collar','hair','clothes','arms','pupils','sleeve','hand')
        for index,role in enumerate(queries):
            crop=crops['detail' if role=='pupils' else 'full' if role in ('face','clothes','arms','sleeve','hand') else 'head']
            im=rgb.crop(crop)
            inputs=processor(images=im,text=('eye pupil' if role=='pupils' else role)+'.',return_tensors='pt').to('cuda')
            with torch.inference_mode():prediction=detector(**inputs)
            found=processor.post_process_grounded_object_detection(prediction,inputs.input_ids,threshold=threshold,text_threshold=threshold,target_sizes=[(im.height,im.width)])[0]
            records[role]=[{'box':[bb[0]+crop[0],bb[1]+crop[1],bb[2]+crop[0],bb[3]+crop[1]],'score':float(score)} for bb,score in zip(found['boxes'].cpu().tolist(),found['scores'].cpu().tolist())]
            if role=='face':
                faces=select_boxes(records,'face')
                if not faces:raise ValueError('全体画像から顔を検出できません')
                fl,ft,fr,fb=faces[0]['box'];fw=fr-fl;fh=fb-ft
                # 全身比率ではなく、最初に検出した顔を基準に細部の解析範囲を取る。
                crops['head']=(max(0,int(fl-fw)),max(0,int(ft-fh)),
                               min(image.width,int(np.ceil(fr+fw))),min(image.height,int(np.ceil(fb+fh))))
                crops['detail']=(max(0,int(fl-fw*.3)),max(0,int(ft-fh*.3)),
                                 min(image.width,int(fr+fw*.3)),min(image.height,int(fb+fh*.3)))
            emit('progress',stage='grounding',progress=.1+.3*(index+1)/len(queries))
    finally:
        del detector,processor
        gc.collect();torch.cuda.empty_cache()
    faces=select_boxes(records,'face')
    if not faces:raise ValueError('顔の意味領域が検出されませんでした')
    face=faces[0]['box'];chosen={}
    for role in records:
        if role in ('sleeve','hand'):continue
        boxes=select_boxes(records,role,face)
        if role in ('face','eyes','mouth','neck','hair','pupils') and len(boxes)!=(2 if role in ('eyes','pupils') else 1):
            raise ValueError(f'必須の意味領域を確定できません: {role}')
        for index,item in enumerate(boxes):
            key=('left_' if index==0 else 'right_')+('eye' if role=='eyes' else 'arm') if role in ('eyes','arms') else role
            if role=='pupils':key=('left_' if index==0 else 'right_')+'eye_iris'
            chosen[key]=item
    processor=Sam2Processor.from_pretrained(sam_path,local_files_only=True)
    model,info=Sam2VideoModel.from_pretrained(sam_path,local_files_only=True,output_loading_info=True)
    if any(info[key] for key in ('missing_keys','unexpected_keys','mismatched_keys','error_msgs')):raise ValueError('SAM2の重みと実装が一致しません')
    model=model.to('cuda').eval();masks={}
    try:
        for index,(role,item) in enumerate(chosen.items()):
            crop=crops['detail' if role.endswith('_iris') else 'full' if role in ('clothes','left_arm','right_arm') else 'head']
            if role in ('left_eye','right_eye'):
                crop=eye_context(item['box'],rgb.size,eye_context_margin)
                item['sampling_region']=crop
            im=rgb.crop(crop);box=item['box'];local=[box[0]-crop[0],box[1]-crop[1],box[2]-crop[0],box[3]-crop[1]]
            prompts={'input_boxes':[[local]]}
            if role=='hair':
                # 髪の矩形だけでは頭全体が選ばれるため、検出済みの目口を明示的に除外する。
                # 4.57.6のbox+points同時指定はnum_objects未初期化になる。
                # 公式VideoProcessorと同じ角ラベル2/3へ変換し、複数候補推論を維持する。
                points=[local[:2],local[2:]]
                for feature in ('left_eye','right_eye','mouth'):
                    fl,ft,fr,fb=chosen[feature]['box']
                    points.append([(fl+fr)/2-crop[0],(ft+fb)/2-crop[1]])
                prompts={'input_points':[[points]],'input_labels':[[[2,3]+[0]*(len(points)-2)]]}
                item['negative_features']=['left_eye','right_eye','mouth']
            if role.endswith('_iris') and coarse_pupil_box(box,chosen[role.replace('_iris','')]['box']):
                # 細かく検出済みの虹彩へ白い点の除外を追加するとハイライトを消す。
                # 目全体に近い粗い候補だけを対象にし、キャラ名では分岐しない。
                eye=masks[role.replace('_iris','')]
                white=iris_white_points(rgba,eye)
                points=[local[:2],local[2:],[(local[0]+local[2])/2,(local[1]+local[3])/2]]
                points.extend([[x-crop[0],y-crop[1]] for x,y in white])
                prompts={'input_points':[[points]],'input_labels':[[[2,3,1,0,0]]]}
                item['white_exclusion_points']=white
            inputs=processor(images=im,return_tensors='pt',**prompts).to('cuda')
            with torch.inference_mode():prediction=model._single_frame_forward(**inputs)
            small=processor.post_process_masks(prediction.pred_masks.cpu().unsqueeze(0),inputs['original_sizes'].cpu())[0].reshape(-1,im.height,im.width)[0].numpy().astype(bool)
            mask=np.zeros(subject.shape,bool);mask[crop[1]:crop[3],crop[0]:crop[2]]=small;mask &= subject
            masks[role],components=semantic_components(mask,role)
            item.update(components)
            item['sam_score']=float(prediction.iou_scores.max().cpu())
            emit('progress',stage='grounded_sam',progress=.4+.3*(index+1)/len(chosen))
        def sample_optional(role,item):
            # 単一maskに対してIoU配列は複数のまま返る。配列を保ち、複数mask保存と称しない。
            optional_inputs=processor(images=rgb,input_boxes=[[item['box']]],return_tensors='pt').to('cuda')
            try:
                with torch.inference_mode():optional_prediction=model._single_frame_forward(**optional_inputs)
                result=processor.post_process_masks(optional_prediction.pred_masks.cpu().unsqueeze(0),optional_inputs['original_sizes'].cpu())[0]
                mask=result.reshape(-1,image.height,image.width)[0].numpy().astype(bool)&subject
                return mask,{'sam_iou_scores':optional_prediction.iou_scores.detach().cpu().tolist(),
                             'mask_policy':'selected_best_mask','pixels':int(mask.sum()),'sampling_region':list(crops['full'])}
            finally:
                optional_inputs=None
        optional_masks,optional_status=analyse_optional_limbs(records,masks,sample_optional)
        masks.update(optional_masks)
        emit('progress',stage='grounded_optional_limbs',progress=.8)
    finally:
        del model,processor
        gc.collect();torch.cuda.empty_cache()
    features={name:[int(np.floor(v)) if i<2 else int(np.ceil(v)) for i,v in enumerate(chosen[name]['box'])] for name in ('left_eye','right_eye','mouth')}
    for box in features.values():
        box[0]=max(0,box[0]);box[1]=max(0,box[1]);box[2]=min(image.width,box[2]);box[3]=min(image.height,box[3])
    return masks,features,{'schema_version':1,'candidates':records,'selected':chosen,'optional_limbs':optional_status,
                          'method':'grounding-dino-base+sam2.1','visual_status':'unverified'}
