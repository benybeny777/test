"""生成途中の例外と切替中断から前回成果物を保護する。"""

from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from output_transaction import directory_output


class OutputTransactionTests(unittest.TestCase):
    def setUp(self):
        base = Path(__file__).resolve().parents[1] / "temp" / "tests"
        base.mkdir(parents=True, exist_ok=True)
        self.sandbox = tempfile.TemporaryDirectory(dir=base)
        self.addCleanup(self.sandbox.cleanup)
        self.destination = Path(self.sandbox.name) / "rig2d"
        self.destination.mkdir()
        (self.destination / "rig.json").write_bytes(b"old")

    def test_generation_failure_preserves_previous_bytes(self):
        with self.assertRaisesRegex(RuntimeError, "生成失敗"):
            with directory_output(self.destination) as pending:
                (pending / "rig.json").write_bytes(b"half")
                raise RuntimeError("生成失敗")
        self.assertEqual((self.destination / "rig.json").read_bytes(), b"old")
        self.assertFalse((self.destination.parent / "temp/rig2d-pending").exists())

    def test_success_replaces_whole_generation_and_cleans_backup(self):
        (self.destination / "obsolete.png").write_bytes(b"old")
        with directory_output(self.destination) as pending:
            (pending / "rig.json").write_bytes(b"new")
        self.assertEqual((self.destination / "rig.json").read_bytes(), b"new")
        self.assertFalse((self.destination / "obsolete.png").exists())
        self.assertFalse((self.destination.parent / "temp/rig2d-previous").exists())

    def test_publication_failure_rolls_back(self):
        rename = Path.rename
        def fail_pending(path, target):
            if path.name == "rig2d-pending":
                raise OSError("公開失敗")
            return rename(path, target)
        with self.assertRaisesRegex(OSError, "公開失敗"):
            with patch.object(Path, "rename", fail_pending):
                with directory_output(self.destination) as pending:
                    (pending / "rig.json").write_bytes(b"new")
        self.assertEqual((self.destination / "rig.json").read_bytes(), b"old")

    def test_restart_recovers_interrupted_rename(self):
        workspace = self.destination.parent / "temp"
        workspace.mkdir()
        self.destination.rename(workspace / "rig2d-previous")
        with self.assertRaisesRegex(RuntimeError, "次の生成失敗"):
            with directory_output(self.destination):
                self.assertEqual((self.destination / "rig.json").read_bytes(), b"old")
                raise RuntimeError("次の生成失敗")
        self.assertEqual((self.destination / "rig.json").read_bytes(), b"old")

    def test_concurrent_writer_is_rejected(self):
        with directory_output(self.destination):
            with self.assertRaises(OSError):
                with directory_output(self.destination):
                    self.fail("同じ生成先を二重に開けてはいけない")


if __name__ == "__main__":
    unittest.main()
