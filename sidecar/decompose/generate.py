"""SAM 2.1の候補マスクから2.5Dキャラクターレイヤーを生成する。"""

from __future__ import annotations

import argparse
import json
import logging
import os
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable

import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from output_transaction import directory_output


LOGGER = logging.getLogger("local_vtuber_studio.decompose")
SCHEMA_VERSION = 2


@dataclass(frozen=True)
class PartSpec:
    """1つの意味レイヤーの識別子、重なり順、回転中心を表す。"""

    name: str
    z_index: int
    pivot: tuple[float, float]


PART_SPECS = (
    PartSpec("neutral", 0, (0.50, 0.50)),
    PartSpec("back_hair", 10, (0.50, 0.18)),
    PartSpec("body", 20, (0.50, 0.55)),
    PartSpec("left_arm", 30, (0.30, 0.40)),
    PartSpec("right_arm", 31, (0.70, 0.40)),
    PartSpec("face", 40, (0.50, 0.19)),
    PartSpec("front_hair", 50, (0.50, 0.13)),
    PartSpec("side_hair", 51, (0.50, 0.18)),
    PartSpec("left_eye_open", 60, (0.44, 0.18)),
    PartSpec("right_eye_open", 61, (0.56, 0.18)),
    PartSpec("left_eye_closed", 62, (0.44, 0.18)),
    PartSpec("right_eye_closed", 63, (0.56, 0.18)),
    PartSpec("mouth_closed", 70, (0.50, 0.24)),
    PartSpec("mouth_open", 71, (0.50, 0.24)),
    PartSpec("left_eye_base", 58, (0.44, 0.18)),
    PartSpec("right_eye_base", 59, (0.56, 0.18)),
    PartSpec("left_eyelid_upper", 64, (0.44, 0.18)),
    PartSpec("right_eyelid_upper", 65, (0.56, 0.18)),
)


def _emit(event: str, **values: object) -> None:
    """Rust側が読める1行JSON進捗を標準出力へ送る。"""

    print(json.dumps({"event": event, **values}, ensure_ascii=False), flush=True)


def _bbox(mask: np.ndarray) -> tuple[int, int, int, int]:
    points = np.argwhere(mask)
    if points.size == 0:
        return (0, 0, 0, 0)
    top, left = points.min(axis=0)
    bottom, right = points.max(axis=0) + 1
    return (int(left), int(top), int(right), int(bottom))


def _rect_mask(
    shape: tuple[int, int], box: tuple[float, float, float, float]
) -> np.ndarray:
    height, width = shape
    left, top, right, bottom = box
    result = np.zeros(shape, dtype=bool)
    result[
        max(0, int(top * height)) : min(height, int(np.ceil(bottom * height))),
        max(0, int(left * width)) : min(width, int(np.ceil(right * width))),
    ] = True
    return result


