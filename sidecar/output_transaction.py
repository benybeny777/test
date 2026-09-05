"""前回の生成物を保護し、完成したディレクトリだけを公開する。"""

from contextlib import contextmanager
import os
from pathlib import Path
import shutil


def _remove_owned(path: Path, workspace: Path) -> None:
    """生成専用の一時ディレクトリ以外は削除しない。"""
    if path.is_symlink() or path.resolve().parent != workspace.resolve():
        raise ValueError(f"生成一時領域の格納先が不正です: {path}")
    if path.exists():
        shutil.rmtree(path)


@contextmanager
def directory_output(destination: Path):
    """生成失敗時は旧版を維持し、切替中断時は次回開始時に復旧する。"""
    destination = destination.absolute()
    if destination.is_symlink() or destination.resolve() != destination:
        raise ValueError("出力先にリンクを使用できません")
    workspace = destination.parent / "temp"
    workspace.mkdir(parents=True, exist_ok=True)
    if workspace.is_symlink() or workspace.resolve() != workspace:
        raise ValueError("生成一時領域にリンクを使用できません")
    pending = workspace / f"{destination.name}-pending"
    previous = workspace / f"{destination.name}-previous"
    # OSのファイルロックは異常終了でも解放される。ロックファイルは再利用する。
    with (workspace / f"{destination.name}.lock").open("a+b") as lock:
        lock.seek(0, 2)
        if lock.tell() == 0:
            lock.write(b"0")
            lock.flush()
        lock.seek(0)
        if os.name == "nt":
            import msvcrt
            msvcrt.locking(lock.fileno(), msvcrt.LK_NBLCK, 1)
        else:
            import fcntl
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        try:
            if previous.is_symlink() or previous.resolve().parent != workspace.resolve():
                raise ValueError("復旧対象が生成一時領域の外です")
            if previous.exists():
                if destination.exists():
                    _remove_owned(previous, workspace)
                else:
                    previous.rename(destination)
            _remove_owned(pending, workspace)
            pending.mkdir()
            try:
                yield pending
                for entry in pending.rglob("*"):
                    if entry.is_symlink():
                        raise ValueError("生成物にリンクを含められません")
                    if entry.is_file():
                        with entry.open("r+b") as handle:
                            os.fsync(handle.fileno())
                if destination.exists():
                    destination.rename(previous)
                try:
                    pending.rename(destination)
                except BaseException:
                    if previous.exists():
                        previous.rename(destination)
                    raise
                _remove_owned(previous, workspace)
            finally:
                _remove_owned(pending, workspace)
        finally:
            lock.seek(0)
            if os.name == "nt":
                msvcrt.locking(lock.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                fcntl.flock(lock, fcntl.LOCK_UN)
