"""通常補完の耳解析・隠れ素材生成・リグ組立を接続する。"""
import json
from pathlib import Path
import numpy as np
from PIL import Image
from hidden_controller import hidden_edits, ear_analysis, assert_result_sources, digest
from ear_runtime import segment_ear_image
from hidden_assemble import assemble_hidden
from hidden_materials import HiddenSettings
from hidden_regions import scene_support,hidden_edit_masks,white_reference


def prepare_hidden(args, white, region, source, generation, guard, infer, emit, input_path, masks):
    """固定解析モデルと生成元を確認し、個別キャッシュから原寸素材を用意する。"""
    metadata = json.loads((args.character/'analysis/analysis.json').read_text(encoding='utf-8'))
    models = {}
    for name, directory in [('detector', args.grounding_model), ('sam', args.sam_model)]:
        directory = Path(directory).resolve()
        if not directory.is_dir():
            raise ValueError('既存の解析モデルがありません: '+name)
        models[name] = {path.name: digest(path) for path in sorted(directory.iterdir())
                        if path.is_file() and path.suffix in ('.json', '.txt', '.safetensors')}
    if not all(models.values()) or models != metadata['identity']['models']:
        raise ValueError('耳補完のモデルが元のDINO/SAM解析と一致しません。分解から再実行してください')
    # infer は finally で所有 ComfyUI を終了・wait してから戻る同期関数である。
    running = False
    def ensure_stopped():
        if running:
            raise RuntimeError('ComfyUIと耳解析の同時実行を拒否しました')
    def infer_serial(*values):
        nonlocal running
        if running:
            raise RuntimeError('補完推論が重複しました')
        running = True
        try:
            return infer(*values)
        finally:
            running = False
    runtime = {'runtime': generation['runtime'], 'comfy_code': generation['comfy_code'],
               'workflow_sha256': generation['workflow']}
    parser = {'models': models, 'threshold': args.grounding_threshold, 'context': args.ear_context,
              'runtime': generation['runtime'], 'code': digest(Path(__file__).with_name('ear_runtime.py'))}
    def segment(image, original):
        return segment_ear_image(image, args.grounding_model, args.sam_model, args.grounding_threshold,
                                 args.ear_context, original, lambda event: emit(event))
    # 原画耳の解析を先に確定し、解析モデル解放後だけ画像編集を開始する。
    original = ear_analysis(args.character/'completion-original-ears',input_path,True,parser,segment,ensure_stopped,guard)
    l,t,r,b = region
    base_rig=json.loads(args.base_rig.read_text(encoding='utf-8'))
    names={'scene_face'}|{node['layer'] for node in base_rig['scene_graph'] if node['role']=='hair'}
    if 'scene_residual' in base_rig['layers']:names.add('scene_residual')
    parts={}
    for name in names:
        with Image.open(args.base_rig.parent/f'parts/{name}.png') as image:parts[name]=np.array(image.convert('RGBA'))
    with Image.open(args.character/'source/isolated.png') as image:isolated=np.array(image.convert('RGBA'))
    _,surface,hair=scene_support(base_rig,parts,isolated,masks)
    roi=isolated[t:b,l:r]
    if not np.array_equal(np.array(white.convert('RGB')),white_reference(roi)[:,:,:3]):
        raise ValueError('髪境界判定と生成の白合成入力が不一致です')
    features=[masks[name][t:b,l:r] for name in ('left_eye','right_eye','mouth')]
    face_box=base_rig['layers']['face']['bbox']
    band=max(1,round((face_box[2]-face_box[0])*args.hair_edge_band_ratio))
    original_ears = original[0][0] | original[0][1]
    edit_masks=hidden_edit_masks(roi,surface[t:b,l:r],hair[t:b,l:r],features,original_ears,band)
    workflow = json.loads(args.workflow.read_text(encoding='utf-8'))
    results = hidden_edits(args,white.convert('RGB'),source,region,generation['models'],runtime,
                          workflow,guard,infer_serial,ensure_stopped,emit,edit_masks)
    generated = ear_analysis(args.character/'completion-generated-ears',results['side-ears'][0],False,
                             parser,segment,ensure_stopped,guard)
    # 片側だけの測定で反対側の旧輪郭まで消さない。raw左右マスクは独立保存済み。
    contour_ears = original_ears if all(mask.any() for mask in original[0]) else np.zeros_like(original_ears)
    ears = {'generated_ears':generated[0][0]|generated[0][1], 'source_ears':contour_ears,
            'reports':{'generated':generated[1],'source':original[1]}}
    assert_result_sources(results,guard)
    images = {}
    for job, (path, _) in results.items():
        with Image.open(path) as image:
            images[job] = np.array(image.convert('RGBA'))
    return {'results': results, 'images': images, 'ears': ears,
            'identity': {'generations': {job: record for job, (_, record) in results.items()},
                         'ears': ears['reports'],
                         'settings': {name: getattr(args, name) for name in
                                      ('hidden_band_ratio', 'hidden_motion_ratio', 'hair_edge_band_ratio', 'hair_edge_gain')}}}


def assert_hidden_sources(args, prepared, guard):
    assert_result_sources(prepared['results'], guard)
    for name,folder in (('source','completion-original-ears'),('generated','completion-generated-ears')):
        if digest(args.character/folder/'masks.npz') != prepared['ears']['reports'][name]['masks_sha256']:
            raise ValueError('完成公開の直前に耳解析素材が変更されました')


def apply_hidden(args, rig, base, eye_parts, isolated, masks, region, prepared, guard):
    assert_hidden_sources(args,prepared,guard)
    parts = {}
    for name in rig['layers']:
        with Image.open(base/f'parts/{name}.png') as opened:
            parts[name] = np.array(opened.convert('RGBA'))
    parts.update({name: np.array(image.convert('RGBA')) for name, image in eye_parts.items()})
    settings = HiddenSettings(args.hidden_band_ratio, args.hidden_motion_ratio,
                              args.hair_edge_band_ratio, args.hair_edge_gain)
    updated, pixels, report = assemble_hidden(rig, parts, np.array(isolated), masks,
        prepared['images']['hidden-face'], prepared['images']['side-ears'], list(region), settings,
        prepared['ears']['source_ears'], prepared['ears']['generated_ears'], require_visible_contour=False)
    if prepared['ears']['reports']['source']['details']['status'] == 'partial':
        report.update(status='partial',warning='原画耳の一部または全部が未検出です。未測定の輪郭修正は完了扱いにしません')
        updated['local_hidden_completion'] = report
    changed = {name: Image.fromarray(image) for name, image in pixels.items()
               if name not in parts or not np.array_equal(image, parts[name])}
    changed = {**eye_parts, **changed}
    assert_hidden_sources(args,prepared,guard)
    return updated, changed, report
