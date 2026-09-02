"""多視点生成: 正面から側面・背面のビューを起こす。

**正面の複製で埋めない。** 複製で通すと、できあがるモデルは横から見ると板のように
潰れており、利用者は配信本番で気づくことになる。
"""

from __future__ import annotations

import argparse

from _contract import add_common_arguments, not_implemented, report


def main() -> None:
    parser = argparse.ArgumentParser(description="側面・背面ビューを生成する")
    parser.add_argument("--input", required=True, help="正規化済みの正面イラスト")
    parser.add_argument("--masks-dir", required=True, help="パーツ分割マスクの場所")
    parser.add_argument("--output-dir", required=True, help="ビューの出力先")
    parser.add_argument("--model", required=True, help="重みのファイル名")
    parser.add_argument("--steps", type=int, default=30, help="生成ステップ数")
    parser.add_argument("--views", required=True, help="作るビュー名（カンマ区切り）")
    add_common_arguments(parser)
    args = parser.parse_args()

    report(0.0, "多視点生成モデルを読み込み中")
    not_implemented("多視点生成", f"{args.model} を読み込む推論の実装が要る")


if __name__ == "__main__":
    main()
