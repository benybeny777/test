import argparse
import json
import os
import shutil
import sys
import time
import types
from pathlib import Path

import numpy as np
from PIL import Image


def emit(event: str, **values: object) -> None:
    print(json.dumps({"event": event, **values}, ensure_ascii=False), flush=True)


def validate_full_body_a_pose(alpha: np.ndarray) -> dict[str, float]:
    points = np.argwhere(alpha > 32)
    if points.size == 0:
        raise ValueError("人物の前景を検出できません")
    top, left = points.min(axis=0)
    bottom, right = points.max(axis=0)
    height = bottom - top + 1
    width = right - left + 1
    image_height, image_width = alpha.shape
    height_ratio = height / image_height
    width_ratio = width / image_width
    center_error = abs((left + right) / 2 - image_width / 2) / image_width
    if height_ratio < 0.68:
        raise ValueError("全身が小さすぎます。頭頂から靴先まで写したA/Tポーズ画像が必要です")
    if width_ratio < 0.32:
        raise ValueError("腕の開きを検出できません。A/Tポーズで腕を胴体から離してください")
    if center_error > 0.12:
        raise ValueError("人物が中央から外れています。正面中央のA/Tポーズ画像が必要です")
    upper = alpha[top + int(height * 0.18) : top + int(height * 0.55)] > 32
    lower = alpha[top + int(height * 0.55) : bottom + 1] > 32
    if not upper.any() or not lower.any():
        raise ValueError("上半身または脚を検出できません。全身画像が必要です")
    return {
        "height_ratio": round(float(height_ratio), 4),
        "width_ratio": round(float(width_ratio), 4),
        "center_error": round(float(center_error), 4),
    }


def install_cpu_marching_cubes() -> None:
    import torch
    from skimage.measure import marching_cubes as skimage_marching_cubes

    replacement = types.ModuleType("torchmcubes")

    def marching_cubes(volume: "torch.Tensor", threshold: float):
        vertices, faces, _, _ = skimage_marching_cubes(
            volume.detach().float().cpu().numpy(), level=threshold
        )
        # TripoSR側はtorchmcubesのZYX戻り値をXYZへ反転するため、
        # XYZで返すscikit-image側を先にZYXへ合わせる。
        vertices = vertices[:, [2, 1, 0]]
        return torch.from_numpy(vertices.copy()).float(), torch.from_numpy(faces.copy()).long()

    replacement.marching_cubes = marching_cubes
    sys.modules["torchmcubes"] = replacement


def prepare_isnet_input(img: Image.Image, size: int = 1024) -> tuple[np.ndarray, tuple[int, int, int, int]]:
    rgb = np.asarray(img.convert("RGB"), dtype=np.float32) / 255.0
    original_height, original_width = rgb.shape[:2]
    if original_height > original_width:
        height, width = size, int(size * original_width / original_height)
    else:
        height, width = int(size * original_height / original_width), size
    pad_height, pad_width = size - height, size - width
    resized = np.asarray(
        Image.fromarray((rgb * 255).astype(np.uint8)).resize(
            (width, height), Image.Resampling.BILINEAR
        ),
        dtype=np.float32,
    ) / 255.0
    padded = np.zeros((size, size, 3), dtype=np.float32)
    top, left = pad_height // 2, pad_width // 2
    padded[top : top + height, left : left + width] = resized
    return np.transpose(padded, (2, 0, 1))[None], (top, left, height, width)


class AnimeIsnetSession:
    """rembg-compatible session for the official 1024px anime-seg ONNX model."""

    def __init__(self, model_path: Path):
        import onnxruntime as ort

        self.inner_session = ort.InferenceSession(
            str(model_path), providers=["CPUExecutionProvider"]
        )

    def predict(self, img: Image.Image, *args: object, **kwargs: object) -> list[Image.Image]:
        input_tensor, (top, left, height, width) = prepare_isnet_input(img)
        input_name = self.inner_session.get_inputs()[0].name
        prediction = self.inner_session.run(None, {input_name: input_tensor})[0]
        mask = np.squeeze(prediction).astype(np.float32)
        mask = np.clip(mask[top : top + height, left : left + width], 0.0, 1.0)
        result = Image.fromarray((mask * 255).astype(np.uint8), mode="L")
        return [result.resize(img.size, Image.Resampling.BILINEAR)]


