"""承認された比較モデルを公式固定リビジョンから専用配置へ取得する。"""
import hashlib,json,shutil
from pathlib import Path
from huggingface_hub import HfApi,hf_hub_download
ROOT=Path(__file__).resolve().parents[2]
import torch
if not torch.cuda.is_available():raise RuntimeError('この比較はCUDA実機向けです。取得前にGPUを確認してください')
SPECS=[('microsoft/Florence-2-large-ft','4a12a2b54b7016a48a22037fbd62da90cd566f2a'),('IDEA-Research/grounding-dino-base','12bdfa3120f3e7ec7b434d90674b3396eccf88eb')]
for repo,revision in SPECS:
    dest=ROOT/'models/semantic-evaluation'/repo.split('/')[-1]
    info=HfApi().model_info(repo,revision=revision,files_metadata=True)
    manifest={'repo':repo,'revision':revision,'files':[]}
    for entry in info.siblings:
        name=entry.rfilename
        if name.endswith('.bin') or name=='.gitattributes':continue
        print(f'取得: {repo}/{name}',flush=True)
        path=Path(hf_hub_download(repo,name,revision=revision,local_dir=dest))
        with path.open('rb') as file:digest=hashlib.file_digest(file,'sha256').hexdigest()
        if entry.lfs and digest!=entry.lfs.sha256:raise ValueError(f'ハッシュ不一致: {name}')
        manifest['files'].append({'name':name,'sha256':digest,'size_gb':path.stat().st_size/1e9})
    report=ROOT/'temp'/f'{dest.name}-download.json'
    report.write_text(json.dumps(manifest,indent=2),encoding='utf-8')
    cache=(dest/'.cache').resolve()
    if cache.is_dir() and cache.is_relative_to(dest.resolve()):
        shutil.rmtree(cache)
    print(f'検証完了: {repo}',flush=True)
