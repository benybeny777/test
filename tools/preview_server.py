"""確認画面と検証用キャラクターだけを配信する読み取り専用サーバー。"""
import argparse
import http.server
import re
import sys
from pathlib import Path
from urllib.parse import unquote, urlsplit

ROOT=Path(__file__).resolve().parents[1]
STATIC={'/ui/check.html','/ui/check.js','/ui/shared/avatar-renderer.js','/ui/shared/local-assets.js',
        '/ui/shared/mouth-geometry.js',
        '/ui/shared/eye-geometry.js',
        '/ui/shared/texture-alpha.js',
        '/ui/shared/vendor/three/three.module.min.js','/ui/shared/vendor/three/three.core.min.js'}
ASSET=re.compile(r'/temp/t7-characters/c_[0-9a-f]{12}/(?:character\.json|source/input\.png|rig2d/rig\.json|rig2d/parts/[a-z_]+\.png)')


def resolve_asset(url,root=ROOT):
    """公開対象と実パスの両方を検査し、ディレクトリ一覧やモデルを出さない。"""
    path=unquote(urlsplit(url).path)
    if path=='/':path='/ui/check.html'
    if path not in STATIC and not ASSET.fullmatch(path):return None
    target=(root/path.lstrip('/')).resolve()
    if not target.is_relative_to(root.resolve()) or not target.is_file():return None
    return target


class PreviewHandler(http.server.SimpleHTTPRequestHandler):
    """任意のリポジトリファイルの配信を拒否する。"""
    def send_head(self):
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
