"""工程スクリプトの共通部分。

引数の受け取り、進捗の出し方、失敗の返し方をここへ集約する。各スクリプトで書き分けると、
進捗の書式や終了コードの扱いがばらつき、Rust 側の読み取りが工程ごとに変わってしまう。
"""

from __future__ import annotations

import argparse
import sys


def add_common_arguments(parser: argparse.ArgumentParser) -> None:
    """すべての工程が受け取る引数。"""
    parser.add_argument(
        "--device",
        default="auto",
        choices=["auto", "cpu", "cuda"],
        help="実行デバイス。cuda は CUDA 対応 NVIDIA GPU がある場合だけ指定する。",
    )


def report(ratio: float, message: str) -> None:
    """進捗を出す。Rust 側はこの書式の行だけを進捗として拾う。"""
    ratio = min(max(float(ratio), 0.0), 1.0)
    print(f"PROGRESS {ratio:.3f} {message}", flush=True)


def not_implemented(stage: str, needs: str) -> "NoReturn":  # noqa: F821
    """未実装であることを、終了コードと理由で返す。

    白紙や複製の成果物を書いて成功にしない。欠損したまま最後まで通ると、利用者は
    配信本番になって初めて気づくことになる。
    """
    print(
        f"工程「{stage}」の実装がまだありません（{needs}）。\n"
        "採用するモデルとライブラリを決めてから実装します。着手条件は docs/TASKS.md を参照してください。",
        file=sys.stderr,
    )
    raise SystemExit(2)
