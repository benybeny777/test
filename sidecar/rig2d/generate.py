"""意味レイヤーmanifestからlvs-anime25d-v1リグの骨格を作る。"""

from __future__ import annotations

import argparse
import json
import logging
import os
import sys
import math
from pathlib import Path
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from output_transaction import directory_output


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
    "left_eye_base", "right_eye_base", "left_eyelid_upper", "right_eyelid_upper",
    "left_eye_iris", "right_eye_iris", "left_eye_remainder", "right_eye_remainder",
    "left_eye_backplate", "right_eye_backplate",
    "scene_torso", "scene_left_arm", "scene_right_arm", "scene_neck", "scene_face", "scene_hair",
}


def validate_layers(manifest: dict, directory: Path) -> dict:
    """コピー前に全素材を検査する。読めることと見た目の合格は分ける。"""
    if manifest.get("schema_version") != 2:
        raise ValueError("未対応のレイヤー形式です。分解工程を再実行してください")
    canvas = manifest.get("canvas", {})
    size = (canvas.get("width"), canvas.get("height"))
    if any(type(value) is not int or value <= 0 for value in size):
        raise ValueError("キャンバス寸法が不正です")
    parts = {}
    for part in manifest.get("parts", []):
        name = part.get("name")
        if name not in REQUIRED_PARTS | {"neck", "collar", "scene_residual", "scene_collar"} or name in parts:
            raise ValueError(f"部位名が不正または重複しています: {name}")
        parts[name] = part
    missing = sorted(REQUIRED_PARTS - parts.keys())
    if missing:
        raise ValueError(f"2.5Dリグの必須部位がありません: {', '.join(missing)}")
    for name, part in parts.items():
        relative = part.get("path")
        if not isinstance(relative, str) or not relative:
            raise ValueError(f"部位画像のパスがありません: {name}")
        source = (directory / relative).resolve()
        if not source.is_relative_to(directory.resolve()):
            raise ValueError(f"部位画像がレイヤーディレクトリ外です: {name}")
        with Image.open(source) as opened:
            opened.load()
            if opened.format != "PNG" or opened.mode != "RGBA" or opened.size != size:
                raise ValueError(f"部位画像の形式または寸法が不正です: {name}")
            if opened.getchannel("A").getbbox() is None:
                raise ValueError(f"部位画像が全透明です: {name}")
        for field, length in (("pivot", 2), ("bbox", 4)):
            values = part.get(field)
            if not isinstance(values, list) or len(values) != length or any(
                isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value)
                for value in values
            ):
                raise ValueError(f"部位の{field}が不正です: {name}")
        left, top, right, bottom = part["bbox"]
        if not (0 <= left < right <= size[0] and 0 <= top < bottom <= size[1]):
            raise ValueError(f"部位のbboxがキャンバス外または空です: {name}")
        if any(not 0 <= value <= 1 for value in part["pivot"]):
            raise ValueError(f"部位のpivotが範囲外です: {name}")
        if type(part.get("z_index")) is not int:
            raise ValueError(f"部位の重なり順が不正です: {name}")
    seam = parts['mouth_closed'].get('lip_seam')
    if not isinstance(seam, list) or len(seam) < 3:
        raise ValueError("唇の分割境界がありません。分解工程を再実行してください")
    previous_x = -1
    for point in seam:
        if not isinstance(point, list) or len(point) != 2 or any(type(v) not in (int,float) or not math.isfinite(v) for v in point):
            raise ValueError("唇の分割境界が不正です")
        x,y=point
        if not previous_x < x <= size[0] or not 0 <= y <= size[1]:
            raise ValueError("唇の分割境界が交差またはキャンバス外です")
        previous_x=x
    for side in ('left', 'right'):
        expected = None
        for suffix in ('eye_base', 'eyelid_upper', 'eye_closed'):
            aperture = parts[f'{side}_{suffix}'].get('eye_aperture')
            if not isinstance(aperture, list) or len(aperture) < 2:
                raise ValueError('まぶたの実測境界がありません。分解工程を再実行してください')
            previous_x = -1
            for point in aperture:
                if not isinstance(point, list) or len(point) != 4 or any(
                    type(v) not in (int, float) or not math.isfinite(v) for v in point
                ):
                    raise ValueError('まぶたの実測境界が不正です')
                x, top, bottom, closed = point
                if not (0 <= x <= size[0] and previous_x < x and
                        0 <= top <= bottom <= size[1] and 0 <= closed <= size[1]):
                    raise ValueError('まぶたの実測境界が交差またはキャンバス外です')
                previous_x = x
            if expected is not None and aperture != expected:
                raise ValueError('まぶた素材間の実測境界が一致しません')
            expected = aperture
    graph=manifest.get('scene_graph')
    if not isinstance(graph,list) or not graph:
        raise ValueError('独立部位の構造がありません。分解工程を再実行してください')
    roles={item.get('role') for item in graph if isinstance(item,dict)}
    expected={name.removeprefix('scene_') for name in parts if name.startswith('scene_')}
    if roles!=expected or len(roles)!=len(graph):raise ValueError('独立部位の構造が重複または欠落しています')
    parents={}
    for item in graph:
        role=item['role'];parent=item.get('parent')
        if item.get('layer')!='scene_'+role or (parent is not None and parent not in roles):
            raise ValueError('独立部位の素材または親が不正です')
        if type(item.get('owned_pixels')) is not int or not 0<item['owned_pixels']<=size[0]*size[1]:
            raise ValueError('独立部位の所有画素数が不正です')
        parents[role]=parent
    for role in parents:
        seen=set();current=role
        while current is not None:
            if current in seen:raise ValueError('独立部位の親子関係が循環しています')
            seen.add(current);current=parents[current]
    return parts


