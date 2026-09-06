"""管理下ComfyUIで局所閉眼を補完し、検証済みのリグ世代だけを公開する。"""
import argparse
import hashlib
import importlib.util
import importlib.metadata
from contextlib import contextmanager
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import time
import uuid

import numpy as np
from PIL import Image
import psutil

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
from output_transaction import directory_output
from rig2d.texture import bleed_transparent_rgb
from apply import apply_closed_eyes
from materials import validate_masked_pixels
from regions import eye_edit_mask, measured_head_region

spec = importlib.util.spec_from_file_location('completion_comfy_client', HERE.parent/'expression/generate.py')
client = importlib.util.module_from_spec(spec)
spec.loader.exec_module(client)
VERSION = 1
PROMPT = ('Close both eyes naturally, preserving the original character identity and original rendering style. '
          'Relaxed closed eyelids with a thin natural eyelash line. Edit only the eyes within the mask. '
          'Preserve the original hair, eyebrows, nose, mouth, skin texture, lighting, pose and image framing. '
          'Do not change any unmasked area.')


def digest(path):
    with Path(path).open('rb') as handle:
        return hashlib.file_digest(handle, 'sha256').hexdigest()


def tree_hashes(path):
    result = {}
    for item in sorted(path.rglob('*')):
        if item.is_symlink():
            raise ValueError('補完入力にリンクを含められません')
        if item.is_file():
            result[item.relative_to(path).as_posix()] = digest(item)
    return result


def source_hashes(character):
    return {name: digest(character/name) for name in
            ('source/input.png', 'source/isolated.png', 'analysis/analysis.json', 'analysis/masks.npz')}


