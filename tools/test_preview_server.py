"""確認サーバーがモデルやプロジェクトファイルを公開しないことを検査する。"""
import tempfile
import unittest
from pathlib import Path
from preview_server import resolve_asset,ROOT


class PreviewServerTests(unittest.TestCase):
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