def _normalise_candidates(
    candidates: Iterable[np.ndarray], subject: np.ndarray
) -> list[np.ndarray]:
    from scipy import ndimage

    result: list[np.ndarray] = []
    subject_area = int(subject.sum())
    for candidate in candidates:
        mask = np.asarray(candidate, dtype=bool)
        if mask.shape != subject.shape:
            raise ValueError("候補マスクの寸法が入力画像と一致しません")
        clipped = mask & subject
        labels, component_count = ndimage.label(clipped)
        if component_count:
            component_sizes = np.bincount(labels.ravel())
            component_sizes[0] = 0
            clipped = labels == int(component_sizes.argmax())
        area = int(clipped.sum())
        if area < max(4, subject_area // 1000) or area > subject_area * 0.98:
            continue
        if any(np.array_equal(clipped, existing) for existing in result):
            continue
        result.append(clipped)
    return result


def _best_candidate(
    candidates: list[np.ndarray], target: np.ndarray, subject: np.ndarray
) -> tuple[np.ndarray, float]:
    target = target & subject
    target_area = int(target.sum())
    if target_area == 0:
        return target, 0.0
    best = target
    best_score = 0.0
    for candidate in candidates:
        intersection = int((candidate & target).sum())
        if intersection == 0:
            continue
        coverage = intersection / target_area
        precision = intersection / max(1, int(candidate.sum()))
        score = 2.0 * coverage * precision / max(1e-6, coverage + precision)
        if score > best_score:
            best = candidate
            best_score = score
    # 過大なSAMマスクより安全な幾何フォールバックを優先する。
    if best_score < 0.35:
        return target, 0.0
    return best, best_score


def classify_semantic_parts(
    subject: np.ndarray, candidates: Iterable[np.ndarray]
) -> tuple[dict[str, np.ndarray], dict[str, float]]:
    """候補マスクを決定的な部位名へ割り当てる。

    SAMが適切な候補を返さない部位は、被写体シルエット内の保守的な
    領域へフォールバックする。すべての結果は必ず被写体内に収める。
    """

    subject = np.asarray(subject, dtype=bool)
    if subject.ndim != 2 or not subject.any():
        raise ValueError("被写体のアルファ領域が空です")
    left, top, right, bottom = _bbox(subject)
    height = bottom - top
    width = right - left
    if height < 8 or width < 8:
        raise ValueError("被写体が小さすぎて部位分解できません")

    normalised = _normalise_candidates(candidates, subject)
    canvas_h, canvas_w = subject.shape

    def relative_box(x1: float, y1: float, x2: float, y2: float) -> np.ndarray:
        absolute = (
            (left + x1 * width) / canvas_w,
            (top + y1 * height) / canvas_h,
            (left + x2 * width) / canvas_w,
            (top + y2 * height) / canvas_h,
        )
        return _rect_mask(subject.shape, absolute) & subject

    # 全身入力では頭が被写体高の上側約2割に収まる。最初にSAM候補から
    # 頭・髪・上衣を確定し、顔内の小部位は頭の実測bboxを基準に切り出す。
    face, face_score = _best_candidate(
        normalised, relative_box(0.37, 0.01, 0.63, 0.19), subject
    )
    hair, hair_score = _best_candidate(
        normalised, relative_box(0.36, 0.00, 0.64, 0.085), subject
    )
    torso, torso_score = _best_candidate(
        normalised, relative_box(0.12, 0.15, 0.88, 0.48), subject
    )
    if min(face_score, hair_score, torso_score) <= 0.0:
        raise ValueError(f"SAM 2.1候補から部位を識別できません: 顔={face_score:.3f}, 髪={hair_score:.3f}, 上半身={torso_score:.3f}")

    # 頭部の高スコア候補が髪そのものの場合、顔として二重採用しない。
    # 髪の実測範囲内で肌側の連結領域を求め、後段の目口検査も必須にする。
    from scipy import ndimage
    hl, ht, hr, hb = _bbox(hair)
    head_region = np.zeros(subject.shape, dtype=bool)
    head_region[ht:min(hb, ht+round((hr-hl)*1.3)), hl:hr] = True
    skin_candidates = head_region & subject & ~hair
    labels, count = ndimage.label(skin_candidates)
    if count == 0:
        raise ValueError("髪と区別できる顔の連結領域がありません")
    sizes = np.bincount(labels.ravel()); sizes[0] = 0
    face = labels == sizes.argmax()

    face_left, face_top, face_right, face_bottom = _bbox(face)
    face_height = face_bottom - face_top
    face_width = face_right - face_left

    def face_box(x1: float, y1: float, x2: float, y2: float) -> np.ndarray:
        result = np.zeros(subject.shape, dtype=bool)
        result[
            int(face_top + y1 * face_height) : int(face_top + y2 * face_height),
            int(face_left + x1 * face_width) : int(face_left + x2 * face_width),
        ] = True
        return result & face

    center_x = left + width / 2
    arm_gap = width * 0.12
    x_grid = np.broadcast_to(np.arange(canvas_w), subject.shape)
    arm_height = relative_box(0.0, 0.14, 1.0, 0.60)
    left_arm = arm_height & (x_grid < center_x - arm_gap)
    right_arm = arm_height & (x_grid > center_x + arm_gap)
    hair_top = _bbox(hair)[1]
    hair_bottom = _bbox(hair)[3]
    y_grid = np.broadcast_to(np.arange(canvas_h)[:, None], subject.shape)
    front_hair = hair & (y_grid < hair_top + 0.72 * (hair_bottom - hair_top))
    side_hair = hair & (y_grid >= hair_top + 0.38 * (hair_bottom - hair_top))

    parts = {
        "neutral": subject,
        "back_hair": hair,
        "body": subject & ~face & ~hair & ~left_arm & ~right_arm,
        "left_arm": left_arm,
        "right_arm": right_arm,
        "face": face,
        "front_hair": front_hair,
        "side_hair": side_hair,
        "left_eye_open": face_box(0.15, 0.23, 0.45, 0.38),
        "right_eye_open": face_box(0.55, 0.23, 0.85, 0.38),
        "left_eye_closed": face_box(0.15, 0.23, 0.45, 0.38),
        "right_eye_closed": face_box(0.55, 0.23, 0.85, 0.38),
        "mouth_closed": face_box(0.34, 0.48, 0.66, 0.62),
        # T11でMouthOpenY変形を適用する同一座標の変形元。ここでは原画を
        # 描き替えず、開閉双方の必須スロットを確定する。
        "mouth_open": face_box(0.34, 0.48, 0.66, 0.62),
    }
    scores = {
        "neutral": 1.0,
        "back_hair": hair_score,
        "body": 1.0,
        "left_arm": torso_score,
        "right_arm": torso_score,
        "face": face_score,
        "front_hair": hair_score,
        "side_hair": hair_score,
        "left_eye_open": face_score,
        "right_eye_open": face_score,
        "left_eye_closed": face_score,
        "right_eye_closed": face_score,
        "mouth_closed": face_score,
        "mouth_open": face_score,
    }

    missing = [name for name, mask in parts.items() if not mask.any()]
    if missing:
        raise ValueError(f"必須部位を生成できません: {', '.join(missing)}")
    return parts, scores




def load_candidate_masks(directory: Path, size: tuple[int, int]) -> list[np.ndarray]:
    """テストまたは再実行用ディレクトリから候補マスクを読む。"""

    if not directory.is_dir():
        raise ValueError(f"候補マスクディレクトリがありません: {directory}")
    masks: list[np.ndarray] = []
    for path in sorted(directory.glob("*.png")):
        with Image.open(path) as opened:
            if opened.size != size:
                raise ValueError(f"候補マスクの寸法が一致しません: {path.name}")
            masks.append(np.asarray(opened.convert("L"), dtype=np.uint8) > 127)
    if not masks:
        raise ValueError("候補マスクが1枚もありません")
    return masks


def generate_sam2_masks(image: Image.Image, model_path: Path, points_per_batch: int, pred_iou_threshold: float, stability_threshold: float) -> list[np.ndarray]:
    """ローカルのSAM 2.1モデルをCUDAで実行して候補マスクを返す。"""

    if not model_path.is_dir():
        raise ValueError(f"SAM 2.1モデルがありません: {model_path}")
    os.environ["HF_HUB_OFFLINE"] = "1"
    os.environ["TRANSFORMERS_OFFLINE"] = "1"
    try:
        import torch
        from transformers import pipeline
    except ImportError as error:
        raise RuntimeError("SAM 2.1実行依存がインストールされていません") from error
    if not torch.cuda.is_available():
        raise RuntimeError("SAM 2.1の実行にはCUDA対応GPUが必要です")
    generator = pipeline(
        "mask-generation",
        model=str(model_path),
        device=0,
        dtype=torch.float32,
        local_files_only=True,
    )
    # 透明画素に残ったRGBをモデルへ見せない。原画そのものは変更しない。
    rgb = Image.alpha_composite(Image.new("RGBA", image.size, (128,128,128,255)), image.convert("RGBA")).convert("RGB")
    options = dict(points_per_batch=points_per_batch, pred_iou_thresh=pred_iou_threshold, stability_score_thresh=stability_threshold)
    output: dict[str, Any] = generator(rgb, **options)
    masks = [np.asarray(mask, dtype=bool) for mask in output.get("masks", [])]
    # 全身の疎な探索で小さい頭部を取りこぼさないよう、同じモデルで追加解析する。
    alpha = np.asarray(image.convert("RGBA"))[:,:,3]
    l,t,r,b = _bbox(alpha >= 128)
    head_bottom = min(b, t + round((b-t)*.30))
    output = generator(rgb.crop((l,t,r,head_bottom)), **options)
    for small in output.get("masks", []):
        mask = np.zeros(alpha.shape, dtype=bool)
        mask[t:head_bottom,l:r] = np.asarray(small, dtype=bool)
        masks.append(mask)
    return masks


def _save_psd(layer_paths: list[tuple[str, Path]], destination: Path) -> None:
    try:
        from psd_tools import PSDImage
    except ImportError as error:
        raise RuntimeError("PSD出力依存 psd-tools がインストールされていません") from error
    with Image.open(layer_paths[0][1]) as opened:
        psd = PSDImage.new(mode="RGB", size=opened.size, depth=8, color=0)
    for name, path in layer_paths:
        with Image.open(path) as opened:
            rgba = opened.convert("RGBA")
            bounds = rgba.getchannel("A").getbbox()
            if bounds is None:
                layer_image = Image.new("RGBA", (1, 1), (0, 0, 0, 0))
                offset = (0, 0)
            else:
                layer_image = rgba.crop(bounds)
                offset = bounds[:2]
            psd.create_pixel_layer(
                layer_image, name=name, top=offset[1], left=offset[0]
            )
    destination.parent.mkdir(parents=True, exist_ok=True)
    psd.save(destination)


def decompose_image(
    input_path: Path, output_dir: Path, **options: Any,
) -> Path:
    """全素材とmanifestの生成後に公開し、失敗時は前回の出力を保つ。"""
    with directory_output(output_dir) as pending:
        _decompose_image(input_path, pending, published_dir=output_dir, **options)
    result = output_dir / "manifest.json"
    _emit("complete", manifest=str(result))
    return result


def _decompose_image(
    input_path: Path,
    output_dir: Path,
    *,
    published_dir: Path,
    model_path: Path | None = None,
    candidate_masks_dir: Path | None = None,
    keep_candidates: bool = False,
    points_per_batch: int = 8,
    pred_iou_threshold: float = .7,
    stability_threshold: float = .85,
    grounding_model: Path | None = None,
    grounding_threshold: float = .20,
) -> Path:
    """入力画像を意味レイヤーへ分解し、manifestのパスを返す。"""

    with Image.open(input_path) as opened:
        source = opened.convert("RGBA")
    rgba = np.asarray(source, dtype=np.uint8)
    subject = rgba[:, :, 3] >= 128
    if not subject.any():
        raise ValueError("入力画像に不透明な被写体がありません")
    grounded_result = None
    if grounding_model is not None:
        if model_path is None:
            raise ValueError("SAM2モデルの指定が必要です")
        from grounded import analyse_cached
        grounded_result = analyse_cached(source, grounding_model, model_path, grounding_threshold,
                                         _emit, published_dir.parent / 'analysis')
        candidates = list(grounded_result[0].values())
        method = "grounding-dino-base+sam2.1"
    elif candidate_masks_dir is not None:
        candidates = load_candidate_masks(candidate_masks_dir, source.size)
        method = "fixture"
    elif model_path is not None:
        _emit("progress", stage="sam2", progress=0.1)
        if not 1 <= points_per_batch <= 64:
            raise ValueError("SAMバッチ数は1〜64で指定してください")
        candidates = generate_sam2_masks(source, model_path, points_per_batch, pred_iou_threshold, stability_threshold)
        method = "sam2.1-hiera-tiny"
    else:
        raise ValueError("--model または --candidate-masks のどちらかが必要です")
    if not candidates:
        raise ValueError("SAM 2.1が候補マスクを生成しませんでした")
    if keep_candidates:
        diagnostics = output_dir / "candidates"
        diagnostics.mkdir(parents=True, exist_ok=True)
        for index, candidate in enumerate(candidates):
            mask = np.asarray(candidate, dtype=bool) & subject
            Image.fromarray((mask * 255).astype(np.uint8), mode="L").save(
                diagnostics / f"{index:03}.png"
            )

    from features import locate_features, expression_patch
    from lips import trace_lip_seam
    if grounded_result is None:
        parts, scores = classify_semantic_parts(subject, candidates)
        features = locate_features(rgba, parts['face'])
    else:
        masks, features, analysis = grounded_result
        if 'left_arm' not in masks or 'right_arm' not in masks:
            raise ValueError("左右の腕を識別できません。未検出を固定領域で補いません")
        hair=masks['hair'];face=masks['face'];neck=masks['neck'] & ~face
        parts={'neutral':subject,'back_hair':hair,'face':face,
               'left_arm':masks['left_arm'],'right_arm':masks['right_arm'],
               'front_hair':hair,'side_hair':hair,'neck':neck}
        parts['body']=subject & ~face & ~hair & ~neck & ~parts['left_arm'] & ~parts['right_arm']
        if 'collar' in masks:
            collar=masks['collar'] & ~face & ~neck & ~hair
            if collar.any():parts['collar']=collar
        if any(not mask.any() for mask in parts.values()):
            raise ValueError("意味分離後に空の必須素材が残りました")
        scores={spec.name:0.0 for spec in PART_SPECS}
        scores.update(neck=analysis['selected']['neck']['score'],collar=analysis['selected'].get('collar',{}).get('score',0.0))
        output_dir.mkdir(parents=True,exist_ok=True)
        (output_dir/'analysis.json').write_text(json.dumps(analysis,ensure_ascii=False,indent=2),encoding='utf-8')
    for feature, box in features.items():
        mask = np.zeros(subject.shape, dtype=bool)
        l,t,r,b = box
        mask[t:b,l:r] = subject[t:b,l:r]
        suffixes = ('closed','open')
        for suffix in suffixes:
            parts[f'{feature}_{suffix}'] = mask
    lip_seam = trace_lip_seam(rgba, features['mouth'])
    eye_materials={}
    for feature in ('left_eye','right_eye'):
        closed,bounds,color,base,upper,aperture=expression_patch(
            rgba,features[feature],'eye',parts['face'] if grounded_result is not None else None,
            masks[feature] if grounded_result is not None else None,
            masks['hair'] if grounded_result is not None else None,with_parts=True)
        names=(feature+'_closed',feature+'_base',feature.replace('_eye','_eyelid_upper'))
        for name,material in zip(names,(closed,base,upper)):
            parts[name]=material[:,:,3]>0
            scores[name]=scores.get(feature+'_closed',0.)
            eye_materials[name]=(material,list(bounds),color,aperture,feature)
        if grounded_result is not None:
            parts[feature+'_open']=masks[feature] & ~masks['hair'] & subject
            from eyelids import partition_eye
            iris,remainder=partition_eye(parts[feature+'_open'],masks[feature+'_iris'])
            parts[feature+'_iris']=iris
            parts[feature+'_remainder']=remainder
            scores[feature+'_iris']=analysis['selected'][feature+'_iris']['score']
            scores[feature+'_remainder']=scores[feature+'_iris']
    parts_dir = output_dir / "parts"
    parts_dir.mkdir(parents=True, exist_ok=True)
    layer_paths: list[tuple[str, Path]] = []
    manifest_parts: list[dict[str, object]] = []
    specs = list(PART_SPECS)
    specs.extend(PartSpec(name,z,(.5,.5)) for name,z in [('neck',35),('collar',36)] if name in parts)
    if grounded_result is not None:
        specs.extend(PartSpec(side+'_eye_'+kind,z,(.5,.5)) for side in ('left','right')
                     for kind,z in [('remainder',60),('iris',61)])
    for spec in specs:
        mask = parts[spec.name]
        box = list(_bbox(mask))
        color = None
        aperture=None
        feature_name=spec.name.rsplit('_',1)[0]
        if spec.name in eye_materials:
            layer,box,color,aperture,feature_name=eye_materials[spec.name]
        elif spec.name == 'mouth_open':
            layer, box, color = expression_patch(
                rgba, box, 'mouth' if spec.name=='mouth_open' else 'eye',
                parts['face'] if grounded_result is not None else None,
                masks[spec.name.rsplit('_',1)[0]] if grounded_result is not None else None,
                masks['hair'] if grounded_result is not None else None)
        elif spec.name == 'mouth_closed':
            # 唇の周囲を含む原画を残す。変形しても端の肌は元の位置へ固定する。
            x0,y0,x1,y1=features['mouth']
            margin=max(3,round((x1-x0)*.6))
            box=[max(0,x0-margin),max(0,y0-margin),min(source.width,x1+margin),min(source.height,y1+margin)]
            layer=np.zeros_like(rgba)
            l,t,r,b=box
            layer[t:b,l:r]=rgba[t:b,l:r]
        else:
            layer = rgba.copy()
            if spec.name != "neutral":
                layer[~mask] = 0
        # 回転・口の変形中心はキャンバス比率ではなく実測領域から求める。
        center = [(box[0]+box[2])/2/source.width,(box[1]+box[3])/2/source.height]
        path = parts_dir / f"{spec.name}.png"
        Image.fromarray(layer, mode="RGBA").save(path)
        layer_paths.append((spec.name, path))
        manifest_parts.append(
            {
                "name": spec.name,
                "path": f"parts/{path.name}",
                "bbox": box,
                "z_index": spec.z_index,
                "pivot": center,
                "feature_box": features.get(feature_name),
                "line_color": color,
                "lip_seam": lip_seam if spec.name == 'mouth_closed' else None,
                "eye_aperture": aperture,
                "candidate_score": round(scores[spec.name], 6),
                "generated_variant": spec.name
                in set(eye_materials) | {"mouth_open"},
            }
        )
    psd_path = output_dir / "source.psd"
    _save_psd(layer_paths, psd_path)
    manifest = {
        "schema_version": SCHEMA_VERSION,
        "method": method,
        "canvas": {"width": source.width, "height": source.height},
        "source": os.path.relpath(input_path.resolve(), published_dir.resolve()),
        "psd": psd_path.name,
        "parts": manifest_parts,
        "features": features,
    }
    from materials import assess_materials
    manifest["material_readiness"] = assess_materials(manifest_parts)
    manifest_path = output_dir / "manifest.json"
    manifest_path.write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    return manifest_path


def build_parser() -> argparse.ArgumentParser:
    """コマンドライン引数パーサーを構築する。"""

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--model", type=Path)
    source.add_argument("--candidate-masks", type=Path)
    parser.add_argument("--keep-candidates", action="store_true")
    parser.add_argument("--points-per-batch", type=int, default=8)
    parser.add_argument("--pred-iou-threshold", type=float, default=.7)
    parser.add_argument("--stability-threshold", type=float, default=.85)
    parser.add_argument("--grounding-model", type=Path)
    parser.add_argument("--grounding-threshold", type=float, default=.20)
    return parser


def main() -> int:
    """CLIエントリーポイント。"""

    logging.basicConfig(level=logging.INFO)
    args = build_parser().parse_args()
    try:
        decompose_image(
            args.input,
            args.output,
            model_path=args.model,
            candidate_masks_dir=args.candidate_masks,
            keep_candidates=args.keep_candidates,
            points_per_batch=args.points_per_batch,
            pred_iou_threshold=args.pred_iou_threshold,
            stability_threshold=args.stability_threshold,
            grounding_model=args.grounding_model,
            grounding_threshold=args.grounding_threshold,
        )
    except Exception as error:
        LOGGER.exception("レイヤー分解に失敗しました")
        _emit("error", message=str(error))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
