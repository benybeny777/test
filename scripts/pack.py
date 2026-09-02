"""書き出し: VRM 1.0 として組み立てる。

**書き出してよいかの判断はここでしない。** ボーン・表情・口形・テクスチャ・利用許諾が
揃っているかは Rust 側（`vrm.rs`）が先に検証している。ここがやるのは、検証を通った
材料を glTF へ詰めることだけ。推論はしないので GPU も要らない。
"""

from __future__ import annotations

import argparse

from _contract import add_common_arguments, not_implemented, report


def main() -> None:
    parser = argparse.ArgumentParser(description="VRM 1.0 として書き出す")
    parser.add_argument("--rig", required=True, help="スキニング済みメッシュ（glb）")
    parser.add_argument("--expressions-dir", required=True, help="表情の変形指示の場所")
    parser.add_argument("--visemes-dir", required=True, help="口形の変形指示の場所")
    parser.add_argument("--texture", required=True, help="テクスチャ")
    parser.add_argument("--meta", required=True, help="VRM メタデータ（meta.json）")
    parser.add_argument("--output", required=True, help="書き出す model.vrm")
    add_common_arguments(parser)
    parser.parse_args()

    report(0.0, "材料を読み込み中")
    not_implemented("書き出し", "VRM 1.0 拡張を書き込むライブラリの採用が要る")


if __name__ == "__main__":
    main()
