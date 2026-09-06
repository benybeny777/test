"""確認サーバーがモデルやプロジェクトファイルを公開しないことを検査する。"""
import tempfile
import unittest
import re
import posixpath
from pathlib import Path
from preview_server import resolve_asset,ROOT,STATIC


class PreviewServerTests(unittest.TestCase):
    def test_static_module_imports_are_served(self):
        for url in STATIC:
            if not url.endswith('.js'):continue
            source=(ROOT/url.lstrip('/')).read_text(encoding='utf-8')
            for imported in re.findall(r"\bfrom\s*['\"]([^'\"]+)['\"]",source):
                if not imported.startswith('.'):continue
                dependency=posixpath.normpath(posixpath.join(posixpath.dirname(url),imported.split('?')[0]))
                self.assertIn(dependency,STATIC,f'{url} の依存が非公開です: {dependency}')

    def test_only_allowlisted_assets_are_public(self):
        parent=ROOT/'temp/tests';parent.mkdir(parents=True,exist_ok=True)
        with tempfile.TemporaryDirectory(dir=parent) as folder:
            root=Path(folder);(root/'ui').mkdir()
            (root/'ui/check.html').write_text('preview',encoding='utf-8')
            (root/'AGENTS.md').write_text('private',encoding='utf-8')
            self.assertEqual(resolve_asset('/?v=3',root),root/'ui/check.html')
            self.assertIsNone(resolve_asset('/AGENTS.md',root))
            self.assertIsNone(resolve_asset('/ui/../AGENTS.md',root))
            self.assertIsNone(resolve_asset('/ui/%2e%2e/AGENTS.md',root))
            self.assertIsNone(resolve_asset('/models/model.safetensors',root))
            self.assertIsNone(resolve_asset('/temp/t7-characters/',root))


if __name__=='__main__':unittest.main()
