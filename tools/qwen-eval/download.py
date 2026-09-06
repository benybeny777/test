"""承認済みQwen比較重みだけを取得し、固定SHA検査後に公開する。製品からは呼ばない。"""
import concurrent.futures
from collections import deque
import hashlib
import json
import os
from pathlib import Path
import time

import requests
import torch

ROOT=Path(__file__).resolve().parents[2]
LOCK=json.loads(Path(__file__).with_name('models.json').read_text(encoding='utf-8'))


def digest(path):
    with path.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()


def fetch_range(url,start,end):
    for attempt in range(3):
        try:
            with requests.get(url,headers={'Range':f'bytes={start}-{end}'},timeout=(30,120)) as response:
                response.raise_for_status()
                if response.status_code!=206 or not response.headers.get('Content-Range','').startswith(f'bytes {start}-{end}/'):
                    raise ValueError('Range応答が要求範囲と一致しません')
                if len(response.content)!=end-start+1:raise ValueError('Range応答の長さが一致しません')
                return response.content
        except requests.RequestException:
            if attempt==2:raise
            time.sleep(2*(attempt+1))


def download(item):
    relative=Path(item['path']).relative_to('split_files')
    target=ROOT/'models/qwen-eval'/relative
    target.parent.mkdir(parents=True,exist_ok=True)
    if target.exists():
        if digest(target)!=item['sha256']:raise ValueError(f'既存ファイルのSHA不一致: {target.name}')
        print(json.dumps({'event':'verified','file':target.name}),flush=True)
        return
    pending=ROOT/'temp/qwen-download'/f'{target.name}.part'
    pending.parent.mkdir(parents=True,exist_ok=True)
    offset=pending.stat().st_size if pending.exists() else 0
    url=f"https://huggingface.co/{item['repo']}/resolve/{item['revision']}/{item['path']}?download=true"
    with requests.head(url,allow_redirects=True,timeout=(30,120)) as response:
        response.raise_for_status()
        total=int(response.headers['Content-Length'])
        if offset>total:raise ValueError('partが取得対象より大きいため保持して停止します')
        started=last=time.monotonic()
        print(json.dumps({'event':'downloading','file':target.name,'total_gb':round(total/1e9,3),'resume_gb':round(offset/1e9,3)}),flush=True)
        ranges=iter((start,min(total-1,start+64*1024*1024-1)) for start in range(offset,total,64*1024*1024))
        # 同時に保持する応答を4個に制限し、巨大ファイル全体をRAMへ溜めない。
        with pending.open('ab' if offset else 'wb') as output, concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
            queue=deque(pool.submit(fetch_range,url,*bounds) for bounds in [next(ranges,None) for _ in range(4)] if bounds is not None)
            while queue:
                block=queue.popleft().result()
                output.write(block);output.flush();offset+=len(block)
                bounds=next(ranges,None)
                if bounds is not None:queue.append(pool.submit(fetch_range,url,*bounds))
                if time.monotonic()-last>=30:
                    print(json.dumps({'event':'progress','file':target.name,'gb':round(offset/1e9,3),'total_gb':round(total/1e9,3)}),flush=True)
                    last=time.monotonic()
            output.flush();os.fsync(output.fileno())
    if offset!=total or digest(pending)!=item['sha256']:raise ValueError(f'取得ファイルの整合性検査失敗: {target.name}')
    pending.replace(target)
    print(json.dumps({'event':'complete','file':target.name,'seconds':round(time.monotonic()-started,2),'sha256':item['sha256']}),flush=True)


if __name__=='__main__':
    if os.name!='nt' or not torch.cuda.is_available():raise RuntimeError('Windows/CUDAの比較対象機が必要です')
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:list(pool.map(download,LOCK['files']))
