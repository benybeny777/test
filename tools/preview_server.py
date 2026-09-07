"""確認資産を配信する。素材は変更せず、公開世代の読者leaseと終了時GCだけを管理する。"""
import argparse
import http.server
import json
import os
import re
import sys
from pathlib import Path
from urllib.parse import unquote, urlsplit

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'sidecar'))
from reference_store import acquire,reference,safe
from snapshot_stream import records
CONFIG_PATH=Path(os.environ.get('APPDATA',''))/'com.localvtuberstudio.desktop/config.json'
SNAPSHOT=re.compile(r'/api/characters/(c_[0-9a-f]{12})/snapshot')

def snapshot_limits(root=None,config_path=None):
    root=ROOT if root is None else root
    config_path=CONFIG_PATH if config_path is None else config_path
    # 作業用サーバーも製品 config.rs の既定値を読む。永続ファイルが環境変数より優先する。
    source=(root/'src-tauri/src/config.rs').read_text(encoding='utf-8')
    defaults=dict(re.findall(r'\b(snapshot_[a-z_]+): (\d+),',source))
    config=json.loads(config_path.read_text(encoding='utf-8')).get('display',{}) if config_path.exists() else {}
    limits={}
    for key in ('record_bytes','chunk_bytes','part_bytes','total_bytes','parts','dimension'):
        full='snapshot_'+key
        value=config.get(full,os.environ.get('LVS_DISPLAY_'+full.upper(),defaults[full]))
        if isinstance(value,bool):raise ValueError('配信上限が不正です')
        limits[key]=int(value)
        if str(limits[key])!=str(value) or not 0<limits[key]<=4294967295:raise ValueError('配信上限が不正です')
    if (limits['chunk_bytes']+2)//3*4+512>limits['record_bytes'] or limits['part_bytes']>limits['total_bytes']:raise ValueError('配信上限の大小関係が不正です')
    return limits

STATIC={'/ui/shared/snapshot-sha256.js','/ui/shared/snapshot-client.js','/ui/check.html','/ui/check.js','/ui/qwen-check.html','/ui/qwen-check.js','/ui/shared/avatar-renderer.js','/ui/shared/local-assets.js',
        '/ui/shared/mouth-geometry.js',
        '/ui/shared/eye-geometry.js',
        '/ui/shared/texture-alpha.js',
        '/ui/shared/native-scene-batch.js',
        '/ui/shared/rig-motion.js',
        '/ui/shared/hair-coverage.js',
        '/ui/shared/vendor/three/three.module.min.js','/ui/shared/vendor/three/three.core.min.js'}
ASSET=re.compile(r'/temp/t7-characters/c_[0-9a-f]{12}/(?:character\.json|source/input\.png|rig2d/rig\.json|rig2d/parts/[a-z_]+\.png)')


def normal_characters(root=ROOT):
    """通常補完を完了したキャラの表示名だけを返し、内部設定は列挙しない。"""
    result=[]
    directory=root/'temp/t7-characters'
    for path in sorted(directory.glob('c_*/character.json')):
        if not re.fullmatch(r'c_[0-9a-f]{12}',path.parent.name):continue
        if not path.resolve().is_relative_to(directory.resolve()):continue
        character=json.loads(path.read_text(encoding='utf-8'))
        if not character.get('model',{}).get('rig2d_base'):continue
        if os.path.lexists(path.parent/'rig-current.json'):
            with acquire(path.parent):pass
        else:
            if character.get('stages',{}).get('complete',{}).get('status')!='complete':continue
            if not (path.parent/'rig2d/completion.json').is_file():continue
        result.append({'id':path.parent.name,'name':str(character['displayName'])})
    return result


def comparisons(root=ROOT):
    """比較レポートに記録された画像だけを公開し、ログやモデルは出さない。"""
    rows=[];assets={}
    for report_path in sorted((root/'temp').glob('qwen-eval-*/report.json')):
        directory=report_path.parent.resolve()
        if not directory.is_relative_to((root/'temp').resolve()):continue
        name=directory.name
        if not re.fullmatch(r'qwen-eval-[a-z0-9-]+',name):continue
        try:
            report=json.loads(report_path.read_text(encoding='utf-8'))
            if report.get('mode') not in ('layered','edit'):continue
            row={'name':name,'mode':report['mode'],'status':report['status'],'seconds':report.get('seconds'),'images':[]}
            names=['reference.png']
            for index,image_path in enumerate(report.get('images',[])):
                if not isinstance(image_path,str) or not re.fullmatch(r'output/[a-zA-Z0-9_-]+\.png',image_path):raise ValueError('比較画像パスが不正です')
                names.append(image_path)
            if (directory/'recomposed.png').is_file():names.append('recomposed.png')
            for relative in names:
                path=(directory/relative).resolve()
                if not path.is_relative_to(directory) or not path.is_file():continue
                url='/temp/'+name+'/'+relative;assets[url]=path
                if relative=='reference.png':label='原画（編集していない比較範囲）'
                elif relative=='recomposed.png':label='生成レイヤーの再合成'
                elif report['mode']=='edit':label='Image-Editの補完候補'
                else:
                    number=names.index(relative)-1
                    label='全体再生成（原画ではありません）' if number==0 else f'生成レイヤー {number}'
                row['images'].append({'url':url,'label':label})
            rows.append(row)
        except (OSError,ValueError,TypeError,KeyError):
            rows.append({'name':name,'status':'invalid','images':[]})
    return rows,assets


