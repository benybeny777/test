"""粗い分割が細分化済みと誤表示されないことを確認する。"""
import unittest
from materials import assess_materials, CORE_ROLES


class MaterialTests(unittest.TestCase):
    def test_legacy_parts_are_not_complete(self):
        result = assess_materials([{"name": "face", "path": "face.png"}])
        self.assertEqual(result['status'], 'incomplete')
        self.assertEqual(set(result['missing_roles']), CORE_ROLES)

    def test_names_without_assets_do_not_pass(self):
        result = assess_materials([{"role": role} for role in CORE_ROLES])
        self.assertEqual(result['status'], 'incomplete')
        self.assertTrue(result['issues'])

    def test_iris_requires_sclera_clip(self):
        result = assess_materials([{'role': 'left_eye_iris', 'path': 'iris.png',
            'source_region': [1,2,3,4], 'status': 'verified'}])
        self.assertTrue(any('クリッピング' in item for item in result['issues']))


if __name__ == '__main__':
    unittest.main()
