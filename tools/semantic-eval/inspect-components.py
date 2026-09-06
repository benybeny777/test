"""正規の検出矩形でSAMの連結成分を比較する。診断だけで素材を採用しない。"""
import os
os.environ['HF_HUB_OFFLINE']='1'
os.environ['TRANSFORMERS_OFFLINE']='1'
import argparse
import json
import re
from pathlib import Path
import numpy as np
import torch
from PIL import Image
from scipy import ndimage
from transformers import Sam2Processor,Sam2VideoModel

root=Path(__file__).resolve().parents[2]
parser=argparse.ArgumentParser()
parser.add_argument('--characters',nargs='+',default=['c_2700e1166676','c_190454c86edb','c_828ead7c98ab'])
args=parser.parse_args()
processor=Sam2Processor.from_pretrained(root/'models/sam2.1-hiera-tiny',local_files_only=True)
model,info=Sam2VideoModel.from_pretrained(root/'models/sam2.1-hiera-tiny',local_files_only=True,output_loading_info=True)
if any(info[key] for key in ('missing_keys','unexpected_keys','mismatched_keys','error_msgs')):raise ValueError('SAM重みの不一致')
model=model.to('cuda').eval()
try:
    for cid in args.characters:
        if not re.fullmatch(r'[A-Za-z0-9_-]+',cid):raise ValueError('キャラIDが不正です')
        source=root/'temp/t7-characters'/cid
        metadata=json.loads((source/'analysis/analysis.json').read_text(encoding='utf-8'))
        box=metadata['analysis']['selected']['clothes']['box']
        image=Image.open(source/'source/isolated.png').convert('RGBA')
        rgb=Image.alpha_composite(Image.new('RGBA',image.size,(128,128,128,255)),image).convert('RGB')
        inputs=processor(images=rgb,input_boxes=[[box]],return_tensors='pt').to('cuda')
        with torch.inference_mode():prediction=model._single_frame_forward(**inputs)
        raw=processor.post_process_masks(prediction.pred_masks.cpu().unsqueeze(0),inputs['original_sizes'].cpu())[0].reshape(-1,image.height,image.width)[0].numpy().astype(bool)
        pixels=np.array(image);raw &= pixels[:,:,3]>=128
        labels,count=ndimage.label(raw);sizes=np.bincount(labels.ravel());sizes[0]=0
        if not count:raise ValueError('衣服のマスクが空です')
        order=np.argsort(sizes[1:])[::-1]+1
        output=root/'temp/clothing-components'/cid;output.mkdir(parents=True,exist_ok=True)
        combined=pixels.copy();combined[~raw]=0;Image.fromarray(combined).save(output/'all.png')
        records=[]
        for rank,index in enumerate(order):
            if rank>=5:break
            selected=labels==index
            part=pixels.copy();part[~selected]=0
            rendered=Image.fromarray(part);bounds=rendered.getbbox()
            rendered.crop(bounds).save(output/f'component-{rank}.png')
            records.append({'rank':rank,'pixels':int(sizes[index]),'bbox':bounds})
        report={'character':cid,'box':box,'components':count,'total_pixels':int(raw.sum()),'largest_pixels':int(sizes[order[0]]),'largest_ratio':float(sizes[order[0]]/raw.sum()),'top_components':records}
        (output/'report.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
        print(json.dumps(report),flush=True)
finally:
    del model,processor
    torch.cuda.empty_cache()
