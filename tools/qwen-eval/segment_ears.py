"""承認済みDINO/SAMで比較画像の左右耳を原寸抽出する。色差で代用しない。"""
import argparse,gc,json,os,time,sys
from pathlib import Path
os.environ['HF_HUB_OFFLINE']='1'
os.environ['TRANSFORMERS_OFFLINE']='1'
import numpy as np
import torch
from PIL import Image
from transformers import AutoProcessor,AutoModelForZeroShotObjectDetection,Sam2Processor,Sam2VideoModel
from scipy import ndimage
from download import digest
from run import ROOT,verify_source
sys.path.insert(0,str(ROOT/'sidecar'))
from output_transaction import directory_output


def segment(directory,character,threshold=.2,context=.5,source_reference=False):
    if not 0<threshold<1 or not 0<context<=2:raise ValueError('検出閾値または解析余白が不正です')
    directory=directory.resolve();character=character.resolve()
    if not directory.is_relative_to(ROOT/'temp') or not character.is_relative_to(ROOT/'temp'):raise ValueError('診断入出力はtempに限定します')
    report=json.loads((directory/'report.json').read_text(encoding='utf-8'))
    if report['status']!='complete' or report['mode']!='edit':raise ValueError('完了した編集比較が必要です')
    verify_source(character,report['source'])
    source=(directory/'reference.png' if source_reference else directory/report['images'][0]).resolve()
    if not source.is_relative_to(directory):raise ValueError('比較画像が範囲外です')
    fingerprint=digest(source)
    with Image.open(source) as opened:image=opened.convert('RGB')
    destination=directory/('source-ears' if source_reference else 'ears')
    if destination.exists():raise ValueError('耳の診断出力は上書きしません')
    begin=time.monotonic();model_path=ROOT/'models/semantic-evaluation/grounding-dino-base'
    processor=AutoProcessor.from_pretrained(model_path,local_files_only=True)
    model,info=AutoModelForZeroShotObjectDetection.from_pretrained(model_path,local_files_only=True,output_loading_info=True)
    if info['missing_keys'] or info['mismatched_keys']:raise ValueError('DINOの固定重みが不完全です')
    model=model.to('cuda').eval();detections={}
    for label in ['face','ear']:
        inputs=processor(images=image,text=label+'.',return_tensors='pt').to('cuda')
        with torch.inference_mode():output=model(**inputs)
        result=processor.post_process_grounded_object_detection(output,inputs.input_ids,threshold=threshold,text_threshold=threshold,target_sizes=[(image.height,image.width)])[0]
        detections[label]=list(zip(result['boxes'].cpu().tolist(),result['scores'].cpu().tolist()))
    del model,processor,inputs,output,result;gc.collect();torch.cuda.empty_cache()
    if not detections['face']:raise ValueError('比較画像の顔を検出できません')
    face=max(detections['face'],key=lambda pair:pair[1])[0];cx=(face[0]+face[2])/2
    boxes=[]
    for left in [True,False]:
        candidates=[(box,score) for box,score in detections['ear'] if ((box[0]+box[2])/2<cx)==left and box[2]-box[0]<(face[2]-face[0])*.6]
        if not candidates:
            if source_reference:boxes.append(None);continue
            raise ValueError('左右の耳を検出できません。色や固定座標で補いません')
        boxes.append(max(candidates,key=lambda pair:pair[1]))
    if not any(boxes):raise ValueError('可視の耳を検出できません')
    print(json.dumps({'ear_boxes':boxes}),flush=True)
    sam_path=ROOT/'models/sam2.1-hiera-tiny'
    processor=Sam2Processor.from_pretrained(sam_path,local_files_only=True)
    model,info=Sam2VideoModel.from_pretrained(sam_path,local_files_only=True,output_loading_info=True)
    if any(info[key] for key in ('missing_keys','unexpected_keys','mismatched_keys','error_msgs')):raise ValueError('SAMの固定重みが不完全です')
    model=model.to('cuda').eval();masks=[]
    for entry in boxes:
        if entry is None:masks.append(np.zeros((image.height,image.width),bool));continue
        box,score=entry
        l,t,r,b=box;mx=(r-l)*context;my=(b-t)*context
        region=[max(0,int(l-mx)),max(0,int(t-my)),min(image.width,int(np.ceil(r+mx))),min(image.height,int(np.ceil(b+my)))]
        sample=image.crop(region);local=[v-region[i%2] for i,v in enumerate(box)]
        inputs=processor(images=sample,input_boxes=[[local]],return_tensors='pt').to('cuda')
        with torch.inference_mode():output=model._single_frame_forward(**inputs)
        raw=processor.post_process_masks(output.pred_masks.cpu().unsqueeze(0),inputs['original_sizes'].cpu())[0].reshape(-1,sample.height,sample.width)[0].numpy().astype(bool)
        components,count=ndimage.label(raw)
        if not count:raise ValueError('耳マスクが空です')
        sizes=np.bincount(components.ravel());sizes[0]=0;raw=components==sizes.argmax()
        mask=np.zeros((image.height,image.width),bool);mask[region[1]:region[3],region[0]:region[2]]=raw;masks.append(mask)
    del model,processor,inputs,output;gc.collect();torch.cuda.empty_cache()
    with directory_output(destination) as pending:
        np.savez_compressed(pending/'masks.npz',left=masks[0],right=masks[1])
        for side,mask in zip(['left','right'],masks):
            rgba=np.array(image.convert('RGBA'));rgba[:,:,3]=mask*255;Image.fromarray(rgba).save(pending/f'{side}.png')
        (pending/'report.json').write_text(json.dumps({'status':'complete','source_sha256':fingerprint,'original_source':report['source'],'source_reference':source_reference,'boxes':boxes,'detections':detections,'threshold':threshold,'context':context,'seconds':time.monotonic()-begin},ensure_ascii=False,indent=2),encoding='utf-8')
        verify_source(character,report['source'])
        if digest(source)!=fingerprint:raise ValueError('解析中に比較画像が変わりました')
    print(json.dumps({'directory':str(destination),'seconds':time.monotonic()-begin}),flush=True)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('directory',type=Path);parser.add_argument('--character',type=Path,required=True)
    parser.add_argument('--threshold',type=float,default=.2);parser.add_argument('--context',type=float,default=.5)
    parser.add_argument('--source-reference',action='store_true',help='原画側の可視耳も解析する。隠れて検出されない側は欠測として記録する')
    args=parser.parse_args();segment(args.directory,args.character,args.threshold,args.context,args.source_reference)
