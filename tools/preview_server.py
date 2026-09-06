"""確認画面と検証用キャラクターだけを配信する読み取り専用サーバー。"""
import argparse
import http.server
import json
import re
import sys
from pathlib import Path
from urllib.parse import unquote, urlsplit

ROOT=Path(__file__).resolve().parents[1]
STATIC={'/ui/check.html','/ui/check.js','/ui/qwen-check.html','/ui/qwen-check.js','/ui/shared/avatar-renderer.js','/ui/shared/local-assets.js',
        '/ui/shared/mouth-geometry.js',
        '/ui/shared/eye-geometry.js',
        '/ui/shared/texture-alpha.js',
        '/ui/shared/rig-motion.js',
        '/ui/shared/vendor/three/three.module.min.js','/ui/shared/vendor/three/three.core.min.js'}
ASSET=re.compile(r'/temp/t7-characters/c_[0-9a-f]{12}/(?:character\.json|source/input\.png|rig2d/rig\.json|rig2d/parts/[a-z_]+\.png)')


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
    def send_head(self):
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
    parser.add_argument('--port',type=int,default=8791)
    parser.add_argument('--lan',action='store_true',help='信頼できるLANからのスマホ確認を許可する')
    args=parser.parse_args()
    host='0.0.0.0' if args.lan else '127.0.0.1'
    with http.server.ThreadingHTTPServer((host,args.port),PreviewHandler) as server:
        print(f'確認画面: http://127.0.0.1:{args.port}/ui/check.html （LAN公開: {args.lan}）',flush=True)
        try:server.serve_forever()
        except KeyboardInterrupt:pass


if __name__=='__main__':main()
