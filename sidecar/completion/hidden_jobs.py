"""追加2編集のキャッシュ契約実装。推論と素材抽出を分離する。"""

JOBS = {
    'hidden-face': {
        'cache_directory': 'completion-hidden-source',
    },
    'side-ears': {
        'cache_directory': 'completion-side-source',
    },
}


def generation_identity(job, source, source_region, prepared_sha, models, workflow, runtime, parameters):
    """キャラIDではなく実入力と条件で識別する。抽出コードはこの署名に含めない。"""
    if job not in JOBS:
        raise ValueError('未承認の補完タスクです')
    required = {'source/input.png', 'source/isolated.png', 'analysis/analysis.json', 'analysis/masks.npz'}
    if set(source) != required:
        raise ValueError('原画・解析の4SHAが必要です')
    if len(source_region) != 4 or source_region[2] <= source_region[0] or source_region[3] <= source_region[1]:
        raise ValueError('原寸ROIが不正です')
    if not isinstance(parameters.get('prompt'), str) or not parameters['prompt'].strip():
        raise ValueError('実行する編集指示が署名に必要です')
    return {'version': 1, 'job': job, 'source': source, 'source_region': list(source_region),
            'prepared_input_sha256': prepared_sha, 'models': models, 'workflow': workflow,
            'runtime': runtime, 'parameters': parameters,
            'upscaled': False, 'output_adoption': 'measured-hidden-or-ear-region-only'}