def resolve_asset(url,root=ROOT):
    """公開対象と実パスの両方を検査し、ディレクトリ一覧やモデルを出さない。"""
    path=unquote(urlsplit(url).path)
    if path=='/':path='/ui/check.html'
    if path.startswith('/temp/qwen-eval-'):
        return comparisons(root)[1].get(path)
    if path not in STATIC and not ASSET.fullmatch(path):return None
    target=(root/path.lstrip('/')).resolve()
    if not target.is_relative_to(root.resolve()) or not target.is_file():return None
    return target


class PreviewHandler(http.server.SimpleHTTPRequestHandler):
    """任意のリポジトリファイルの配信を拒否する。"""
    def do_GET(self):
        matched=SNAPSHOT.fullmatch(urlsplit(self.path).path)
        if matched:
            directory=safe(ROOT/'temp/t7-characters'/matched[1])
            if not os.path.lexists(directory/'rig-current.json'):
                self.send_error(404,'No published generation');return
            stream=None;started=False
            try:
                stream=records(directory,snapshot_limits())
                first=next(stream)
                self.send_response(200);self.send_header('Content-Type','application/x-ndjson');self.send_header('Connection','close');self.end_headers()
                started=True;self.close_connection=True
                self.wfile.write(first)
                for record in stream:self.wfile.write(record)
                self.wfile.flush()
            except (OSError,ValueError,TypeError,KeyError,StopIteration) as error:
                if not started:self.send_error(500,'Published snapshot is invalid')
                else:self.log_error('公開世代の配信失敗: %s',error)
            finally:
                if stream is not None:stream.close()
            return
        super().do_GET()

    def send_head(self):
        if SNAPSHOT.fullmatch(urlsplit(self.path).path):
            self.send_error(405,'Use GET for snapshot');return None
        if urlsplit(self.path).path=='/api/snapshot-config':
            from io import BytesIO
            try:data=json.dumps(snapshot_limits()).encode()
            except (OSError,ValueError,KeyError,TypeError):
                self.send_error(500,'Snapshot limits are invalid');return None
            self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.end_headers()
            return BytesIO(data)
        if urlsplit(self.path).path=='/api/normal-characters':
            from io import BytesIO
            try:
                data=json.dumps({'characters':normal_characters()},ensure_ascii=False).encode('utf-8')
            except (OSError,ValueError,KeyError,TypeError):
                self.send_error(500,'Character index is invalid')
                return None
            self.send_response(200)
            self.send_header('Content-Type','application/json; charset=utf-8')
            self.send_header('Content-Length',str(len(data)))
            self.end_headers()
            return BytesIO(data)
        if urlsplit(self.path).path=='/api/qwen-comparisons':
            from io import BytesIO
            data=json.dumps({'runs':comparisons()[0]},ensure_ascii=False).encode('utf-8')
            self.send_response(200)
            self.send_header('Content-Type','application/json; charset=utf-8')
            self.send_header('Content-Length',str(len(data)))
            self.end_headers()
            return BytesIO(data)
        if resolve_asset(self.path) is None:
            self.send_error(404,'Not found')
            return None
        return super().send_head()

    def translate_path(self,path):
        target=resolve_asset(path)
        if target is None:raise ValueError('公開対象外です')
        return str(target)

    def end_headers(self):
        self.send_header('Cache-Control','no-store')
        self.send_header('X-Content-Type-Options','nosniff')
        super().end_headers()


def main():
    sys.stdout.reconfigure(encoding='utf-8')
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config',type=Path,default=CONFIG_PATH,help='本アプリの永続設定ファイル（都度読込）')
    parser.add_argument('--port',type=int,default=8791)
    parser.add_argument('--lan',action='store_true',help='信頼できるLANからのスマホ確認を許可する')
    args=parser.parse_args()
    globals()['CONFIG_PATH']=args.config
    host='0.0.0.0' if args.lan else '127.0.0.1'
    with http.server.ThreadingHTTPServer((host,args.port),PreviewHandler) as server:
        print(f'確認画面: http://127.0.0.1:{args.port}/ui/check.html （LAN公開: {args.lan}）',flush=True)
        try:server.serve_forever()
        except KeyboardInterrupt:pass


if __name__=='__main__':main()
