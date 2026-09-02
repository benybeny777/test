"""パーツ分割: 前景と体パーツ（頭・髪・胴・腕・脚）のマスクを作る。

入力は下ごしらえ済みの正面イラスト1枚。出力は `<出力先>/<パーツ>.png`。
推論は利用者のPC内だけで行う（外部の推論APIは呼ばない）。
"""

from __future__ import annotations

import argparse

from _contract import add_common_arguments, not_implemented, report


def main() -> None:
    parser = argparse.ArgumentParser(description="体パーツの分割マスクを作る")
    parser.add_argument("--input", required=True, help="正規化済みの正面イラスト")
    parser.add_argument("--output-dir", required=True, help="マスクの出力先")
    parser.add_argument("--model", required=True, help="重みのファイル名")
    parser.add_argument("--parts", required=True, help="作るパーツ名（カンマ区切り）")
    add_common_arguments(parser)
    args = parser.parse_args()

    report(0.0, "分割モデルを読み込み中")
    not_implemented("パーツ分割", f"{args.model} を読み込む推論の実装が要る")


if __name__ == "__main__":
    main()
