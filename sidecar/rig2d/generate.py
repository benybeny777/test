"""意味レイヤーmanifestからlvs-anime25d-v1リグの骨格を作る。"""

from __future__ import annotations

import argparse
import json
import logging
import os
from pathlib import Path


LOGGER = logging.getLogger("local_vtuber_studio.rig2d")
REQUIRED_PARTS = {
    "back_hair",
    "body",
    "left_arm",
    "right_arm",
    "face",
    "front_hair",
    "left_eye_open",
    "right_eye_open",
    "mouth_closed",
    "mouth_open",
}


def create_rig(manifest_path: Path, output_path: Path) -> Path:
    """レイヤーを検証し、T11で変形定義を追加できるリグJSONを保存する。"""

    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    parts = {part["name"]: part for part in manifest.get("parts", [])}
    missing = sorted(REQUIRED_PARTS - parts.keys())
    if missing:
        raise ValueError(f"2.5Dリグの必須部位がありません: {', '.join(missing)}")
    rig = {
        "schema_version": 1,
        "profile": "lvs-anime25d-v1",
        "canvas": manifest["canvas"],
        "layers_manifest": os.path.relpath(
            manifest_path.resolve(), output_path.parent.resolve()
        ),
        "draw_order": [
            part["name"]
            for part in sorted(parts.values(), key=lambda value: value["z_index"])
        ],
        "parameters": {},
    }
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(
        json.dumps(rig, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(json.dumps({"event": "complete", "output": str(output_path)}), flush=True)
    return output_path


def main() -> int:
    """CLIエントリーポイント。"""

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    logging.basicConfig(level=logging.INFO)
    try:
        create_rig(args.manifest, args.output)
    except Exception as error:
        LOGGER.exception("2.5Dリグ作成に失敗しました")
        print(json.dumps({"event": "error", "message": str(error)}), flush=True)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
