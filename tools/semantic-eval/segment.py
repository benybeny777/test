"""意味領域の候補を既存SAM2へ渡す比較。製品リグへ自動昇格しない。"""
import os,json,time,argparse
from pathlib import Path
os.environ['HF_HUB_OFFLINE']='1'
os.environ['TRANSFORMERS_OFFLINE']='1'
import numpy as np
import torch
from PIL import Image
from scipy import ndimage
from transformers import Sam2VideoModel,Sam2Processor
root=Path(__file__).resolve().parents[2]
parser=argparse.ArgumentParser()
parser.add_argument('--backend',choices=['grounding-dino-base','Florence-2-large-ft'],default='grounding-dino-base')
parser.add_argument('--characters',nargs='+',default=['c_2700e1166676','c_190454c86edb','c_828ead7c98ab'])
parser.add_argument('--run-name',default='')
parser.add_argument('--roles',nargs='+',default=['face','eyes','mouth','neck','collar'])
parser.add_argument('--box-context',type=float,default=None)
args=parser.parse_args()
if args.box_context is not None and not 0<args.box_context<=2:raise ValueError('局所解析の余白は0より大きく2以下にしてください')
path=root/'models/sam2.1-hiera-tiny'
processor=Sam2Processor.from_pretrained(path,local_files_only=True)
model,info=Sam2VideoModel.from_pretrained(path,local_files_only=True,output_loading_info=True)
if any(info[key] for key in ('missing_keys','unexpected_keys','mismatched_keys','error_msgs')):raise ValueError(info)
model=model.to('cuda').eval()
print('SAM2動画クラスで重み不一致なし',flush=True)
for cid in args.characters:
    report=json.loads((root/'temp/semantic-evaluation'/args.backend/args.run_name/f'{cid}-head.json').read_text(encoding='utf-8'))
    rgba=Image.open(root/'temp/t7-characters'/cid/'source/isolated.png').convert('RGBA').crop(report['crop'])
    rgb=Image.alpha_composite(Image.new('RGBA',rgba.size,(128,128,128,255)),rgba).convert('RGB')
    face_record=next((item for item in report['records'] if item['query']=='face'),None)
    if face_record is not None:
        face=face_record['boxes'][int(np.argmax(face_record.get('scores',[1.0]*len(face_record['boxes']))))]
    else:
        saved=json.loads((root/'temp/t7-characters'/cid/'analysis/analysis.json').read_text(encoding='utf-8'))
        face=saved['analysis']['selected']['face']['box']
        face=[value-report['crop'][index%2] for index,value in enumerate(face)]
    fl,ft,fr,fb=face;fw=fr-fl;fh=fb-ft
    dest=(root/'temp/grounded-masks-v2'/args.backend/args.run_name/cid).resolve()
    if args.box_context is not None:dest=dest/f'box-context-{args.box_context:g}'
    if not dest.is_relative_to((root/'temp/grounded-masks-v2').resolve()):raise ValueError('診断出力がtemp外です')
    dest.mkdir(parents=True,exist_ok=True)
    result=[]
    for item in report['records']:
        role=item['query']
        if role not in args.roles:continue
        if not all(letter.isalnum() or letter in ' _-' for letter in role):raise ValueError('部位名が不正です')
        boxes=[]
        for box,score in zip(item['boxes'],item.get('scores',[None]*len(item['boxes']))):
            l,t,r,b=box;cx=(l+r)/2;cy=(t+b)/2
            if role in ['eyes','mouth','eyebrow','eye pupil'] and not (fl<=cx<=fr and ft<=cy<=fb and r-l<fw*.7 and b-t<fh*.45):continue
            if role=='neck' and not (fl<=cx<=fr and cy>ft+fh*.6 and r-l<fw*1.1 and b-t<fh):continue
            if role=='collar' and not (fl<=cx<=fr and t>ft+fh*.7 and fw*.5<r-l<fw*2.5):continue
            boxes.append((score,box))
        # 比較時は両モデルで同じ選別を使い、返却順・片方だけのスコアに依存しない。
        # 口の部分線だけを採らないよう、顔内の口候補は包含する大きい方を使う。
        boxes=sorted(boxes,key=lambda entry:(entry[1][2]-entry[1][0])*(entry[1][3]-entry[1][1]),reverse=role=='mouth')
        if role in ['eyebrow','eye pupil']:
            # 左右で最小の包含候補を選ぶ。両眼全体や全顔を細部として渡さない。
            groups=[[entry for entry in boxes if ((entry[1][0]+entry[1][2])/2<(fl+fr)/2)==left] for left in (True,False)]
            boxes=[group[0] for group in groups if group]
            if len(boxes)!=2:raise ValueError(f'{cid}: 左右の{role}を識別できません')
        else:boxes=boxes[:2 if role=='eyes' else 1]
        if role=='eyes':boxes.sort(key=lambda entry:entry[1][0])
        for index,(score,box) in enumerate(boxes):
            region=[0,0,rgba.width,rgba.height]
            if args.box_context is not None:
                l,t,r,b=box;mx=(r-l)*args.box_context;my=(b-t)*args.box_context
                region=[max(0,int(np.floor(l-mx))),max(0,int(np.floor(t-my))),min(rgba.width,int(np.ceil(r+mx))),min(rgba.height,int(np.ceil(b+my)))]
            sample=rgb.crop(region)
            local=[value-region[i%2] for i,value in enumerate(box)]
            inputs=processor(images=sample,input_boxes=[[local]],return_tensors='pt').to('cuda')
            begin=time.monotonic()
            with torch.inference_mode():pred=model._single_frame_forward(**inputs)
            # 動画クラスの単一フレーム出力へ画像バッチ軸を補い、画像プロセッサへ渡す。
            sampled=processor.post_process_masks(pred.pred_masks.cpu().unsqueeze(0),inputs['original_sizes'].cpu())[0].reshape(-1,sample.height,sample.width)
            masks=torch.zeros((sampled.shape[0],rgba.height,rgba.width),dtype=sampled.dtype)
            masks[:,region[1]:region[3],region[0]:region[2]]=sampled
            # 単一フレームAPIは予測IoU最大のマスク1枚を返す。
            scores=[float(pred.iou_scores.max().cpu())]
            for k,sam_score in enumerate(scores):
                a=np.array(rgba);mask=masks[k].numpy().astype(bool)&(a[:,:,3]>=128);a[~mask]=0
                output=Image.fromarray(a);output.save(dest/f'{role}-{index}-{k}.png')
                bounds=output.getbbox()
                if bounds:output.crop(bounds).save(dest/f'{role}-{index}-{k}-crop.png')
                # 比較用に小さな孤立島を分離する。原始マスクも残し、削除画素数を記録する。
                components,count=ndimage.label(mask)
                sizes=np.bincount(components.ravel());sizes[0]=0
                if count:
                    connected=components==sizes.argmax()
                    clean=np.array(rgba);clean[~connected]=0
                    Image.fromarray(clean).save(dest/f'{role}-{index}-{k}-connected.png')
                    result.append({'role':role,'removed_island_pixels':int(mask.sum()-connected.sum())})
            result.append({'role':role,'box':box,'sampling_region':region,'grounding_score':score,'sam_scores':scores,'seconds':time.monotonic()-begin})
    (dest/'report.json').write_text(json.dumps(result,indent=2),encoding='utf-8')
    print(json.dumps({'character':cid,'regions':sum('box' in item for item in result)}),flush=True)
