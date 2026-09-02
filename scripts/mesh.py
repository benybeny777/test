"""メッシュ化: 多視点ビューから素体メッシュとテクスチャを作る。

テクスチャは入力イラストの等倍以下に保つ（引き伸ばし拡大の禁止）。大きくしても
細部は増えず、後続の検証と利用者の両方に「高精細だ」と誤認させる。
"""

from __future__ import annotations

import argparse

from _contract import add_common_arguments, not_implemented, report


def main() -> None:
    parser = argparse.ArgumentParser(description="素体メッシュとテクスチャを作る")
    parser.add_argument("--views-dir", required=True, help="多視点ビューの場所")
    parser.add_argument("--output-dir", required=True, help="mesh.glb / texture.png の出力先")
    parser.add_argument("--model", required=True, help="重みのファイル名")
    parser.add_argument("--target-faces", type=int, default=30000, help="目標ポリゴン数")
    add_common_arguments(parser)
    args = parser.parse_args()

    report(0.0, "メッシュ化モデルを読み込み中")
    not_implemented("メッシュ化", f"{args.model} を読み込む推論の実装が要る")


if __name__ == "__main__":
    main()