@contextmanager
def generation_lock(character):
    """同じキャラクター集合のGPU補完を、生成開始前から公開まで直列化する。"""
    workspace = character.parent/'temp'
    workspace.mkdir(exist_ok=True)
    if workspace.is_symlink() or workspace.resolve() != workspace:
        raise ValueError('補完ロックの一時領域が不正です')
    with (workspace/'completion-gpu.lock').open('a+b') as lock:
        lock.seek(0, 2)
        if lock.tell() == 0:
            lock.write(b'0'); lock.flush()
        lock.seek(0)
        if os.name == 'nt':
            import msvcrt
            msvcrt.locking(lock.fileno(), msvcrt.LK_NBLCK, 1)
        else:
            import fcntl
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        try:
            yield
        finally:
            lock.seek(0)
            if os.name == 'nt':
                msvcrt.locking(lock.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                fcntl.flock(lock, fcntl.LOCK_UN)


def cache_valid(output, identity):
    marker = output/'completion.json'
    if not marker.exists():
        return False
    record = json.loads(marker.read_text(encoding='utf-8'))
    if record['identity'] != identity:
        return False
    current = tree_hashes(output)
    current.pop('completion.json')
    if current != record['outputs']:
        raise ValueError('補完キャッシュの素材が改変または欠落しています')
    return True


def stop_owned(process):
    if process.poll() is not None:
        process.wait()
        return
    parent = psutil.Process(process.pid)
    members = parent.children(recursive=True) + [parent]
    for member in reversed(members):
        try:
            member.terminate()
        except psutil.NoSuchProcess:
            pass
    _, alive = psutil.wait_procs(members, timeout=10)
    for member in alive:
        try:
            member.kill()
        except psutil.NoSuchProcess:
            pass
    psutil.wait_procs(alive, timeout=10)
    process.wait()


def generate_image(args, run, workflow):
    with socket.socket() as probe:
        probe.bind(('127.0.0.1', args.port))
    command = [sys.executable, str(args.comfy/'main.py'), '--listen', '127.0.0.1', '--port', str(args.port),
               '--base-directory', str(run), '--models-directory', str(args.models),
               '--temp-directory', str(run/'temp'), '--disable-auto-launch', '--disable-all-custom-nodes',
               '--disable-api-nodes', '--preview-method', 'none']
    if args.fast_disk:
        command.append('--fast-disk')
    environment = dict(os.environ, HF_HUB_OFFLINE='1', TRANSFORMERS_OFFLINE='1', HF_HUB_DISABLE_TELEMETRY='1')
    with (run/'comfy.log').open('w', encoding='utf-8') as log:
        process = subprocess.Popen(command, cwd=args.comfy, env=environment, stdout=log,
                                   stderr=subprocess.STDOUT, creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
        try:
            url = f'http://127.0.0.1:{args.port}'
            client.wait_for_server(url, process, args.startup_timeout)
            queued = client.request_json(url+'/prompt', {'prompt': workflow})
            if not queued.get('prompt_id'):
                raise ValueError(f'局所補完ワークフローが拒否されました: {queued}')
            started = time.monotonic()
            while time.monotonic()-started < args.generation_timeout:
                if process.poll() is not None:
                    raise RuntimeError(f'ComfyUIが補完中に終了しました: {process.returncode}')
                history = client.request_json(url+'/history/'+queued['prompt_id'])
                result = history.get(queued['prompt_id'], {})
                if result.get('status', {}).get('status_str') == 'error':
                    raise RuntimeError(json.dumps(result['status'], ensure_ascii=False))
                images = result.get('outputs', {}).get('13', {}).get('images')
                if images:
                    if len(images) != 1:
                        raise ValueError('閉眼補完の生成枚数が不正です')
                    item = images[0]
                    image = (run/'output'/item.get('subfolder', '')/item['filename']).resolve()
                    if not image.is_relative_to(run/'output'):
                        raise ValueError('補完画像の保存先が範囲外です')
                    return image
                print(json.dumps({'event': 'completion_generating', 'seconds': round(time.monotonic()-started)}), flush=True)
                time.sleep(5)
            raise TimeoutError('閉眼補完が制限時間を超えました')
        finally:
            stop_owned(process)


def complete_locked(args):
    for field in ('character', 'base_rig', 'output', 'comfy', 'models', 'workflow', 'overlay'):
        setattr(args, field, getattr(args, field).resolve())
    base = args.base_rig.parent
    if args.output == base or args.output.is_relative_to(base) or base.is_relative_to(args.output):
        raise ValueError('補完元リグと完成出力を分離してください')
    if args.output == args.character or not args.output.is_relative_to(args.character):
        raise ValueError('完成出力はキャラクター内の専用ディレクトリに限定します')
    if args.port == 8188 or not 1024 <= args.port <= 65535:
        raise ValueError('管理専用ポートを指定してください')
    if (args.comfy/'extra_model_paths.yaml').exists():
        raise ValueError('既存ComfyUIの追加モデル設定は利用しません')
    if not 1 <= args.steps <= 100 or not 256 <= args.resolution <= 4096 or not 0 < args.mask_margin_ratio <= .5:
        raise ValueError('補完の生成設定が範囲外です')
    if args.startup_timeout <= 0 or args.generation_timeout <= 0 or not args.prompt.strip():
        raise ValueError('補完の待機時間または編集指示が不正です')
    models = json.loads((HERE/'models.json').read_text(encoding='utf-8'))
    for item in models['files']:
        path = args.models/Path(item['path']).relative_to('split_files')
        print(json.dumps({'event': 'completion_checking_model', 'file': path.name}), flush=True)
        if digest(path) != item['sha256']:
            raise ValueError(f'補完モデルの固定SHAが一致しません: {path.name}')
    source = source_hashes(args.character)
    baseline = tree_hashes(base)
    comfy_code = {p.relative_to(args.comfy).as_posix(): digest(p) for folder in ('comfy', 'comfy_extras')
                  for p in sorted((args.comfy/folder).rglob('*.py'))}
    comfy_code['main.py'] = digest(args.comfy/'main.py')
    identity = {'version': VERSION, 'source': source, 'base': baseline, 'models': models,
                'code': {p.name: digest(p) for p in HERE.glob('*.py') if not p.name.startswith('test_')},
                'comfy_code': comfy_code,
                'runtime': {name: importlib.metadata.version(name) for name in ('numpy', 'scipy', 'Pillow', 'torch', 'transformers')},
                'workflow': digest(args.workflow), 'overlay': digest(args.overlay),
                'parameters': {k: getattr(args, k) for k in ('steps', 'seed', 'resolution', 'mask_margin_ratio', 'prompt', 'fast_disk')}}
    if cache_valid(args.output, identity):
        print(json.dumps({'event': 'completion_cached', 'output': str(args.output)}), flush=True)
        return
    rig = json.loads(args.base_rig.read_text(encoding='utf-8'))
    metadata = json.loads((args.character/'analysis/analysis.json').read_text(encoding='utf-8'))
    with Image.open(args.character/'source/isolated.png') as opened:
        isolated = opened.convert('RGBA')
    selected = metadata['analysis']['selected']
    bounds = measured_head_region(selected['face']['box'], selected['neck']['box'], isolated.size, args.resolution)
    l, t, r, b = bounds
    original = np.array(isolated.crop(bounds))
    with np.load(args.character/'analysis/masks.npz', allow_pickle=False) as masks:
        mask = eye_edit_mask([masks[s+'_eye'][t:b, l:r] for s in ('left', 'right')],
                             masks['hair'][t:b, l:r], original[:, :, 3], args.mask_margin_ratio)
    run = args.character/'temp'/('completion-'+uuid.uuid4().hex)
    for name in ('input', 'output', 'temp', 'user'):
        (run/name).mkdir(parents=True)
    white = Image.new('RGBA', (r-l, b-t), 'white')
    white.alpha_composite(Image.fromarray(original))
    white.convert('RGB').save(run/'input/input.png')
    Image.fromarray(mask).convert('RGB').save(run/'input/eye-mask.png')
    workflow = json.loads(args.workflow.read_text(encoding='utf-8'))
    workflow.update(json.loads(args.overlay.read_text(encoding='utf-8')))
    # overlay の VAEEncode は原寸入力から潜在寸法を決める。幅・高さの再指定や拡大はしない。
    workflow['10']['inputs'].update(steps=args.steps, seed=args.seed, latent_image=['15', 0])
    workflow['7']['inputs']['prompt'] = args.prompt
    workflow['13']['inputs']['images'] = ['16', 0]
    inputs = tree_hashes(run/'input')
    try:
        edited_path = generate_image(args, run, workflow)
        with Image.open(edited_path) as opened:
            edited = np.array(opened.convert('RGBA'))
        validate_masked_pixels(edited, np.array(white), mask)
        materials, measurements = apply_closed_eyes(args.character, base, rig, edited, original, mask, bounds)
        if source_hashes(args.character) != source or tree_hashes(base) != baseline or tree_hashes(run/'input') != inputs:
            raise ValueError('補完中に原画・解析・リグ・編集入力が変更されました')
        with directory_output(args.output) as pending:
            shutil.copytree(base, pending, dirs_exist_ok=True)
            for name, image in materials.items():
                bleed_transparent_rgb(image).save(pending/f'parts/{name}.png')
            rig['local_completion'] = {'version': VERSION, 'model': 'Qwen-Image-Edit-2511', 'measurements': measurements}
            (pending/'rig.json').write_text(json.dumps(rig, ensure_ascii=False, indent=2), encoding='utf-8')
            allowed = {'rig.json'} | {f'parts/{name}.png' for name in materials}
            outputs = tree_hashes(pending)
            if any(outputs.get(name) != sha for name, sha in baseline.items() if name not in allowed):
                raise ValueError('閉眼以外の素材が変わりました')
            record = {'identity': identity, 'outputs': outputs, 'edited_sha256': digest(edited_path), 'source_region': bounds}
            (pending/'completion.json').write_text(json.dumps(record, ensure_ascii=False, indent=2), encoding='utf-8')
            if source_hashes(args.character) != source or tree_hashes(base) != baseline:
                raise ValueError('完成公開の直前に原画・解析・元リグが変更されました')
        print(json.dumps({'event': 'completion_complete', 'output': str(args.output)}), flush=True)
    except BaseException as error:
        (run/'failure.json').write_text(json.dumps({'error': str(error)}, ensure_ascii=False), encoding='utf-8')
        raise
    else:
        if run.is_symlink() or run.resolve().parent != (args.character/'temp').resolve():
            raise ValueError('補完一時出力の削除範囲が不正です')
        shutil.rmtree(run)


def complete(args):
    character = args.character.absolute()
    if character.resolve() != character:
        raise ValueError('キャラクターにリンクを使用できません')
    with generation_lock(character):
        complete_locked(args)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('character', 'base-rig', 'output', 'comfy', 'models', 'workflow', 'overlay'):
        parser.add_argument('--'+name, type=Path, required=True)
    for name, default in (('steps', 50), ('seed', 777), ('resolution', 1024), ('port', 58125),
                          ('startup-timeout', 600), ('generation-timeout', 14400)):
        parser.add_argument('--'+name, type=int, default=default)
    parser.add_argument('--mask-margin-ratio', type=float, default=.2)
    parser.add_argument('--fast-disk', action='store_true')
    parser.add_argument('--prompt', default=PROMPT)
    complete(parser.parse_args())


if __name__ == '__main__':
    sys.stdout.reconfigure(encoding='utf-8')
    main()
