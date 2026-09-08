"""最終リグの不変世代とWindows読者leaseを管理する。"""
from contextlib import contextmanager
import errno
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import time
import uuid

GENERATION=re.compile(r'g_[0-9a-f]{32}')
LEASE=re.compile(r'l_[0-9a-f]{32}\.lease')
PART=re.compile(r'[a-z][a-z0-9_]{0,127}')


def safe(path):
    path=Path(path).absolute()
    for node in (path,*path.parents):
        try:metadata=node.lstat()
        except FileNotFoundError:continue
        if stat.S_ISLNK(metadata.st_mode) or getattr(metadata,'st_file_attributes',0)&stat.FILE_ATTRIBUTE_REPARSE_POINT:
            raise ValueError('リンク経由の公開素材を拒否します')
    return path


def sha(path):
    with safe(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()


def rename_generation(source,target):
    """Windowsの短時間占有だけを有限回待ち、世代ディレクトリを原子的に公開する。"""
    for attempt in range(5):
        try:
            source.rename(target)
            return
        except PermissionError:
            if os.name!='nt' or attempt==4:raise
            time.sleep(.02*(attempt+1))


def try_lock(stream):
    import msvcrt
    stream.seek(0)
    try:msvcrt.locking(stream.fileno(),msvcrt.LK_NBLCK,1);return True
    except OSError as error:
        if error.errno in (errno.EACCES,errno.EDEADLK):return False
        raise


def unlock(stream):
    import msvcrt
    stream.seek(0);msvcrt.locking(stream.fileno(),msvcrt.LK_UNLCK,1)


@contextmanager
def guard(character):
    """参照・lease登録・GCの短い操作だけを直列化する。推論中は持たない。"""
    root=safe(character);root.mkdir(parents=True,exist_ok=True)
    with safe(root/'rig-catalog.lock').open('a+b') as stream:
        stream.seek(0,2)
        if not stream.tell():stream.write(b'0');stream.flush()
        deadline=time.monotonic()+10
        while not try_lock(stream):
            if time.monotonic()>deadline:raise TimeoutError('公開参照ロックの待機時間を超えました')
            time.sleep(.02)
        try:yield
        finally:unlock(stream)


def decode_reference(data):
    if len(data)>1024:raise ValueError('公開参照JSONが大きすぎます')
    value=json.loads(data)
    if not isinstance(value,dict) or value.get('schema_version')!=1:raise ValueError('公開参照形式が不正です')
    if not isinstance(value.get('generation'),str) or not GENERATION.fullmatch(value['generation']):raise ValueError('公開世代IDが不正です')
    previous=value.get('previous')
    if previous is not None and (not isinstance(previous,str) or not GENERATION.fullmatch(previous)):raise ValueError('旧世代IDが不正です')
    for key in ('rig_sha256','completion_sha256'):
        if not isinstance(value.get(key),str) or not re.fullmatch(r'[0-9a-f]{64}',value[key]):raise ValueError('公開素材SHAが不正です')
    return value


def reference(character):
    with safe(Path(character)/'rig-current.json').open('rb') as stream:return decode_reference(stream.read(1025))


def generation_path(character,name):
    if not GENERATION.fullmatch(name):raise ValueError('公開世代IDが不正です')
    return safe(Path(character)/'rig-generations'/name)


@contextmanager
def acquire(character):
    """参照を一度読み、全素材の読了まで同じ世代を保護する。"""
    root=safe(character);stream=None;lease=None
    with guard(root):
        selected=reference(root);path=generation_path(root,selected['generation'])
        if sha(path/'rig.json')!=selected['rig_sha256']:raise ValueError('公開リグが改変されています')
        if sha(path/'completion.json')!=selected['completion_sha256']:raise ValueError('公開補完証跡が改変されています')
        leases=safe(root/'rig-leases');leases.mkdir(exist_ok=True)
        lease=leases/('l_'+uuid.uuid4().hex+'.lease')
        stream=lease.open('x+b')
        try:
            stream.write(b'0'+selected['generation'].encode());stream.flush()
            if not try_lock(stream):raise RuntimeError('新規読者leaseを取得できません')
        except BaseException:stream.close();lease.unlink();raise
    try:yield selected,path
    finally:
        try:
            with guard(root):
                try:unlock(stream)
                finally:stream.close()
                lease.unlink()
        finally:
            # catalog取得に失敗してもOSハンドルを残さず、古いleaseは次のGCで回収する。
            if not stream.closed:stream.close()
        collect(root)


def remove_generation(path,parent):
    path=safe(path);parent=safe(parent)
    if path.parent!=parent or not GENERATION.fullmatch(path.name):raise ValueError('削除対象の世代が不正です')
    for child in path.rglob('*'):safe(child)
    shutil.rmtree(path)


def collect(character):
    """current・直前1世代・生存読者を保持し、終了読者だけをOSロックで判定する。"""
    root=safe(character)
    with guard(root):
        selected=reference(root);protected={selected['generation'],selected.get('previous')}
        leases=safe(root/'rig-leases')
        if leases.exists():
            for path in leases.iterdir():
                safe(path)
                if not LEASE.fullmatch(path.name) or not path.is_file():raise ValueError('不正な読者leaseがあります')
                stale=False
                with path.open('r+b') as stream:
                    stale=try_lock(stream)
                    try:
                        if not stale:
                            stream.seek(1);name=stream.read(35).decode('ascii')
                            if not GENERATION.fullmatch(name):raise ValueError('読者leaseの世代が不正です')
                            protected.add(name)
                    finally:
                        if stale:unlock(stream)
                if stale:path.unlink()
        parent=safe(root/'rig-generations')
        for path in parent.iterdir():
            if GENERATION.fullmatch(path.name) and path.name not in protected:remove_generation(path,parent)


def publish(character,build,verify_sources):
    """buildで検証済み素材を作り、既存4SHA等の検査を公開直前に呼ぶ。"""
    root=safe(character);parent=safe(root/'rig-generations');parent.mkdir(parents=True,exist_ok=True)
    # 呼出し側の生成ライターロック取得後だけ実行する。異常終了した同プロトコルのpendingを回収する。
    for stale in parent.iterdir():
        if stale.name.startswith('_pending_') and GENERATION.fullmatch(stale.name[9:]):
            safe(stale)
            for child in stale.rglob('*'):safe(child)
            shutil.rmtree(stale)
    name='g_'+uuid.uuid4().hex;pending=parent/('_pending_'+name);pending.mkdir()
    renamed=False
    try:
        build(pending)
        if not (pending/'rig.json').is_file() or not (pending/'completion.json').is_file():raise ValueError('完成リグと補完証跡が必要です')
        for file in pending.rglob('*'):
            safe(file)
            if file.is_file():
                with file.open('r+b') as stream:stream.flush();os.fsync(stream.fileno())
        rig_sha=sha(pending/'rig.json')
        with guard(root):
            verify_sources()
            previous=reference(root)['generation'] if (root/'rig-current.json').exists() else None
            rename_generation(pending,parent/name);renamed=True
            value={'schema_version':1,'generation':name,'previous':previous,'rig_sha256':rig_sha,'completion_sha256':sha(parent/name/'completion.json')}
            temporary=safe(root/('rig-current-'+uuid.uuid4().hex+'.part'))
            try:
                with temporary.open('xb') as stream:
                    stream.write(json.dumps(value).encode());stream.flush();os.fsync(stream.fileno())
                os.replace(temporary,root/'rig-current.json')
            finally:
                if temporary.exists():temporary.unlink()
        try:collect(root)
        except (OSError,ValueError) as error:value['cleanup_warning']='公開済みですが旧世代の回収に失敗しました: '+str(error)
        return value
    finally:
        if not renamed and pending.exists():
            safe(pending)
            for child in pending.rglob('*'):safe(child)
            shutil.rmtree(pending)
