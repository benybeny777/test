"""承認済みローカルモデルで、原寸の目マスク内だけを閉眼編集する比較診断。"""
from pathlib import Path
import argparse
import hashlib
import json
import subprocess
import sys
import time

import numpy as np
from PIL import Image
from scipy import ndimage

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'sidecar/expression'))
from generate import prepare_workflow, request_json, wait_for_server, wait_for_result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--character',type=Path,required=True)
    parser.add_argument('--comfy',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--port',type=int,default=58123)
    parser.add_argument('--denoise',type=float,default=.85)
    parser.add_argument('--mask-grow',type=int,default=2)
    args=parser.parse_args()
    output=args.output.resolve()
    if not output.is_relative_to((ROOT/'temp').resolve()) or output.exists():
        raise ValueError('診断出力は未作成のtemp内ディレクトリを指定してください')
    if not 1024<=args.port<=65535 or args.port==8188:raise ValueError('専用ポートが必要です')
    if not 0<=args.denoise<=1 or not 0<=args.mask_grow<=16:raise ValueError('診断パラメータが範囲外です')
    comfy=args.comfy.resolve()
    if (comfy/'extra_model_paths.yaml').exists():
        raise ValueError('既存モデル設定を読むComfyUIでは比較しません')
    source_path=args.character/'source/isolated.png'
    with Image.open(source_path) as opened:source=opened.convert('RGBA')
    with np.load(args.character/'analysis/masks.npz',allow_pickle=False) as masks:
        target=masks['left_eye'] | masks['right_eye']
        if args.mask_grow:target=ndimage.binary_dilation(target,iterations=args.mask_grow)
        target &= ~masks['hair']
    if target.shape!=(source.height,source.width) or not target.any():raise ValueError('目のマスクが不正です')
    output.mkdir(parents=True)
    for name in ['input','output','temp','user']:(output/name).mkdir()
    # VAEの倍数へ余白だけを足す。画像を引き伸ばしたり縮小したりしない。
    size=((source.width+7)//8*8,(source.height+7)//8*8)
    conditioning=Image.new('RGB',size,'white');conditioning.paste(source,(0,0),source.getchannel('A'))
    conditioning.save(output/'input/neutral.png')
    mask=Image.new('L',size);mask.paste(Image.fromarray(target.astype(np.uint8)*255),(0,0))
    mask.convert('RGB').save(output/'input/mask.png')
    workflow=prepare_workflow(json.loads((ROOT/'workflows/expression-inpaint-api.json').read_text()),
                              '(closed eyes:1.5), both eyelids shut, relaxed eyelids','close',1000,args.denoise,0,'')
    workflow['5']['inputs']['text']+=', open eyes, visible iris, visible pupils, colorful eyelids'
    workflow['4']['inputs']['text']=workflow['4']['inputs']['text'].replace('eye color,','')+', natural dark eyelashes'
    (output/'workflow.json').write_text(json.dumps(workflow,indent=2),encoding='utf-8')
    checkpoint=comfy/'models/checkpoints/animagine-xl-4.0-opt.safetensors'
    with checkpoint.open('rb') as handle:checkpoint_hash=hashlib.file_digest(handle,'sha256').hexdigest()
    if checkpoint_hash!='6327eca98bfb6538dd7a4edce22484a1bbc57a8cff6b11d075d40da1afb847ac':
        raise ValueError('採用済みAnimagineの固定ハッシュと一致しません')
    command=[sys.executable,str(comfy/'main.py'),'--listen','127.0.0.1','--port',str(args.port),
             '--base-directory',str(output),'--models-directory',str(comfy/'models'),
             '--temp-directory',str(output/'temp'),'--disable-auto-launch','--disable-all-custom-nodes',
             '--disable-api-nodes','--lowvram','--preview-method','none']
    started=time.monotonic()
    with (output/'comfy.log').open('w',encoding='utf-8') as log:
        process=subprocess.Popen(command,cwd=comfy,stdout=log,stderr=subprocess.STDOUT)
        print(json.dumps({'event':'starting','pid':process.pid,'output':str(output)}),flush=True)
        try:
            url=f'http://127.0.0.1:{args.port}'
            wait_for_server(url,process,180)
            queued=request_json(url+'/prompt',{'prompt':workflow})
            print(json.dumps({'event':'generating','prompt_id':queued['prompt_id']}),flush=True)
            result=wait_for_result(url,queued['prompt_id'],900)
            item=result['outputs']['10']['images'][0]
            generated=(output/'output'/item.get('subfolder','')/item['filename']).resolve()
            if not generated.is_relative_to(output/'output'):raise ValueError('出力が診断領域外です')
            with Image.open(generated) as opened:
                if opened.size != size:raise ValueError('生成画像の寸法が入力と一致しません')
                edited=opened.convert('RGBA').crop((0,0,source.width,source.height))
            # VAE由来の変化や透過消失を外側へ持ち込まない。
            merged=np.asarray(source).copy();pixels=np.asarray(edited)
            alpha=np.minimum(1,ndimage.distance_transform_edt(target)/2)[target,None]
            merged[target,:3]=np.rint(merged[target,:3]*(1-alpha)+pixels[target,:3]*alpha).astype(np.uint8)
            Image.fromarray(merged).save(output/'candidate.png')
            changed=np.any(merged[:,:,:3]!=np.asarray(source)[:,:,:3],axis=2)
            if not changed.any() or (changed & ~target).any():raise ValueError('変更領域が不正です')
            report={'seconds':round(time.monotonic()-started,2),'changed_pixels':int(changed.sum()),
                    'checkpoint_sha256':checkpoint_hash,'canvas':list(source.size),
                    'denoise':args.denoise,'mask_grow':args.mask_grow,
                    'quality':'unverified','product_adopted':False}
            (output/'report.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
            print(json.dumps({'event':'complete',**report}),flush=True)
        finally:
            process.terminate()
            try:process.wait(timeout=15)
            except subprocess.TimeoutExpired:process.kill();process.wait()


if __name__=='__main__':main()
