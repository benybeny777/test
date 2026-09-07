"""採用済み閉眼補完の3モデルだけを、固定SHA確認後に管理領域へ配置する。"""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import time

import requests

ROOT = Path(__file__).resolve().parents[1]
LOCK = ROOT/'sidecar/completion/models.json'
EXPECTED = {
    'text_encoders/qwen_2.5_vl_7b.safetensors',
    'vae/qwen_image_vae.safetensors',
    'diffusion_models/qwen_image_edit_2511_bf16.safetensors',
}


def digest(path):
    with path.open('rb') as handle:
        return hashlib.file_digest(handle, 'sha256').hexdigest()


def entries():
    """固定ロック以外のリポジトリ・パス・重みを取得しない。"""
    files = json.loads(LOCK.read_text(encoding='utf-8'))['files']
    if len(files) != 3:
        raise ValueError('閉眼補完の固定モデル数は3件です')
    result = []
    for item in files:
        path = PurePosixPath(item['path'])
        if path.is_absolute() or '..' in path.parts or '\\' in item['path']:
            raise ValueError('モデルの相対パスが不正です')
        relative = path.relative_to('split_files').as_posix()
        if relative not in EXPECTED or item['repo'] not in ('Comfy-Org/Qwen-Image_ComfyUI', 'Comfy-Org/Qwen-Image-Edit_ComfyUI'):
            raise ValueError('採用していないモデルの取得を拒否しました')
        if not re.fullmatch('[0-9a-f]{40}', item['revision']) or not re.fullmatch('[0-9a-f]{64}', item['sha256']):
            raise ValueError('モデルの固定revisionまたはSHAが不正です')
        result.append((item, relative))
    if {relative for _, relative in result} != EXPECTED:
        raise ValueError('補完モデルが重複または欠落しています')
    return result


def download(item, relative):
    destination = ROOT/'models/qwen-eval'/relative
    pending = ROOT/'temp/completion-download'/(destination.name+'.part')
    for path in (destination, pending):
        if path.absolute() != path.resolve():
            raise ValueError('モデル保存先にリンクを使用できません')
        path.parent.mkdir(parents=True, exist_ok=True)
    if destination.exists():
        if digest(destination) != item['sha256']:
            raise ValueError(f'既存モデルのSHA不一致。上書きしません: {destination.name}')
        # 成功した同じモデルの再開残骸だけを片付ける。
        if pending.exists():
            pending.unlink()
        print(json.dumps({'event': 'verified', 'file': destination.name}), flush=True)
        return
    url = f"https://huggingface.co/{item['repo']}/resolve/{item['revision']}/{item['path']}?download=true"
    offset = pending.stat().st_size if pending.exists() else 0
    with requests.head(url, allow_redirects=True, timeout=(30, 120)) as response:
        response.raise_for_status()
        total = int(response.headers['Content-Length'])
    if total <= 0 or offset > total:
        raise ValueError('再開ファイルの長さが不正です。削除せず停止します')
    print(json.dumps({'event': 'downloading', 'file': destination.name,
                      'total_gb': round(total/1e9, 3), 'resume_gb': round(offset/1e9, 3)}), flush=True)
    if offset < total:
        with requests.get(url, headers={'Range': f'bytes={offset}-{total-1}'}, stream=True, timeout=(30, 120)) as response:
            response.raise_for_status()
            if response.status_code != 206 or response.headers.get('Content-Range') != f'bytes {offset}-{total-1}/{total}':
                raise ValueError('取得応答の範囲が再開位置と一致しません')
            last = time.monotonic()
            with pending.open('ab' if offset else 'wb') as output:
                for block in response.iter_content(chunk_size=1024*1024):
                    if not block:
                        continue
                    if offset+len(block) > total:
                        raise ValueError('取得応答がモデルの指定長を超えています')
                    output.write(block); offset += len(block)
                    if time.monotonic()-last >= 30:
                        print(json.dumps({'event': 'progress', 'file': destination.name,
                                          'gb': round(offset/1e9, 3), 'total_gb': round(total/1e9, 3)}), flush=True)
                        last = time.monotonic()
                output.flush(); os.fsync(output.fileno())
    # 取得済みpartからの再開でも、公開前の同期を省略しない。
    with pending.open('r+b') as output:
        os.fsync(output.fileno())
    if offset != total or digest(pending) != item['sha256']:
        raise ValueError(f'取得モデルの整合性検査失敗。partを保持します: {destination.name}')
    pending.replace(destination)
    print(json.dumps({'event': 'complete', 'file': destination.name}), flush=True)


def main():
    if os.name != 'nt' or platform.machine().lower() not in ('amd64', 'x86_64'):
        raise RuntimeError('閉眼補完モデルは現在Windows x64専用です')
    import torch
    if not torch.cuda.is_available():
        raise RuntimeError('CUDA対応NVIDIA GPUを確認できません。取得しません')
    for item, relative in entries():
        download(item, relative)


if __name__ == '__main__':
    main()
