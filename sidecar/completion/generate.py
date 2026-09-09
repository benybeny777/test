"""管理下ComfyUIで局所閉眼を補完し、検証済みのリグ世代だけを公開する。"""
import argparse
import copy
import hashlib
import importlib.util
import importlib.metadata
from contextlib import contextmanager
import json
from io import BytesIO
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
from reference_store import acquire as acquire_rig, publish as publish_rig
from rig2d.texture import bleed_transparent_rgb
from apply import apply_closed_eyes
from materials import validate_masked_pixels
from regions import eye_edit_mask, measured_head_region
from hidden_bridge import prepare_hidden, apply_hidden, assert_hidden_sources
from raw_reuse import open_raw,pin_generated

spec = importlib.util.spec_from_file_location('completion_comfy_client', HERE.parent/'expression/generate.py')
client = importlib.util.module_from_spec(spec)
spec.loader.exec_module(client)
VERSION = 5
# 推論や入力準備の意味を変えた場合に上げる。抽出だけの変更ではGPUを再実行しない。
IMAGE_GENERATION_VERSION = 5
PROMPT = ('Close only the specified eye completely, preserving the original character identity and original rendering style. '
          'The specified eye must be fully shut with a relaxed closed eyelid and a thin natural eyelash line; leave no iris, pupil, sclera, or open eye visible. Edit only that eye within the mask. '
          'Preserve the original hair, eyebrows, nose, mouth, skin texture, lighting, pose and image framing. '
          'Do not change any unmasked area.')


def side_prompt(base, side):
    label = 'left' if side == 'left' else 'right'
    return f'{base} The specified eye is the character\'s {label} eye.'


def sequential_eye_workflow(template, args):
    """同一ComfyUI起動内で左右を別マスクのまま直列編集する。"""
    workflow = copy.deepcopy(template)
    required = {'7','8','9','10','12','13','14','15','16'}
    if not required.issubset(workflow):
        raise ValueError('閉眼の左右直列編集に必要なworkflowノードがありません')
    workflow['7']['inputs']['prompt'] = side_prompt(args.prompt,'left')
    workflow['14']['inputs']['image'] = 'left-eye-mask.png'
    workflow['10']['inputs'].update(steps=args.steps, seed=args.seed, latent_image=['15',0])
    mapping={'7':'17','8':'18','9':'19','10':'20','12':'21','14':'22','15':'23','16':'24'}
    def remap(value):
        if isinstance(value,list):
            if len(value)==2 and isinstance(value[0],str) and value[0] in mapping:
                return [mapping[value[0]],value[1]]
            return [remap(item) for item in value]
        if isinstance(value,dict):return {key:remap(item) for key,item in value.items()}
        return value
    for old,new in mapping.items():workflow[new]=remap(copy.deepcopy(workflow[old]))
    workflow['17']['inputs'].update(prompt=side_prompt(args.prompt,'right'),image1=['16',0])
    workflow['18']['inputs']['image1']=['16',0]
    workflow['19']['inputs']['pixels']=['16',0]
    workflow['20']['inputs'].update(steps=args.steps,seed=(args.seed+1)%(2**32),latent_image=['23',0])
    workflow['22']['inputs']['image']='right-eye-mask.png'
    workflow['24']['inputs']['destination']=['16',0]
    workflow['13']['inputs']['images']=['24',0]
    return workflow


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


