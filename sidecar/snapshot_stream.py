"""lease中の一世代を上限付きNDJSONへ逐次変換する。HTTPは呼出し側で扱う。"""
import base64
import hashlib
import json
from pathlib import Path
from PIL import Image
from reference_store import acquire,safe,PART


def records(character,limits):
    """全体bufferを作らず、接続終了時はgenerator.closeでleaseを解放する。"""
    required=('record_bytes','chunk_bytes','part_bytes','total_bytes','parts','dimension')
    if any(type(limits.get(key)) is not int or limits[key]<=0 for key in required):raise ValueError('配信上限が不正です')
    if limits['chunk_bytes']*4//3+512>limits['record_bytes']:raise ValueError('チャンクがJSON上限に入りません')
    def line(value):
        data=json.dumps(value,separators=(',',':')).encode()+b'\n'
        if len(data)>limits['record_bytes']:raise ValueError('JSONレコード上限を超えました')
        return data
    with acquire(character) as (reference,directory):
        with safe(directory/'completion.json').open('rb') as stream:completion_bytes=stream.read(limits['part_bytes']+1)
        if len(completion_bytes)>limits['part_bytes']:raise ValueError('補完証跡が容量上限を超えました')
        if hashlib.sha256(completion_bytes).hexdigest()!=reference['completion_sha256']:raise ValueError('補完証跡が読込中に改変されました')
        expected_hashes=json.loads(completion_bytes)['outputs']
        with safe(directory/'rig.json').open('rb') as stream:rig_bytes=stream.read(limits['part_bytes']+1)
        if len(rig_bytes)>limits['part_bytes']:raise ValueError('rig.jsonが素材上限を超えました')
        rig=json.loads(rig_bytes);layers=rig.get('layers')
        if not isinstance(layers,dict) or not 0<len(layers)<=limits['parts']:raise ValueError('素材数が不正です')
        entries=[('rig',directory/'rig.json','application/json',None)]
        for name,layer in layers.items():
            reserved=name in ('rig','con','prn','aux','nul') or (len(name)==4 and name[:3] in ('com','lpt') and name[3] in '123456789')
            if not PART.fullmatch(name) or reserved or layer.get('url')!=f'/assets/rig2d/parts/{name}.png':raise ValueError('素材名またはURLが不正です')
            path=safe(directory/'parts'/f'{name}.png')
            with Image.open(path) as image:
                width,height=image.size
                if image.format!='PNG' or image.mode!='RGBA' or max(width,height)>limits['dimension']:raise ValueError('素材形式または寸法が不正です')
                box=layer.get('texture_box')
                if not isinstance(box,list) or len(box)!=4 or (box[2]-box[0],box[3]-box[1])!=(width,height):raise ValueError('素材の実寸がリグと一致しません')
            entries.append((name,path,'image/png',[width,height]))
        sizes=[safe(path).stat().st_size for _,path,_,_ in entries]
        if max(sizes)>limits['part_bytes'] or sum(sizes)>limits['total_bytes']:raise ValueError('素材容量上限を超えました')
        yield line({'type':'snapshot','schema_version':1,'generation':reference['generation'],'rig_sha256':reference['rig_sha256'],'parts':len(layers),'total':sum(sizes)})
        for (name,path,mime,size),expected in zip(entries,sizes):
            yield line({'type':'begin','name':name,'mime':mime,'size':size,'length':expected})
            digest=hashlib.sha256();count=0
            with safe(path).open('rb') as stream:
                while data:=stream.read(limits['chunk_bytes']):
                    count+=len(data)
                    if count>expected:raise ValueError('配信中に素材容量が変わりました')
                    digest.update(data)
                    yield line({'type':'chunk','name':name,'data':base64.b64encode(data).decode('ascii')})
            if count!=expected:raise ValueError('配信中に素材が切り詰められました')
            key='rig.json' if name=='rig' else f'parts/{name}.png'
            if digest.hexdigest()!=expected_hashes.get(key):raise ValueError('公開後に素材が改変されています: '+key)
            yield line({'type':'end','name':name,'sha256':digest.hexdigest()})
        yield line({'type':'complete','generation':reference['generation']})