def remove_background(input_path: Path, model_path: Path) -> Image.Image:
    import rembg

    return rembg.remove(
        Image.open(input_path).convert("RGBA"),
        session=AnimeIsnetSession(model_path),
    )


def frame_foreground(image: Image.Image, ratio: float) -> Image.Image:
    if not 0.5 <= ratio <= 0.95:
        raise ValueError("前景比率は0.5以上0.95以下にしてください")
    rgba = np.asarray(image.convert("RGBA"))
    points = np.argwhere(rgba[:, :, 3] > 0)
    if points.size == 0:
        raise ValueError("人物の前景を検出できません")
    top, left = points.min(axis=0)
    bottom, right = points.max(axis=0)
    foreground = rgba[top : bottom + 1, left : right + 1]
    square_size = max(foreground.shape[:2])
    framed_size = int(np.ceil(square_size / ratio))
    framed = np.zeros((framed_size, framed_size, 4), dtype=np.uint8)
    y = (framed_size - foreground.shape[0]) // 2
    x = (framed_size - foreground.shape[1]) // 2
    framed[y : y + foreground.shape[0], x : x + foreground.shape[1]] = foreground
    return Image.fromarray(framed, mode="RGBA")


def rasterize_position_atlas(
    vertices: np.ndarray,
    faces: np.ndarray,
    uvs: np.ndarray,
    resolution: int,
) -> tuple[np.ndarray, np.ndarray]:
    positions = np.zeros((resolution, resolution, 3), dtype=np.float32)
    valid = np.zeros((resolution, resolution), dtype=bool)
    pixel_uvs = np.column_stack(
        (uvs[:, 0] * (resolution - 1), (1.0 - uvs[:, 1]) * (resolution - 1))
    )
    for face in faces:
        triangle = pixel_uvs[face]
        minimum = np.maximum(np.floor(triangle.min(axis=0)).astype(int), 0)
        maximum = np.minimum(
            np.ceil(triangle.max(axis=0)).astype(int), resolution - 1
        )
        if np.any(maximum < minimum):
            continue
        xs = np.arange(minimum[0], maximum[0] + 1)
        ys = np.arange(minimum[1], maximum[1] + 1)
        grid_x, grid_y = np.meshgrid(xs, ys)
        a, b, c = triangle
        denominator = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (
            a[1] - c[1]
        )
        if abs(denominator) < 1e-8:
            continue
        weight_a = (
            (b[1] - c[1]) * (grid_x - c[0])
            + (c[0] - b[0]) * (grid_y - c[1])
        ) / denominator
        weight_b = (
            (c[1] - a[1]) * (grid_x - c[0])
            + (a[0] - c[0]) * (grid_y - c[1])
        ) / denominator
        weight_c = 1.0 - weight_a - weight_b
        inside = (weight_a >= -1e-5) & (weight_b >= -1e-5) & (weight_c >= -1e-5)
        if not inside.any():
            continue
        interpolated = (
            weight_a[..., None] * vertices[face[0]]
            + weight_b[..., None] * vertices[face[1]]
            + weight_c[..., None] * vertices[face[2]]
        )
        region = positions[minimum[1] : maximum[1] + 1, minimum[0] : maximum[0] + 1]
        region[inside] = interpolated[inside]
        valid_region = valid[
            minimum[1] : maximum[1] + 1, minimum[0] : maximum[0] + 1
        ]
        valid_region[inside] = True
    return positions, valid


