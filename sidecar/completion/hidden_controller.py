"""2編集・耳解析を個別確定する。完成リグの公開は呼出側が行う。"""
import copy
import hashlib
import json
from pathlib import Path
import shutil
import uuid
import numpy as np
from PIL import Image
from output_transaction import directory_output
from hidden_jobs import JOBS, generation_identity


def digest(path):
    with Path(path).open('rb') as handle:
        return hashlib.file_digest(handle, 'sha256').hexdigest()


def read_edit(cache, identity, size):
    marker = cache/'manifest.json'
    if not marker.is_file():
        return None
    record = json.loads(marker.read_text(encoding='utf-8'))
    if record['identity'] != identity:
        return None
    image = cache/'edited.png'
    if image.is_symlink() or not image.is_file() or digest(image) != record['image_sha256']:
        raise ValueError('追加補完キャッシュのSHA不一致または欠落です')
    with Image.open(image) as opened:
        if opened.size != size:
            raise ValueError('追加補完キャッシュの原寸が不一致です')
    return image, record


def hidden_edits(args, prepared, source, region, model_identity, runtime_identity, workflow_template,
                 guard, infer, assert_comfy_stopped, emit, edit_masks):
    """呼出側が共有生成ロックを保持する。inferは所有ComfyUIを停止・wait後に戻る契約。"""
    if prepared.mode != 'RGB' or prepared.size != (region[2]-region[0], region[3]-region[1]):
        raise ValueError('拡縮していない原寸RGB入力が必要です')
    work = args.character/'temp'/('hidden-jobs-'+uuid.uuid4().hex)
    for name in ('input', 'output', 'temp', 'user'):
        (work/name).mkdir(parents=True)
    prepared.save(work/'input/input.png')
    input_sha = digest(work/'input/input.png')
    results = {}; succeeded = False
    try:
        for job, plan in JOBS.items():
            guard(); assert_comfy_stopped()
            graph = copy.deepcopy(workflow_template)
            mask = edit_masks[job]
            if mask.dtype != np.uint8 or mask.shape != (prepared.height,prepared.width) or not np.any(mask==255):
                raise ValueError('追加補完の原寸編集マスクが不正です')
            Image.fromarray(mask).convert('RGB').save(work/'input/hidden-mask.png')
            mask_sha = digest(work/'input/hidden-mask.png')
            graph.update(json.loads(args.overlay.read_text(encoding='utf-8')))
            graph['14']['inputs']['image'] = 'hidden-mask.png'
            graph['10']['inputs'].update(steps=args.steps, seed=args.seed, latent_image=['15',0])
            graph['13']['inputs']['images'] = ['16',0]
            prompt = args.hidden_prompt if job == 'hidden-face' else args.side_prompt
            if not prompt.strip():
                raise ValueError('追加補完の編集指示が空です')
            graph['7']['inputs']['prompt'] = prompt
            graph['13']['inputs']['filename_prefix'] = job
            parameters = {'steps': args.steps, 'seed': args.seed, 'fast_disk': args.fast_disk, 'prompt': prompt}
            identity = generation_identity(job, source, region, input_sha, model_identity, graph, runtime_identity, parameters)
            identity.update(masked_generation_version=1,mask_sha256=mask_sha,overlay_sha256=digest(args.overlay))
            cache = args.character/plan['cache_directory']
            if cache.is_symlink() or cache.resolve().parent != args.character.resolve():
                raise ValueError('追加補完キャッシュの保存先が不正です')
            existing = read_edit(cache, identity, prepared.size)
            if existing is None:
                emit('hidden_generation', job=job)
                image = infer(args, work, graph)
                assert_comfy_stopped()
                if image.resolve().parent != (work/'output').resolve():
                    raise ValueError('推論画像が今回の生成出力外です')
                with Image.open(image) as opened:
                    validate_hidden_edit(opened,prepared,mask)
                with directory_output(cache) as pending:
                    shutil.copy2(image, pending/'edited.png')
                    record = {'identity': identity, 'image_sha256': digest(pending/'edited.png'),
                              'status': 'generated', 'quality': 'unverified'}
                    (pending/'manifest.json').write_text(json.dumps(record, ensure_ascii=False, indent=2), encoding='utf-8')
                    guard()
                    if digest(work/'input/input.png') != input_sha or digest(work/'input/hidden-mask.png') != mask_sha:
                        raise ValueError('追加補完の公開前に原寸入力が変更されました')
                existing = cache/'edited.png', record
            else:
                emit('hidden_generation_cached', job=job)
                with Image.open(existing[0]) as opened:
                    validate_hidden_edit(opened,prepared,mask)
            results[job] = existing
        guard(); succeeded = True
        return results
    except BaseException as error:
        (work/'failure.json').write_text(json.dumps({'error': str(error)}, ensure_ascii=False), encoding='utf-8')
        raise
    finally:
        if succeeded:
            if work.is_symlink() or work.resolve().parent != (args.character/'temp').resolve():
                raise ValueError('追加補完の一時領域が不正です')
            shutil.rmtree(work)


