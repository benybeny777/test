"""意味レイヤーmanifestからlvs-anime25d-v1リグの骨格を作る。"""

from __future__ import annotations

import argparse
import json
import logging
import os
import shutil
from pathlib import Path


LOGGER = logging.getLogger("local_vtuber_studio.rig2d")
REQUIRED_PARTS = {
    "neutral",
    "back_hair",
    "body",
    "left_arm",
    "right_arm",
    "face",
    "front_hair",
    "side_hair",
    "left_eye_open",
    "right_eye_open",
    "left_eye_closed",
    "right_eye_closed",
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
    output_parts = output_path.parent / "parts"
    output_parts.mkdir(parents=True, exist_ok=True)
    for name, part in parts.items():
        source = manifest_path.parent / part["path"]
        if not source.is_file():
            raise ValueError(f"2.5D部位画像がありません: {name}")
        shutil.copy2(source, output_parts / f"{name}.png")
    rig = {
        "schema_version": 2,
        "profile": "lvs-anime25d-v1",
        "material_readiness": manifest.get("material_readiness", {
            "status": "incomplete", "note": "素材分割の充足が未検証です"}),
        "canvas": manifest["canvas"],
        "layers_manifest": os.path.relpath(
            manifest_path.resolve(), output_path.parent.resolve()
        ),
        "draw_order": [
            part["name"]
            for part in sorted(parts.values(), key=lambda value: value["z_index"])
        ],
        "layers": {
            name: {
                "url": f"/assets/rig2d/parts/{name}.png",
                "pivot": parts[name]["pivot"],
                "bbox": parts[name].get("bbox"),
                "feature_box": parts[name].get("feature_box"),
                "line_color": parts[name].get("line_color"),
                "z_index": parts[name]["z_index"],
            }
            for name in parts
        },
        "parameters": {
            "EyeLOpen": {"min": 0.0, "default": 1.0, "max": 1.0},
            "EyeROpen": {"min": 0.0, "default": 1.0, "max": 1.0},
            "MouthOpenY": {"min": 0.0, "default": 0.0, "max": 1.0},
            "MouthForm": {"min": -1.0, "default": 0.0, "max": 1.0},
            "AngleX": {"min": -30.0, "default": 0.0, "max": 30.0},
            "AngleY": {"min": -30.0, "default": 0.0, "max": 30.0},
            "AngleZ": {"min": -15.0, "default": 0.0, "max": 15.0},
            "BodyAngleZ": {"min": -10.0, "default": 0.0, "max": 10.0},
            "ArmLAngle": {"min": -45.0, "default": 0.0, "max": 45.0},
            "ArmRAngle": {"min": -45.0, "default": 0.0, "max": 45.0},
            "HairSway": {"min": -1.0, "default": 0.0, "max": 1.0},
        },
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
