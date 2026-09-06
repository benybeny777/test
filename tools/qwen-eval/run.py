"""承認済みQwen二方式を同じ原寸ROIで比較する。正規素材へは採用しない。"""
import argparse
import json
import math
import os
from pathlib import Path
import socket
import subprocess
import sys
import time

import psutil
from PIL import Image

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'sidecar/expression'))
from generate import request_json,wait_for_server
from download import LOCK,digest

SOURCE_FILES={'source_sha256':'source/input.png','isolated_sha256':'source/isolated.png','analysis_sha256':'analysis/analysis.json','masks_sha256':'analysis/masks.npz'}


def source_hashes(character):
    return {key:digest(character/path) for key,path in SOURCE_FILES.items()}


def verify_source(character,recorded):
    current=source_hashes(character)
    if any(recorded.get(key)!=value for key,value in current.items()):
        raise ValueError('原画または解析マスクが比較開始時と一致しません')


def graph(mode,width,height,prompt,steps,seed,layers):
    if mode not in ('layered','edit'):raise ValueError('未承認の比較方式です')
    result=json.loads((ROOT/f'workflows/qwen-{mode}-api.json').read_text(encoding='utf-8'))
    result['9']['inputs'].update(width=width,height=height)
    result['10']['inputs'].update(steps=steps,seed=seed)
    if mode=='layered':
        result['14']['inputs']['text']=prompt
        result['9']['inputs']['layers']=layers
    else:
        result['7']['inputs']['prompt']=prompt
    return result


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