def validate_hidden_edit(edited, original, mask):
    """原寸とマスク外RGBの完全一致を要求する。構図変更を採用しない。"""
    if edited.size != original.size or not np.any(mask==0):
        raise ValueError('追加編集の原寸または保護範囲が不正です')
    if not np.array_equal(np.array(edited.convert('RGB'))[mask==0],np.array(original.convert('RGB'))[mask==0]):
        raise ValueError('追加編集のマスク外画素が変更されました')


def ear_analysis(cache, image, source_reference, parser_identity, segment, assert_comfy_stopped, guard):
    """生成耳と原画耳の解析を独立キャッシュにする。DINO/SAMはsegment内で逐次解放する。"""
    if cache.is_symlink():
        raise ValueError('耳解析キャッシュにリンクを使用できません')
    fingerprint = digest(image)
    identity = {'version': 1, 'input_sha256': fingerprint, 'source_reference': source_reference, 'parser': parser_identity}
    marker = cache/'manifest.json'
    if marker.is_file():
        record = json.loads(marker.read_text(encoding='utf-8'))
        if record['identity'] == identity:
            archive = cache/'masks.npz'
            if not archive.is_file() or archive.is_symlink() or digest(archive) != record['masks_sha256']:
                raise ValueError('耳解析キャッシュのSHAが一致しません')
            with np.load(archive, allow_pickle=False) as data:
                masks = [data['left'].copy(), data['right'].copy()]
            with Image.open(image) as opened:
                expected = (opened.height, opened.width)
            if any(mask.dtype != bool or mask.shape != expected for mask in masks):
                raise ValueError('耳解析キャッシュの原寸が一致しません')
            if not source_reference and any(not mask.any() for mask in masks):
                raise ValueError('キャッシュ内に左右の生成耳が必要です')
            guard()
            return masks, record
    assert_comfy_stopped(); guard()
    with Image.open(image) as opened:
        original = opened.convert('RGB')
    masks, details = segment(original, source_reference)
    if len(masks) != 2 or any(mask.dtype != bool or mask.shape != (original.height, original.width) for mask in masks):
        raise ValueError('耳解析の出力が原寸左右マスクではありません')
    if not source_reference and any(not mask.any() for mask in masks):
        raise ValueError('左右の生成耳が必要です')
    with directory_output(cache) as pending:
        np.savez_compressed(pending/'masks.npz', left=masks[0], right=masks[1])
        record = {'identity': identity, 'masks_sha256': digest(pending/'masks.npz'), 'details': details}
        (pending/'manifest.json').write_text(json.dumps(record, ensure_ascii=False, indent=2), encoding='utf-8')
        guard()
        if digest(image) != fingerprint:
            raise ValueError('耳解析公開前に入力画像が変更されました')
    return masks, record


def assert_result_sources(results, guard):
    """組立直前と完成リグ公開直前に呼び、元画像と2生成画像の置換を拒否する。"""
    guard()
    for image, record in results.values():
        if digest(image) != record['image_sha256']:
            raise ValueError('組立中に補完画像が変更されました')
