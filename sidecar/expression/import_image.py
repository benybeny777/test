import argparse
import hashlib
import json
import os
import re
from pathlib import Path

import numpy as np
from PIL import Image


ASCII_KEY = re.compile(r"^[a-z0-9][a-z0-9_-]*$")


def load_capture(path: Path) -> Image.Image:
    image = Image.open(path).convert("RGBA")
    if image.size != (1024, 1024):
        raise ValueError(f"1024x1024の画像が必要です: {path}")
    return image


def correlation(reference: np.ndarray, candidate: np.ndarray) -> tuple[float, tuple[int, int]]:
    reference = reference - reference.mean()
    candidate = candidate - candidate.mean()
    denominator = float(np.linalg.norm(reference) * np.linalg.norm(candidate))
    if denominator <= 1e-9:
        raise ValueError("位置合わせに必要な画像特徴がありません")
    values = np.fft.ifft2(np.fft.fft2(reference) * np.conj(np.fft.fft2(candidate))).real
    y, x = np.unravel_index(np.argmax(values), values.shape)
    if x > values.shape[1] // 2:
        x -= values.shape[1]
    if y > values.shape[0] // 2:
        y -= values.shape[0]
    return float(values.max() / denominator), (int(x), int(y))


def inspect_import(
    neutral: Image.Image,
    imported: Image.Image,
    alignment_tolerance: float = 12.0,
    color_tolerance: float = 0.08,
) -> dict:
    neutral_small = np.asarray(neutral.convert("L").resize((256, 256)), dtype=np.float32)
    imported_small = np.asarray(imported.convert("L").resize((256, 256)), dtype=np.float32)
    normal_score, shift = correlation(neutral_small, imported_small)
    mirrored_score, _ = correlation(neutral_small, np.fliplr(imported_small))
    shift_pixels = [value * 4 for value in shift]
    color_delta = float(
        np.mean(
            np.abs(
                np.asarray(neutral.convert("RGB"), dtype=np.float32)
                - np.asarray(imported.convert("RGB"), dtype=np.float32)
            )
        )
        / 255.0
    )
    warnings = []
    if max(abs(value) for value in shift_pixels) > alignment_tolerance:
        warnings.append("中立キャプチャからの位置ずれを検出しました")
    if mirrored_score > normal_score + 0.03:
        warnings.append("左右反転の可能性を検出しました")
    if color_delta > color_tolerance:
        warnings.append("中立キャプチャとの色差が大きすぎます")
    return {
        "alignment_shift_pixels": shift_pixels,
        "alignment_score": round(normal_score, 6),
        "mirrored_score": round(mirrored_score, 6),
        "color_delta": round(color_delta, 6),
        "warnings": warnings,
    }


def cache_signature(neutral: Image.Image, imported: Image.Image, settings: dict) -> str:
    digest = hashlib.sha256()
    digest.update(neutral.tobytes())
    digest.update(imported.tobytes())
    digest.update(json.dumps(settings, sort_keys=True).encode("utf-8"))
    return digest.hexdigest()


def save_atomic(image: Image.Image, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.tmp")
    image.save(temporary, format="PNG")
    with temporary.open("r+b") as file:
        os.fsync(file.fileno())
    os.replace(temporary, path)


def write_json_atomic(value: dict, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.tmp")
    with temporary.open("w", encoding="utf-8") as file:
        json.dump(value, file, ensure_ascii=False, indent=2)
        file.flush()
        os.fsync(file.fileno())
    os.replace(temporary, path)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--neutral", required=True, type=Path)
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--kind", required=True, choices=("eyes", "mouth"))
    parser.add_argument("--key", required=True)
    parser.add_argument("--alignment-tolerance", type=float, default=12.0)
    parser.add_argument("--color-tolerance", type=float, default=0.08)
    args = parser.parse_args()
    if not ASCII_KEY.fullmatch(args.key):
        raise ValueError("keyはASCII小文字・数字・ハイフン・アンダースコアだけにしてください")
    neutral = load_capture(args.neutral)
    imported = load_capture(args.input)
    settings = {
        "alignment_tolerance": args.alignment_tolerance,
        "color_tolerance": args.color_tolerance,
    }
    result = inspect_import(neutral, imported, **settings)
    result.update(
        {
            "kind": args.kind,
            "key": args.key,
            "source": str(args.input.resolve()),
            "cache_signature": cache_signature(neutral, imported, settings),
        }
    )
    save_atomic(neutral, args.output / "neutral.png")
    save_atomic(imported, args.output / args.kind / f"{args.key}.png")
    manifest = args.output / args.kind / f"{args.key}.import.json"
    write_json_atomic(result, manifest)
    print(json.dumps({"event": "expression_imported", **result}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