def cached_edit(cache, identity, original, mask):
    """生成済み原寸画像だけを再利用する。素材抽出の合格とは区別する。"""
    lease=open_raw(cache,identity,'eye')
    if lease is None:return None
    with Image.open(BytesIO(lease.image_bytes())) as opened:
        edited = np.array(opened.convert('RGBA'))
    validate_masked_pixels(edited, original, mask)
    return lease


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
    for field in ('character', 'base_rig', 'output', 'comfy', 'models', 'workflow', 'overlay', 'grounding_model', 'sam_model'):
        setattr(args, field, getattr(args, field).resolve())
    base = args.base_rig.parent
    if args.output == base or args.output.is_relative_to(base) or base.is_relative_to(args.output):
        raise ValueError('補完元リグと完成出力を分離してください')
    if args.output == args.character or not args.output.is_relative_to(args.character):
        raise ValueError('完成出力はキャラクター内の専用ディレクトリに限定します')
    if args.output == args.character/'completion-source' or args.output.is_relative_to(args.character/'completion-source'):
        raise ValueError('補完画像キャッシュを完成リグ出力で上書きできません')
    for name in ('completion-hidden-source','completion-side-source','completion-original-ears','completion-generated-ears','source','analysis','temp'):
        protected = args.character/name
        if args.output == protected or args.output.is_relative_to(protected) or protected.is_relative_to(args.output):
            raise ValueError('原画・解析・補完キャッシュを完成出力で上書きできません')
    if args.port == 8188 or not 1024 <= args.port <= 65535:
        raise ValueError('管理専用ポートを指定してください')
    if (args.comfy/'extra_model_paths.yaml').exists():
        raise ValueError('既存ComfyUIの追加モデル設定は利用しません')
    if not 1 <= args.steps <= 100 or not 256 <= args.resolution <= 4096 or not 0 < args.mask_margin_ratio <= .5 or not 0 <= args.mask_core_ratio < 1:
        raise ValueError('補完の生成設定が範囲外です')
    if args.startup_timeout <= 0 or args.generation_timeout <= 0 or not args.prompt.strip():
        raise ValueError('補完の待機時間または編集指示が不正です')
    if not args.hidden_prompt.strip() or not args.side_prompt.strip() or not 0 <= args.grounding_threshold <= 1 or not 0 < args.ear_context <= 2:
        raise ValueError('隠れ素材の編集指示・耳解析設定が不正です')
    if not 0 < args.hidden_band_ratio <= .15 or not 0 < args.hidden_motion_ratio <= .4 or not 0 < args.hair_edge_band_ratio <= .05 or not 0 < args.hair_edge_gain <= 255:
        raise ValueError('隠れ素材の抽出設定が不正です')
    models = json.loads((HERE/'models.json').read_text(encoding='utf-8'))
    for item in models['files']:
        path = args.models/Path(item['path']).relative_to('split_files')
        print(json.dumps({'event': 'completion_checking_model', 'file': path.name}), flush=True)
        if not path.is_file():
            raise ValueError(f'補完モデルがありません: {path.name}。cargo xtask setup completion を実行してください')
        if digest(path) != item['sha256']:
            raise ValueError(f'補完モデルの固定SHAが一致しません: {path.name}')
    source = source_hashes(args.character)
    baseline = tree_hashes(base)
    extraction_code = {p.name: digest(p) for p in HERE.glob('*.py') if not p.name.startswith('test_')}
    comfy_code = {p.relative_to(args.comfy).as_posix(): digest(p) for folder in ('comfy', 'comfy_extras')
                  for p in sorted((args.comfy/folder).rglob('*.py'))}
    comfy_code['main.py'] = digest(args.comfy/'main.py')
    generation_identity = {'version': IMAGE_GENERATION_VERSION, 'source': source, 'models': models,
                'comfy_code': comfy_code,
                'runtime': {name: importlib.metadata.version(name) for name in ('numpy', 'scipy', 'Pillow', 'torch', 'transformers')},
                'workflow': digest(args.workflow), 'overlay': digest(args.overlay),
                'parameters': {k: getattr(args, k) for k in ('steps', 'seed', 'resolution', 'mask_margin_ratio', 'mask_core_ratio', 'prompt', 'fast_disk')}}
    rig = json.loads(args.base_rig.read_text(encoding='utf-8'))
    metadata = json.loads((args.character/'analysis/analysis.json').read_text(encoding='utf-8'))
    with Image.open(args.character/'source/isolated.png') as opened:
        isolated = opened.convert('RGBA')
    selected = metadata['analysis']['selected']
    bounds = measured_head_region(selected['face']['box'], selected['neck']['box'], isolated.size, args.resolution)
    l, t, r, b = bounds
    original = np.array(isolated.crop(bounds))
    with np.load(args.character/'analysis/masks.npz', allow_pickle=False) as masks:
        all_masks = {name:masks[name].copy() for name in masks.files}
        eye_masks = {side:eye_edit_mask([masks[side+'_eye'][t:b, l:r]],
                     masks['hair'][t:b, l:r], original[:, :, 3], args.mask_margin_ratio,args.mask_core_ratio)
                     for side in ('left','right')}
        if np.any((eye_masks['left']>0)&(eye_masks['right']>0)):
            raise ValueError('左右の閉眼編集マスクが重なっています')
        mask = np.maximum(eye_masks['left'],eye_masks['right'])
    run = args.character/'temp'/('completion-'+uuid.uuid4().hex)
    for name in ('input', 'output', 'temp', 'user'):
        (run/name).mkdir(parents=True)
    white = Image.new('RGBA', (r-l, b-t), 'white')
    white.alpha_composite(Image.fromarray(original))
    white.convert('RGB').save(run/'input/input.png')
    for side in ('left','right'):
        Image.fromarray(eye_masks[side]).convert('RGB').save(run/f'input/{side}-eye-mask.png')
    workflow = json.loads(args.workflow.read_text(encoding='utf-8'))
    workflow.update(json.loads(args.overlay.read_text(encoding='utf-8')))
    # 左右は別マスクの原寸潜在表現を順番に編集し、両目同時指示の片目残りを避ける。
    workflow = sequential_eye_workflow(workflow,args)
    inputs = tree_hashes(run/'input')
    generation_identity.update(inputs=inputs, source_region=list(bounds), resolved_workflow=workflow)
    def guard():
        if source_hashes(args.character) != source or tree_hashes(base) != baseline or tree_hashes(run/'input') != inputs:
            raise ValueError('補完中に原画・解析・元リグ・編集入力が変更されました')
        current = {p.name:digest(p) for p in HERE.glob('*.py') if not p.name.startswith('test_')}
        if current != extraction_code:
            raise ValueError('補完中に抽出コードが変更されました')
    def emit(event, **details):
        print(json.dumps({'event':event,**details},ensure_ascii=False),flush=True)
    cache = args.character/'completion-source'
    succeeded = False
    try:
        eye_lease = cached_edit(cache, generation_identity, np.array(white), mask)
        if eye_lease is None:
            generated_path = generate_image(args, run, workflow)
            with Image.open(generated_path) as opened:
                validate_masked_pixels(np.array(opened.convert('RGBA')), np.array(white), mask)
            with directory_output(cache) as pending:
                shutil.copy2(generated_path, pending/'edited.png')
                record = {'identity': generation_identity, 'edited_sha256': digest(pending/'edited.png'),
                          'status': 'generated', 'material_quality': 'unverified'}
                (pending/'manifest.json').write_text(json.dumps(record, ensure_ascii=False, indent=2), encoding='utf-8')
                manifest_bytes=(pending/'manifest.json').read_bytes()
                if source_hashes(args.character) != source or tree_hashes(run/'input') != inputs:
                    raise ValueError('補完画像公開の直前に原画・解析・編集入力が変更されました')
            eye_lease=pin_generated(cache,manifest_bytes,generation_identity,'eye')
        else:
            print(json.dumps({'event': 'completion_edit_cached', 'output': str(cache)}), flush=True)
        prepared_hidden = prepare_hidden(args,white,bounds,source,generation_identity,guard,generate_image,emit,run/'input/input.png',all_masks)
        eye_lease.recheck()
        identity = {'version': VERSION, 'source': source, 'base': baseline, 'generation_requested': generation_identity,
                    'raw_origin':eye_lease.origin(),'edited_sha256':eye_lease.image_sha256,
                    'hidden': prepared_hidden['identity'],
                    'code': extraction_code}
        if os.path.lexists(args.character/'rig-current.json'):
            with acquire_rig(args.character) as (_,published_directory):
                if cache_valid(published_directory, identity):
                    assert_hidden_sources(args,prepared_hidden,guard)
                    eye_lease.recheck()
                    cached_rig = json.loads((published_directory/'rig.json').read_text(encoding='utf-8'))
                    warning = cached_rig.get('local_completion',{}).get('hidden',{}).get('warning')
                    if warning:
                        emit('completion_warning',message=warning)
                    print(json.dumps({'event': 'completion_cached', 'output': str(published_directory)}), flush=True)
                    succeeded = True
                    return
        with Image.open(BytesIO(eye_lease.image_bytes())) as opened:
            edited = np.array(opened.convert('RGBA'))
        validate_masked_pixels(edited, np.array(white), mask)
        materials, measurements = apply_closed_eyes(args.character, base, rig, edited, original, mask, bounds)
        rig, materials, hidden_report = apply_hidden(args,rig,base,materials,isolated,all_masks,bounds,prepared_hidden,guard)
        if source_hashes(args.character) != source or tree_hashes(base) != baseline or tree_hashes(run/'input') != inputs:
            raise ValueError('補完中に原画・解析・リグ・編集入力が変更されました')
        def build_final(pending):
            shutil.copytree(base, pending, dirs_exist_ok=True)
            for name, image in materials.items():
                bleed_transparent_rgb(image).save(pending/f'parts/{name}.png')
            rig['local_completion'] = {'version': VERSION, 'model': 'Qwen-Image-Edit-2511', 'measurements': measurements}
            rig['local_completion']['hidden'] = hidden_report
            (pending/'rig.json').write_text(json.dumps(rig, ensure_ascii=False, indent=2), encoding='utf-8')
            allowed = {'rig.json'} | {f'parts/{name}.png' for name in materials}
            outputs = tree_hashes(pending)
            if any(outputs.get(name) != sha for name, sha in baseline.items() if name not in allowed):
                raise ValueError('許可された補完素材以外が変わりました')
            record = {'identity': identity, 'outputs': outputs, 'edited_sha256': eye_lease.image_sha256, 'source_region': bounds}
            (pending/'completion.json').write_text(json.dumps(record, ensure_ascii=False, indent=2), encoding='utf-8')
        def verify_final_sources():
            if source_hashes(args.character) != source or tree_hashes(base) != baseline or tree_hashes(run/'input') != inputs:
                raise ValueError('完成公開の直前に原画・解析・元リグが変更されました')
            current_code = {p.name: digest(p) for p in HERE.glob('*.py') if not p.name.startswith('test_')}
            if current_code != extraction_code:
                raise ValueError('完成公開の直前に補完画像または抽出コードが変更されました')
            assert_hidden_sources(args,prepared_hidden,guard)
            eye_lease.recheck()
        publication = publish_rig(args.character,build_final,verify_final_sources)
        if publication.get('cleanup_warning'):emit('completion_warning',message=publication['cleanup_warning'])
        if hidden_report.get('warning'):
            emit('completion_warning',message=hidden_report['warning'])
        print(json.dumps({'event': 'completion_complete', 'output': str(args.character/'rig-current.json'), 'generation': publication['generation']}), flush=True)
        succeeded = True
    except BaseException as error:
        (run/'failure.json').write_text(json.dumps({'error': str(error)}, ensure_ascii=False), encoding='utf-8')
        raise
    finally:
        if succeeded:
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
    for name in ('character', 'base-rig', 'output', 'comfy', 'models', 'workflow', 'overlay','grounding-model','sam-model'):
        parser.add_argument('--'+name, type=Path, required=True)
    for name, default in (('steps', 50), ('seed', 777), ('resolution', 1024), ('port', 58125),
                          ('startup-timeout', 600), ('generation-timeout', 14400)):
        parser.add_argument('--'+name, type=int, default=default)
    parser.add_argument('--mask-margin-ratio', type=float, default=.2)
    parser.add_argument('--mask-core-ratio', type=float, required=True)
    parser.add_argument('--fast-disk', action='store_true')
    parser.add_argument('--prompt', default=PROMPT)
    for name in ('hidden-prompt','side-prompt'):
        parser.add_argument('--'+name,required=True)
    for name in ('hidden-band-ratio','hidden-motion-ratio','hair-edge-band-ratio','hair-edge-gain','ear-context','grounding-threshold'):
        parser.add_argument('--'+name,type=float,required=True)
    complete(parser.parse_args())


if __name__ == '__main__':
    sys.stdout.reconfigure(encoding='utf-8')
    main()
