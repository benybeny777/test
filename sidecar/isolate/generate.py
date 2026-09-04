"""入力画像から被写体を切り出し、元キャンバスの透過PNGを作る。"""

from __future__ import annotations

import argparse
import json
import logging
from pathlib import Path

import numpy as np
from PIL import Image


LOGGER = logging.getLogger("local_vtuber_studio.isolate")


def prepare_isnet_input(
    image: Image.Image, size: int = 1024
) -> tuple[np.ndarray, tuple[int, int, int, int]]:
    """アスペクト比を保ったIS-Net入力と復元位置を返す。"""

    rgb = np.asarray(image.convert("RGB"), dtype=np.float32) / 255.0
    original_height, original_width = rgb.shape[:2]
    if original_height > original_width:
        height, width = size, int(size * original_width / original_height)
    else:
        height, width = int(size * original_height / original_width), size
    top = (size - height) // 2
    left = (size - width) // 2
    resized = np.asarray(
        Image.fromarray((rgb * 255).astype(np.uint8)).resize(
            (width, height), Image.Resampling.BILINEAR
        ),
        dtype=np.float32,
    ) / 255.0
    padded = np.zeros((size, size, 3), dtype=np.float32)
    padded[top : top + height, left : left + width] = resized
    return np.transpose(padded, (2, 0, 1))[None], (top, left, height, width)


class AnimeIsnetSession:
    """固定anime-segモデルを使うrembg互換セッション。"""

    def __init__(self, model_path: Path):
        import onnxruntime as ort

        if not model_path.is_file():
            raise ValueError(f"背景除去モデルがありません: {model_path}")
        self.inner_session = ort.InferenceSession(
            str(model_path), providers=["CPUExecutionProvider"]
        )

    def predict(self, image: Image.Image, *_: object, **__: object) -> list[Image.Image]:
        """入力画像と同じ寸法のアルファマスクを返す。"""

        tensor, (top, left, height, width) = prepare_isnet_input(image)
        input_name = self.inner_session.get_inputs()[0].name
        prediction = self.inner_session.run(None, {input_name: tensor})[0]
        mask = np.squeeze(prediction).astype(np.float32)
        mask = np.clip(mask[top : top + height, left : left + width], 0.0, 1.0)
        result = Image.fromarray((mask * 255).astype(np.uint8), mode="L")
        return [result.resize(image.size, Image.Resampling.BILINEAR)]


def isolate_image(input_path: Path, output_path: Path, model_path: Path) -> Path:
    """背景除去を実行し、元画像と同じ寸法のRGBA画像を保存する。"""

    import rembg

    with Image.open(input_path) as opened:
        source = opened.convert("RGBA")
    isolated = rembg.remove(source, session=AnimeIsnetSession(model_path)).convert("RGBA")
    if isolated.size != source.size:
        raise RuntimeError("背景除去後のキャンバス寸法が変化しました")
    if not np.asarray(isolated, dtype=np.uint8)[:, :, 3].any():
        raise ValueError("人物の前景を検出できません")
    output_path.parent.mkdir(parents=True, exist_ok=True)
    isolated.save(output_path)
    print(
        json.dumps({"event": "complete", "output": str(output_path)}, ensure_ascii=False),
        flush=True,
    )
    return output_path


def main() -> int:
    """CLIエントリーポイント。"""

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--model", type=Path, required=True)
    args = parser.parse_args()
    logging.basicConfig(level=logging.INFO)
    try:
        isolate_image(args.input, args.output, args.model)
    except Exception as error:
        LOGGER.exception("背景除去に失敗しました")
        print(json.dumps({"event": "error", "message": str(error)}), flush=True)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