def bake_uv_texture(
    mesh: object,
    model: object,
    scene_code: object,
    resolution: int,
    chunk_size: int,
) -> tuple[object, Image.Image, int]:
    import torch
    import trimesh
    import xatlas
    from scipy.ndimage import distance_transform_edt

    atlas = xatlas.Atlas()
    atlas.add_mesh(np.asarray(mesh.vertices), np.asarray(mesh.faces))
    options = xatlas.PackOptions()
    options.resolution = resolution
    options.padding = max(2, round(resolution / 256))
    options.bilinear = True
    atlas.generate(pack_options=options)
    vertex_mapping, indices, uvs = atlas[0]
    vertices = np.asarray(mesh.vertices)[vertex_mapping]
    positions, valid = rasterize_position_atlas(vertices, indices, uvs, resolution)
    if not valid.any():
        raise RuntimeError("UVアトラスへ書き込める画素がありません")
    valid_positions = positions[valid]
    colors = np.zeros((len(valid_positions), 3), dtype=np.float32)
    for start in range(0, len(valid_positions), chunk_size):
        points = torch.from_numpy(valid_positions[start : start + chunk_size]).to(
            device=scene_code.device, dtype=scene_code.dtype
        )
        colors[start : start + chunk_size] = (
            model.renderer.query_triplane(model.decoder, points, scene_code)["color"]
            .detach()
            .float()
            .cpu()
            .numpy()
        )
    texture = np.zeros((resolution, resolution, 4), dtype=np.uint8)
    texture[valid, :3] = np.clip(colors * 255.0, 0, 255).astype(np.uint8)
    texture[valid, 3] = 255
    distance, nearest = distance_transform_edt(~valid, return_indices=True)
    padding = (~valid) & (distance <= options.padding)
    texture[padding] = texture[nearest[0][padding], nearest[1][padding]]
    texture_image = Image.fromarray(texture, mode="RGBA")
    material = trimesh.visual.material.PBRMaterial(
        baseColorTexture=texture_image,
        metallicFactor=0.0,
        roughnessFactor=1.0,
    )
    textured = trimesh.Trimesh(
        vertices=vertices,
        faces=indices,
        vertex_normals=np.asarray(mesh.vertex_normals)[vertex_mapping],
        visual=trimesh.visual.TextureVisuals(uv=uvs, material=material),
        process=False,
    )
    return textured, texture_image, int(valid.sum())


