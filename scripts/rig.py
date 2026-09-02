"""ボーン設定: 与えられたボーン位置でメッシュへスキニングする。

**ボーンの位置はここで決めない。** 位置は Rust 側（`pipeline/stages/rig.rs`）が
パーツマスクから測って `bones.json` に書いてある。ここがやるのは、その位置に合わせて
頂点をボーンへ従わせることだけ。推論はしないので GPU も要らない。
"""

from __future__ import annotations

import argparse

from _contract import add_common_arguments, not_implemented, report


def main() -> None:
    parser = argparse.ArgumentParser(description="メッシュへスキニングする")
    parser.add_argument("--mesh", required=True, help="素体メッシュ（glb）")
    parser.add_argument("--bones", required=True, help="ボーン定義（bones.json）")
    parser.add_argument("--output-dir", required=True, help="rig.glb の出力先")
    add_common_arguments(parser)
    parser.parse_args()

    report(0.0, "メッシュとボーン定義を読み込み中")
    not_implemented("ボーン設定", "glTF のスキニングを書き込むライブラリの採用が要る")


if __name__ == "__main__":
    main()
