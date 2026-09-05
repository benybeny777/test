"""承認済み比較候補をローカル原画へ適用し、推論結果と負荷を保存する。"""
import argparse,json,os,time,hashlib,threading,subprocess
from pathlib import Path
os.environ['HF_HUB_OFFLINE']='1'
os.environ['TRANSFORMERS_OFFLINE']='1'
os.environ['HF_MODULES_CACHE']=str(Path(__file__).resolve().parents[2]/'temp/hf-eval-modules')
import torch
from PIL import Image,ImageDraw
from transformers import AutoProcessor,AutoModelForZeroShotObjectDetection,AutoModelForCausalLM
root=Path(__file__).resolve().parents[2]
parser=argparse.ArgumentParser()
parser.add_argument('backend',choices=['florence','dino'])
parser.add_argument('--run-name',default='')
parser.add_argument('--characters',nargs='+',default=['c_2700e1166676','c_190454c86edb','c_828ead7c98ab'])
parser.add_argument('--labels',nargs='+',default=['face','eyes','mouth','neck','collar','hair','shirt','arms'])
parser.add_argument('--model-path',type=Path)
parser.add_argument('--view',choices=['both','full','head'],default='both')
parser.add_argument('--measured-head',action='store_true')
args=parser.parse_args()
name='Florence-2-large-ft' if args.backend=='florence' else 'grounding-dino-base'
path=args.model_path or root/'models/semantic-evaluation'/name
dest=(root/'temp/semantic-evaluation'/name/args.run_name).resolve()
if not dest.is_relative_to((root/'temp/semantic-evaluation').resolve()):raise ValueError('診断出力がtemp外です')
dest.mkdir(parents=True,exist_ok=True)
stop=threading.Event();gpu_samples=[]
def observe_gpu():
    while not stop.is_set():
        try:
            sample=subprocess.run(['nvidia-smi','--query-gpu=memory.used','--format=csv,noheader,nounits'],capture_output=True,text=True,check=True,timeout=5)
            gpu_samples.append(float(sample.stdout.splitlines()[0])*1024**2/1e9)
        except (OSError,ValueError,subprocess.SubprocessError) as error:
            print(f'GPU総量の観測失敗: {error}',flush=True)
            return
        stop.wait(.5)
observer=threading.Thread(target=observe_gpu,daemon=True);observer.start()
start=time.monotonic()
if args.backend=='florence':
    processor=AutoProcessor.from_pretrained(path,trust_remote_code=True,local_files_only=True)
    model,info=AutoModelForCausalLM.from_pretrained(path,trust_remote_code=True,local_files_only=True,dtype=torch.float16,attn_implementation='eager',output_loading_info=True)
else:
    processor=AutoProcessor.from_pretrained(path,local_files_only=True)
    model,info=AutoModelForZeroShotObjectDetection.from_pretrained(path,local_files_only=True,output_loading_info=True)
if info['missing_keys'] or info['mismatched_keys']:raise ValueError(f'重み未読込: {info}')
model=model.to('cuda').eval()
print(json.dumps({'loaded_seconds':time.monotonic()-start,'loading_info':info}),flush=True)
labels=args.labels
for cid in args.characters:
    source=root/'temp/t7-characters'/cid/'source/isolated.png'
    rgba=Image.open(source).convert('RGBA')
    rgb=Image.alpha_composite(Image.new('RGBA',rgba.size,(128,128,128,255)),rgba).convert('RGB')
    subject=rgba.getchannel('A').point(lambda x:255 if x>=128 else 0).getbbox()
    l,t,r,b=subject
    head=(l,t,r,round(t+(b-t)*.30))
    if args.measured_head:
        analysis=json.loads((source.parents[1]/'analysis/analysis.json').read_text(encoding='utf-8'))
        face=analysis['analysis']['selected']['face']['box']
        fl,ft,fr,fb=face;fw=fr-fl;fh=fb-ft
        head=(max(0,int(fl-fw*.3)),max(0,int(ft-fh*.3)),min(rgba.width,int(fr+fw*.3)),min(rgba.height,int(fb+fh*.3)))
    for view,box in [('full',(0,0,rgba.width,rgba.height)),('head',head)]:
        if args.view!='both' and args.view!=view:continue
        im=rgb.crop(box);records=[]
        torch.cuda.reset_peak_memory_stats();begin=time.monotonic()
        for label in labels:
            if args.backend=='florence':
                task='<CAPTION_TO_PHRASE_GROUNDING>'
                inputs=processor(text=task+label,images=im,return_tensors='pt').to('cuda',torch.float16)
                # 公式旧実装のタプルKVキャッシュと現行Transformersの形式差を避ける。
                # 重み・探索条件は保ち、診断ではキャッシュを無効化して実行する。
                with torch.inference_mode():out=model.generate(**inputs,max_new_tokens=256,num_beams=3,do_sample=False,use_cache=False)
                raw=processor.batch_decode(out,skip_special_tokens=False)[0]
                parsed=processor.post_process_generation(raw,task=task,image_size=im.size)[task]
                records.append({'query':label,'raw':raw,'boxes':parsed.get('bboxes',[]),'labels':parsed.get('labels',[])})
            else:
                inputs=processor(images=im,text=label+'.',return_tensors='pt').to('cuda')
                with torch.inference_mode():out=model(**inputs)
                parsed=processor.post_process_grounded_object_detection(out,inputs.input_ids,threshold=.20,text_threshold=.20,target_sizes=[(im.height,im.width)])[0]
                records.append({'query':label,'boxes':parsed['boxes'].cpu().tolist(),'scores':parsed['scores'].cpu().tolist(),'labels':parsed.get('text_labels',[])})
        torch.cuda.synchronize()
        report={'character':cid,'view':view,'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'crop':box,'seconds':time.monotonic()-begin,'peak_allocated_gb':torch.cuda.max_memory_allocated()/1e9,'peak_reserved_gb':torch.cuda.max_memory_reserved()/1e9,'records':records}
        (dest/f'{cid}-{view}.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8')
        drawn=im.copy();draw=ImageDraw.Draw(drawn)
        for i,record in enumerate(records):
            for bb in record['boxes']:
                draw.rectangle(bb,outline=['red','lime','cyan','orange','magenta','blue','yellow','white'][i%8],width=max(1,im.width//300))
                draw.text((bb[0],bb[1]),record['query'],fill='red')
        drawn.thumbnail((1100,1100));drawn.save(dest/f'{cid}-{view}.png')
        print(json.dumps({key:report[key] for key in ['character','view','seconds','peak_allocated_gb']},ensure_ascii=False),flush=True)
del model,processor
torch.cuda.empty_cache()
stop.set();observer.join(timeout=6)
(dest/'runtime.json').write_text(json.dumps({'nvidia_smi_peak_total_gb':max(gpu_samples) if gpu_samples else None,'samples':len(gpu_samples),'note':'GPU総量は他アプリの使用分を含む。モデル確保量とは別の値。'}),encoding='utf-8')