def run(args: argparse.Namespace) -> None:
    import torch

    os.environ["HF_HUB_OFFLINE"] = "1"
    os.environ["TRANSFORMERS_OFFLINE"] = "1"
    if not torch.cuda.is_available():
        raise RuntimeError("CUDA対応NVIDIA GPUが必要です。CPUフォールバックはありません")
    input_path = args.input.resolve(strict=True)
    output_dir = args.output.resolve()
    runtime = args.runtime.resolve(strict=True)
    model_dir = args.model.resolve(strict=True)
    dino_config_dir = args.dino_config.resolve(strict=True)
    background_model = args.background_model.resolve(strict=True)
    output_dir.mkdir(parents=True, exist_ok=True)
    source_dir = output_dir / "source"
    source_dir.mkdir(exist_ok=True)
    shutil.copy2(input_path, source_dir / "input.png")

    emit("mesh_progress", stage="background_removal", progress=0.05)
    foreground = remove_background(input_path, background_model)
    alpha = np.asarray(foreground.getchannel("A"))
    pose = validate_full_body_a_pose(alpha)
    foreground.save(output_dir / "foreground.png")
    framed_foreground = frame_foreground(foreground, args.foreground_ratio)
    rgba = np.asarray(framed_foreground).astype(np.float32) / 255.0
    rgb = rgba[:, :, :3] * rgba[:, :, 3:4] + (1.0 - rgba[:, :, 3:4]) * 0.5
    reconstruction_input = Image.fromarray((rgb * 255.0).astype(np.uint8))
    reconstruction_input.save(output_dir / "reconstruction-input.png")

    install_cpu_marching_cubes()
    sys.path.insert(0, str(runtime))
    import huggingface_hub
    from omegaconf import OmegaConf

    def local_hf_file(*, filename: str, **kwargs: object) -> str:
        if filename != "config.json":
            raise RuntimeError(f"未同梱のHugging Faceファイルを要求しました: {filename}")
        return str(dino_config_dir / filename)

    huggingface_hub.hf_hub_download = local_hf_file
    from tsr.system import TSR

    emit("mesh_progress", stage="model_loading", progress=0.15)
    started = time.perf_counter()
    torch.cuda.reset_peak_memory_stats()
    config = OmegaConf.load(model_dir / "config.yaml")
    config.image_tokenizer.pretrained_model_name_or_path = str(dino_config_dir)
    OmegaConf.resolve(config)
    model = TSR(config)
    checkpoint = torch.load(model_dir / "model.ckpt", map_location="cpu")
    model.load_state_dict(checkpoint)
    model.renderer.set_chunk_size(args.chunk_size)
    model.to("cuda:0")
    loaded = time.perf_counter()

    emit("mesh_progress", stage="inference", progress=0.35)
    with torch.inference_mode():
        scene_codes = model([reconstruction_input], device="cuda:0")
    inferred = time.perf_counter()

    emit("mesh_progress", stage="mesh_extraction", progress=0.70)
    meshes = model.extract_mesh(scene_codes, False, resolution=args.mc_resolution)
    mesh = meshes[0]
    extracted = time.perf_counter()
    emit("mesh_progress", stage="texture_baking", progress=0.82)
    mesh, texture, texture_pixels = bake_uv_texture(
        mesh,
        model,
        scene_codes[0],
        args.texture_resolution,
        args.chunk_size,
    )
    texture.save(output_dir / "texture.png")
    mesh_path = output_dir / "mesh.glb"
    mesh.export(mesh_path)
    finished = time.perf_counter()
    if len(mesh.vertices) == 0 or len(mesh.faces) == 0 or not mesh_path.exists():
        raise RuntimeError("空の3Dメッシュが生成されました")
    metrics = {
        "input": str(input_path),
        "output": str(mesh_path),
        "vertices": int(len(mesh.vertices)),
        "faces": int(len(mesh.faces)),
        "bounds": np.asarray(mesh.bounds).round(6).tolist(),
        "pose": pose,
        "model_load_seconds": round(loaded - started, 3),
        "inference_seconds": round(inferred - loaded, 3),
        "extraction_seconds": round(extracted - inferred, 3),
        "texture_baking_seconds": round(finished - extracted, 3),
        "total_seconds": round(finished - started, 3),
        "peak_vram_gb": round(torch.cuda.max_memory_allocated() / 1024**3, 3),
        "mc_resolution": args.mc_resolution,
        "chunk_size": args.chunk_size,
        "foreground_ratio": args.foreground_ratio,
        "texture_resolution": args.texture_resolution,
        "texture_pixels": texture_pixels,
    }
    (output_dir / "metrics.json").write_text(
        json.dumps(metrics, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    emit("mesh_complete", **metrics)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--runtime", type=Path, default=Path("sidecar/runtime/TripoSR"))
    parser.add_argument("--model", type=Path, default=Path("models/triposr"))
    parser.add_argument(
        "--dino-config", type=Path, default=Path("models/dino-vitb16")
    )
    parser.add_argument(
        "--background-model",
        type=Path,
        default=Path("models/rembg/isnetis.onnx"),
    )
    parser.add_argument("--mc-resolution", type=int, default=192)
    parser.add_argument("--chunk-size", type=int, default=4096)
    parser.add_argument("--foreground-ratio", type=float, default=0.85)
    parser.add_argument("--texture-resolution", type=int, default=2048)
    return parser.parse_args()


if __name__ == "__main__":
    try:
        run(parse_args())
    except Exception as error:
        emit("mesh_error", message=str(error))
        raise