def prepare_input(character,output,resolution,view,head_framing='fixed'):
    with Image.open(character/'source/isolated.png') as opened:source=opened.convert('RGBA')
    if view=='head':
        metadata=json.loads((character/'analysis/analysis.json').read_text(encoding='utf-8'))
        face=metadata['analysis']['selected']['face']['box']
        neck=metadata['analysis']['selected']['neck']['box']
        width=min(source.width,resolution);height=min(source.height,resolution)
        left=max(0,min(source.width-width,round((face[0]+face[2]-width)/2)))
        top=max(0,min(source.height-height,round((face[1]+neck[3]-height)/2)))
        bounds=measured_head_region(face,neck,source.size,resolution) if head_framing=='measured' else (left,top,left+width,top+height)
        prepared=source.crop(bounds)
    else:
        bounds=(0,0,source.width,source.height)
        prepared=source.copy()
        # 比較専用の縮小。最終レイヤーへ戻す拡大はしない。
        prepared.thumbnail((resolution,resolution),Image.Resampling.LANCZOS)
    padded=Image.new('RGBA',((prepared.width+15)//16*16,(prepared.height+15)//16*16),(255,255,255,255))
    padded.alpha_composite(prepared)
    padded.convert('RGB').save(output/'input/input.png')
    prepared.save(output/'reference.png')
    return padded.size,{**source_hashes(character),'source_region':bounds,'source_size':source.size,'prepared_size':prepared.size,'view':view,'upscaled':False}


def resources(process):
    parent=psutil.Process(process.pid)
    members=[parent]+parent.children(recursive=True)
    rss=sum(p.memory_info().rss for p in members if p.is_running())/1e9
    gpu=subprocess.run(['nvidia-smi','--query-gpu=memory.used','--format=csv,noheader,nounits'],capture_output=True,text=True,check=True,creationflags=subprocess.CREATE_NO_WINDOW)
    return {'rss_gb':round(rss,3),'gpu_total_used_gb':round(float(gpu.stdout.strip().splitlines()[0])*1048576/1e9,3),'system_available_gb':round(psutil.virtual_memory().available/1e9,3)}


def stop_owned(process):
    if process.poll() is not None:return
    parent=psutil.Process(process.pid)
    members=parent.children(recursive=True)+[parent]
    # Windowsのvenvランチャーの実Python子も含め、取得したプロセス同一性をpsutilで検査して止める。
    for member in reversed(members):
        if member.is_running():
            try:member.terminate()
            except psutil.NoSuchProcess:pass
    _,alive=psutil.wait_procs(members,timeout=15)
    for member in alive:
        try:member.kill()
        except psutil.NoSuchProcess:pass
    psutil.wait_procs(alive,timeout=15)
    process.wait()


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode',choices=['layered','edit'])
    parser.add_argument('--character',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--comfy',type=Path,required=True)
    parser.add_argument('--view',choices=['head','full'],default='head')
    parser.add_argument('--head-framing',choices=['fixed','measured'],default='fixed')
    parser.add_argument('--resolution',type=int,default=1024)
    parser.add_argument('--steps',type=int,default=50)
    parser.add_argument('--seed',type=int,default=777)
    parser.add_argument('--layers',type=int,default=4)
    parser.add_argument('--port',type=int,default=58125)
    parser.add_argument('--timeout',type=int,default=14400)
    parser.add_argument('--fast-disk',action='store_true',help='非量子化のままNVMeからの動的読込を優先する')
    parser.add_argument('--prompt')
    args=parser.parse_args()
    output=args.output.resolve();comfy=args.comfy.resolve()
    if output.exists() or not output.is_relative_to(ROOT/'temp'):raise ValueError('新しいtemp内出力を指定してください')
    if not 256<=args.resolution<=1024 or not 1<=args.steps<=100 or not 1<=args.layers<=8:raise ValueError('比較条件が範囲外です')
    if args.port==8188 or not 1024<=args.port<=65535:raise ValueError('専用ポートを指定してください')
    with socket.socket() as probe:probe.bind(('127.0.0.1',args.port))
    if (comfy/'extra_model_paths.yaml').exists():raise ValueError('既存環境の追加モデル設定は利用しません')
    required={'qwen_2.5_vl_7b.safetensors',f'qwen_image_{"layered" if args.mode=="layered" else "edit_2511"}_bf16.safetensors',f'qwen_image_{"layered_" if args.mode=="layered" else ""}vae.safetensors'}
    for item in LOCK['files']:
        if Path(item['path']).name not in required:continue
        model=ROOT/'models/qwen-eval'/Path(item['path']).relative_to('split_files')
        print(json.dumps({'event':'checking_model','file':model.name}),flush=True)
        if digest(model)!=item['sha256']:raise ValueError(f'固定重みSHA不一致: {model.name}')
    for name in ['input','output','temp','user']:(output/name).mkdir(parents=True)
    size,source=prepare_input(args.character,output,args.resolution,args.view,args.head_framing)
    if args.head_framing!='fixed':source['head_framing']=args.head_framing
    prompt=args.prompt or ('A front-facing female character with hair, a face with eyes and mouth, ears, a neck, and clothing against a plain white background.' if args.mode=='layered' else 'Remove only the hair. Reconstruct the face, ears, neck and clothing that were hidden behind the hair. Preserve the exact existing facial features, expression, skin tone, clothing design, pose and rendering style. Keep a plain white background. Do not add objects or change the character identity.')
    workflow=graph(args.mode,*size,prompt,args.steps,args.seed,args.layers)
    (output/'workflow.json').write_text(json.dumps(workflow,indent=2),encoding='utf-8')
    report={'mode':args.mode,'source':source,'parameters':{'steps':args.steps,'seed':args.seed,'layers':args.layers,'prompt':prompt,'size':size},'quality':'unverified','product_adopted':False,'status':'running','resources':[]}
    command=[sys.executable,str(comfy/'main.py'),'--listen','127.0.0.1','--port',str(args.port),'--base-directory',str(output),'--models-directory',str(ROOT/'models/qwen-eval'),'--temp-directory',str(output/'temp'),'--disable-auto-launch','--disable-all-custom-nodes','--disable-api-nodes','--preview-method','none']
    if args.fast_disk:command.append('--fast-disk')
    report['command']=command
    environment=dict(os.environ,HF_HUB_OFFLINE='1',TRANSFORMERS_OFFLINE='1',HF_HUB_DISABLE_TELEMETRY='1')
    started=time.monotonic()
    with (output/'comfy.log').open('w',encoding='utf-8') as log:
        process=subprocess.Popen(command,cwd=comfy,env=environment,stdout=log,stderr=subprocess.STDOUT,creationflags=subprocess.CREATE_NO_WINDOW)
        print(json.dumps({'event':'starting','pid':process.pid,'output':str(output)}),flush=True)
        try:
            url=f'http://127.0.0.1:{args.port}';wait_for_server(url,process,600)
            report['system']=request_json(url+'/system_stats')
            queued=request_json(url+'/prompt',{'prompt':workflow})
            if 'prompt_id' not in queued:raise ValueError(f'ワークフローが拒否されました: {queued}')
            report['prompt_id']=queued['prompt_id'];generation_started=time.monotonic();last_print=0
            while time.monotonic()-generation_started<args.timeout:
                if process.poll() is not None:raise RuntimeError(f'ComfyUIが終了しました: {process.returncode}')
                sample={'seconds':round(time.monotonic()-generation_started,1),**resources(process)}
                report['resources'].append(sample)
                if sample['seconds']-last_print>=30:
                    print(json.dumps({'event':'generating',**sample}),flush=True);last_print=sample['seconds']
                history=request_json(url+'/history/'+queued['prompt_id'])
                if queued['prompt_id'] in history:
                    result=history[queued['prompt_id']]
                    if result.get('status',{}).get('status_str')=='error':raise RuntimeError(json.dumps(result['status'],ensure_ascii=False))
                    items=result.get('outputs',{}).get('13',{}).get('images')
                    if items:
                        expected=args.layers+1 if args.mode=='layered' else 1
                        if len(items)!=expected:raise ValueError('生成枚数が一致しません')
                        report['images']=[]
                        for item in items:
                            path=(output/'output'/item.get('subfolder','')/item['filename']).resolve()
                            if not path.is_relative_to(output/'output'):raise ValueError('出力パスが範囲外です')
                            with Image.open(path) as image:
                                if image.size!=size:raise ValueError('生成寸法が一致しません')
                                if args.mode=='layered' and image.mode!='RGBA':raise ValueError('RGBAレイヤーではありません')
                            report['images'].append(path.relative_to(output).as_posix())
                        verify_source(args.character,source)
                        report['status']='complete';break
                time.sleep(5)
            else:raise TimeoutError('生成の待機時間を超過しました')
        except BaseException as error:
            report['status']='failed';report['error']=str(error);raise
        finally:
            # Popenで保持した子だけを停止し、成功/失敗/中断の全経路で回収する。
            stop_owned(process)
            report['seconds']=round(time.monotonic()-started,2)
            (output/'report.json').write_text(json.dumps(report,indent=2,ensure_ascii=False),encoding='utf-8')
            print(json.dumps({'event':report['status'],'seconds':report['seconds'],'report':str(output/'report.json')}),flush=True)


if __name__=='__main__':
    sys.stdout.reconfigure(encoding='utf-8');sys.stderr.reconfigure(encoding='utf-8')
    main()
