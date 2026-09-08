"""3rawの実入力同一性と、取得から公開までの出自固定を扱う。"""
from dataclasses import dataclass
import copy,hashlib,json,re,stat
from pathlib import Path

SOURCES={'source/input.png','source/isolated.png','analysis/analysis.json','analysis/masks.npz'}
EYE_KEYS={'version','source','models','comfy_code','runtime','workflow','overlay','parameters','inputs','source_region','resolved_workflow'}
HIDDEN_KEYS={'version','job','source','source_region','prepared_input_sha256','models','workflow','runtime','parameters',
             'upscaled','output_adoption','masked_generation_version','mask_sha256','overlay_sha256'}

def sha(data):return hashlib.sha256(data).hexdigest()

def signature(value):
    if not isinstance(value,str) or not re.fullmatch('[0-9a-f]{64}',value):
        raise ValueError('必須SHAが欠落または不正です')
    return value

def semantic_identity(identity,kind,*,requested=False):
    """既知の完全な契約のみ扱い、解析出自2SHA以外の全フィールドを比較する。"""
    expected=EYE_KEYS if kind=='eye' else HIDDEN_KEYS
    if kind not in ('eye','hidden-face','side-ears') or set(identity)!=expected:
        raise ValueError('未対応または不完全なraw生成契約です')
    source=identity['source']
    if not isinstance(source,dict) or set(source)!=SOURCES:raise ValueError('raw生成元4SHAが必要です')
    for value in source.values():signature(value)
    region=identity['source_region']
    if len(region)!=4 or any(type(v)!=int for v in region) or not 0<=region[0]<region[2] or not 0<=region[1]<region[3]:
        raise ValueError('原寸ROIが不正です')
    parameters=identity['parameters']
    common={'steps','seed','fast_disk','prompt'}
    if kind=='eye':
        version=identity['version']
        if type(version)!=int or version not in ((5,) if requested else (1,2,3,4,5)):
            raise ValueError('未対応の閉眼生成版です')
        expected_parameters=common|{'resolution','mask_margin_ratio'}
        if version>=3:expected_parameters.add('mask_core_ratio')
        if set(parameters)!=expected_parameters:raise ValueError('閉眼生成版に必要な推論条件が不足しています')
        expected_inputs={'input.png','left-eye-mask.png','right-eye-mask.png'} if version>=5 else {'input.png','eye-mask.png'}
        if set(identity['inputs'])!=expected_inputs:raise ValueError('閉眼の原画と版別マスクの実入力SHAが必要です')
        for value in identity['inputs'].values():signature(value)
        signature(identity['workflow']);signature(identity['overlay'])
        if not identity['resolved_workflow'] or not identity['comfy_code']:raise ValueError('実workflowとエンジン署名が必要です')
    else:
        latest=3 if kind=='hidden-face' else 2
        supported=(latest,) if requested else tuple(range(1,latest+1))
        if identity['version']!=1 or type(identity['masked_generation_version'])!=int or identity['masked_generation_version'] not in supported or identity['job']!=kind or set(parameters)!=common:
            raise ValueError('未対応の隠れ素材生成版です')
        for name in ('prepared_input_sha256','mask_sha256','overlay_sha256'):signature(identity[name])
        if identity['upscaled'] is not False or identity['output_adoption']!='measured-hidden-or-ear-region-only':
            raise ValueError('rawの原寸採用契約が不正です')
    if not identity['models'] or not identity['workflow'] or not identity['runtime'] or not parameters['prompt']:
        raise ValueError('推論条件が不足しています')
    result=copy.deepcopy(identity)
    result['source']={name:source[name] for name in ('source/input.png','source/isolated.png')}
    return {'semantic_contract_version':1,'kind':kind,'inference':result}

def read_plain(path):
    for item in (path,*path.parents):
        info=item.lstat()
        # Windowsのjunctionを含むreparse pointも、ファイルから全祖先まで拒否する。
        if stat.S_ISLNK(info.st_mode) or getattr(info,'st_file_attributes',0)&stat.FILE_ATTRIBUTE_REPARSE_POINT:
            raise ValueError('rawキャッシュにリンクまたはreparse pointを使用できません')
    return path.read_bytes()

@dataclass(frozen=True)
class RawLease:
    image:Path
    manifest:Path
    image_sha256:str
    manifest_sha256:str
    manifest_bytes:bytes

    def recheck(self):
        if sha(read_plain(self.manifest))!=self.manifest_sha256 or sha(read_plain(self.image))!=self.image_sha256:
            raise ValueError('raw画像または元manifestのSHA不一致があります。取得後の変更を公開しません')

    def image_bytes(self):
        data=read_plain(self.image)
        if sha(data)!=self.image_sha256:raise ValueError('raw画像が元manifestと一致しません')
        return data

    def origin(self):
        return {'manifest_sha256':self.manifest_sha256,'image_sha256':self.image_sha256,
                'generated_from':json.loads(self.manifest_bytes)['identity']}

def open_raw(cache,requested,kind):
    """現在入力を毎回計算した後に呼ぶ。再利用しても元manifestは一切更新しない。"""
    wanted=semantic_identity(requested,kind,requested=True)
    marker=cache/'manifest.json'
    if not marker.exists():return None
    data=read_plain(marker);record=json.loads(data)
    previous=semantic_identity(record['identity'],kind)
    field='edited_sha256' if kind=='eye' else 'image_sha256'
    image_sha=signature(record[field])
    lease=RawLease(cache/'edited.png',marker,image_sha,sha(data),data)
    # 条件不一致でも破損は明示する。壊れたrawを黙って再生成しない。
    lease.recheck()
    if previous!=wanted:return None
    return lease


def pin_generated(cache,manifest_bytes,requested,kind):
    """pendingで確定した期待値を引き継ぎ、公開後の値で基準を更新しない。"""
    record=json.loads(manifest_bytes)
    if record['identity']!=requested:raise ValueError('新規rawの生成出自が今回条件と一致しません')
    semantic_identity(record['identity'],kind,requested=True)
    field='edited_sha256' if kind=='eye' else 'image_sha256'
    lease=RawLease(cache/'edited.png',cache/'manifest.json',signature(record[field]),sha(manifest_bytes),manifest_bytes)
    lease.recheck()
    return lease