def create_rig(manifest_path: Path, output_path: Path) -> Path:
    """完成したリグだけを公開し、失敗した再生成で前回の素材を消さない。"""
    with directory_output(output_path.parent) as pending:
        _create_rig(manifest_path, pending / output_path.name, output_path.parent)
    print(json.dumps({"event": "complete", "output": str(output_path)}), flush=True)
    return output_path


def _create_rig(manifest_path: Path, output_path: Path, published_dir: Path) -> None:
    """レイヤーを検証し、T11で変形定義を追加できるリグJSONを保存する。"""

    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    parts = validate_layers(manifest, manifest_path.parent)
    output_parts = output_path.parent / "parts"
    output_parts.mkdir(parents=True, exist_ok=True)
    for name, part in parts.items():
        source = manifest_path.parent / part["path"]
        if not source.is_file():
            raise ValueError(f"2.5D部位画像がありません: {name}")
        with Image.open(source) as opened:
            bounds=opened.getchannel('A').getbbox()
            if bounds is None:
                raise ValueError(f"2.5D部位画像が全透明です: {name}")
            from texture import bleed_transparent_rgb
            bleed_transparent_rgb(opened.crop(bounds)).save(output_parts / f"{name}.png")
            part['texture_box']=list(bounds)
    rig = {
        "schema_version": 3,
        "lip_rig_version": 1,
        "eye_rig_version": 3,
        "scene_graph_version": 1,
        "scene_graph": manifest.get('scene_graph',[]),
        "profile": "lvs-anime25d-v1",
        "material_readiness": manifest.get("material_readiness", {
            "status": "incomplete", "note": "素材分割の充足が未検証です"}),
        "canvas": manifest["canvas"],
        "layers_manifest": os.path.relpath(
            manifest_path.resolve(), published_dir.resolve()
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
                "texture_box": parts[name]['texture_box'],
                "feature_box": parts[name].get("feature_box"),
                "line_color": parts[name].get("line_color"),
                "lip_seam": parts[name].get("lip_seam"),
                "eye_aperture": parts[name].get("eye_aperture"),
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
