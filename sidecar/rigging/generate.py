import argparse
import io
import json
import math
import struct
import sys
import time
from dataclasses import dataclass
from pathlib import Path

import numpy as np
import trimesh
from PIL import Image
from scipy import sparse


REQUIRED_BONES = (
    "hips",
    "spine",
    "head",
    "leftUpperArm",
    "leftLowerArm",
    "leftHand",
    "rightUpperArm",
    "rightLowerArm",
    "rightHand",
    "leftUpperLeg",
    "leftLowerLeg",
    "leftFoot",
    "rightUpperLeg",
    "rightLowerLeg",
    "rightFoot",
)


@dataclass(frozen=True)
class Bone:
    name: str
    parent: str | None
    position: np.ndarray
    end: np.ndarray


def emit(event: str, **values: object) -> None:
    print(json.dumps({"event": event, **values}, ensure_ascii=False), flush=True)


def load_textured_mesh(path: Path) -> tuple[trimesh.Trimesh, Image.Image]:
    loaded = trimesh.load(path, process=False)
    geometries = list(loaded.geometry.values()) if isinstance(loaded, trimesh.Scene) else [loaded]
    if len(geometries) != 1:
        raise ValueError("リギング入力は単一メッシュのGLBにしてください")
    mesh = geometries[0]
    if not isinstance(mesh, trimesh.Trimesh) or len(mesh.vertices) == 0:
        raise ValueError("有効な三角形メッシュを読み込めません")
    uv = getattr(mesh.visual, "uv", None)
    material = getattr(mesh.visual, "material", None)
    texture = getattr(material, "baseColorTexture", None)
    if uv is None or texture is None:
        raise ValueError("2048px UVテクスチャ付きGLBが必要です")
    mesh.visual.uv = np.asarray(uv, dtype=np.float32)
    return mesh, texture.convert("RGBA")


def normalize_to_vrm_axes(vertices: np.ndarray) -> tuple[np.ndarray, float]:
    source = np.asarray(vertices, dtype=np.float32)
    source_height = float(np.ptp(source[:, 2]))
    if source_height <= 1e-6:
        raise ValueError("メッシュの全高を算出できません")
    scale = 1.7 / source_height
    normalized = np.column_stack((source[:, 1], source[:, 2], source[:, 0])) * scale
    normalized[:, 1] -= normalized[:, 1].min()
    return normalized.astype(np.float32), scale


def validate_humanoid_pose(vertices: np.ndarray) -> dict[str, float]:
    height = float(np.ptp(vertices[:, 1]))
    width = float(np.ptp(vertices[:, 0]))
    depth = float(np.ptp(vertices[:, 2]))
    if height <= 0 or width / height < 0.32:
        raise ValueError("A/Tポーズの腕幅を検出できません")
    if depth / height > 0.5:
        raise ValueError("正面向きの直立した人型メッシュが必要です")
    hand_heights = []
    for sign in (1.0, -1.0):
        side = vertices[vertices[:, 0] * sign > width * 0.38]
        if len(side) < 8:
            raise ValueError("左右両方の腕と手を検出できません")
        extreme = side[side[:, 0] * sign >= np.quantile(side[:, 0] * sign, 0.88)]
        hand_heights.append(float(np.median(extreme[:, 1]) / height))
    if any(not math.isfinite(value) or value < 0.38 or value > 0.86 for value in hand_heights):
        raise ValueError("腕が胴体に沿っているか、A/Tポーズの範囲外です")
    if abs(hand_heights[0] - hand_heights[1]) > 0.1:
        raise ValueError("左右の腕位置が非対称です。正面A/Tポーズが必要です")
    return {
        "height_m": round(height, 4),
        "width_ratio": round(width / height, 4),
        "depth_ratio": round(depth / height, 4),
        "left_hand_height_ratio": round(hand_heights[0], 4),
        "right_hand_height_ratio": round(hand_heights[1], 4),
    }


def estimate_bones(vertices: np.ndarray) -> list[Bone]:
    height = float(vertices[:, 1].max())
    width = float(np.ptp(vertices[:, 0]))
    center_depth = float(np.median(vertices[:, 2]))
    shoulder_y = height * 0.78
    shoulder_band = vertices[
        (vertices[:, 1] > height * 0.73) & (vertices[:, 1] < height * 0.82)
    ]
    shoulder_x = float(np.quantile(np.abs(shoulder_band[:, 0]), 0.42))
    shoulder_x = float(np.clip(shoulder_x, height * 0.085, height * 0.16))
    leg_band = vertices[
        (vertices[:, 1] > height * 0.38) & (vertices[:, 1] < height * 0.5)
    ]
    leg_x = float(np.quantile(np.abs(leg_band[:, 0]), 0.55))
    leg_x = float(np.clip(leg_x, height * 0.055, height * 0.11))

    hips = np.array([0.0, height * 0.52, center_depth], dtype=np.float32)
    spine = np.array([0.0, height * 0.60, center_depth], dtype=np.float32)
    chest = np.array([0.0, height * 0.71, center_depth], dtype=np.float32)
    neck = np.array([0.0, height * 0.84, center_depth], dtype=np.float32)
    head = np.array([0.0, height * 0.875, center_depth], dtype=np.float32)
    head_top = np.array([0.0, height * 0.985, center_depth], dtype=np.float32)
    bones = [
        Bone("hips", None, hips, spine),
        Bone("spine", "hips", spine, chest),
        Bone("chest", "spine", chest, neck),
        Bone("neck", "chest", neck, head),
        Bone("head", "neck", head, head_top),
    ]

    for prefix, sign in (("left", 1.0), ("right", -1.0)):
        side = vertices[vertices[:, 0] * sign > width * 0.25]
        extreme = side[side[:, 0] * sign >= np.quantile(side[:, 0] * sign, 0.94)]
        hand = np.median(extreme, axis=0).astype(np.float32)
        hand[0] = sign * abs(hand[0])
        shoulder = np.array([sign * shoulder_x, shoulder_y, center_depth], dtype=np.float32)
        elbow = shoulder + (hand - shoulder) * 0.52
        wrist = shoulder + (hand - shoulder) * 0.88
        shoulder_root = np.array(
            [sign * shoulder_x * 0.55, shoulder_y, center_depth], dtype=np.float32
        )
        bones.extend(
            [
                Bone(f"{prefix}Shoulder", "chest", shoulder_root, shoulder),
                Bone(f"{prefix}UpperArm", f"{prefix}Shoulder", shoulder, elbow),
                Bone(f"{prefix}LowerArm", f"{prefix}UpperArm", elbow, wrist),
                Bone(f"{prefix}Hand", f"{prefix}LowerArm", wrist, hand),
            ]
        )

    for prefix, sign in (("left", 1.0), ("right", -1.0)):
        foot_vertices = vertices[
            (vertices[:, 0] * sign > 0) & (vertices[:, 1] < height * 0.12)
        ]
        foot_depth = (
            float(np.median(foot_vertices[:, 2])) if len(foot_vertices) else center_depth
        )
        upper = np.array([sign * leg_x, height * 0.51, center_depth], dtype=np.float32)
        knee = np.array([sign * leg_x, height * 0.285, center_depth], dtype=np.float32)
        ankle = np.array([sign * leg_x, height * 0.065, foot_depth], dtype=np.float32)
        toe = np.array([sign * leg_x, height * 0.025, foot_depth + height * 0.055], dtype=np.float32)
        bones.extend(
            [
                Bone(f"{prefix}UpperLeg", "hips", upper, knee),
                Bone(f"{prefix}LowerLeg", f"{prefix}UpperLeg", knee, ankle),
                Bone(f"{prefix}Foot", f"{prefix}LowerLeg", ankle, toe),
            ]
        )
    return bones


def point_segment_distance(points: np.ndarray, start: np.ndarray, end: np.ndarray) -> np.ndarray:
    segment = end - start
    denominator = float(np.dot(segment, segment))
    if denominator <= 1e-12:
        return np.linalg.norm(points - start, axis=1)
    amount = np.clip(((points - start) @ segment) / denominator, 0.0, 1.0)
    closest = start + amount[:, None] * segment
    return np.linalg.norm(points - closest, axis=1)


def vertex_adjacency(vertex_count: int, faces: np.ndarray) -> sparse.csr_matrix:
    edges = np.concatenate((faces[:, [0, 1]], faces[:, [1, 2]], faces[:, [2, 0]]))
    rows = np.concatenate((edges[:, 0], edges[:, 1]))
    columns = np.concatenate((edges[:, 1], edges[:, 0]))
    matrix = sparse.coo_matrix(
        (np.ones(len(rows), dtype=np.float32), (rows, columns)),
        shape=(vertex_count, vertex_count),
    ).tocsr()
    matrix.data[:] = 1.0
    degree = np.asarray(matrix.sum(axis=1)).ravel()
    degree[degree == 0] = 1.0
    return sparse.diags(1.0 / degree) @ matrix


def automatic_heat_weights(
    vertices: np.ndarray, faces: np.ndarray, bones: list[Bone], iterations: int = 18
) -> tuple[np.ndarray, np.ndarray]:
    height = float(np.ptp(vertices[:, 1]))
    sigma = height * 0.055
    sources = np.empty((len(vertices), len(bones)), dtype=np.float32)
    for index, bone in enumerate(bones):
        distance = point_segment_distance(vertices, bone.position, bone.end)
        source = np.exp(-np.square(distance / sigma)).astype(np.float32)
        if bone.name.startswith("left"):
            source *= np.where(vertices[:, 0] >= -height * 0.015, 1.0, 0.01)
        elif bone.name.startswith("right"):
            source *= np.where(vertices[:, 0] <= height * 0.015, 1.0, 0.01)
        if "Arm" in bone.name or "Hand" in bone.name or "Shoulder" in bone.name:
            source *= np.where(vertices[:, 1] > height * 0.38, 1.0, 0.01)
        if "Leg" in bone.name or "Foot" in bone.name:
            source *= np.where(vertices[:, 1] < height * 0.57, 1.0, 0.01)
        sources[:, index] = np.maximum(source, 1e-12)
    transition = vertex_adjacency(len(vertices), faces)
    heat = sources.copy()
    for _ in range(iterations):
        heat = 0.28 * sources + 0.72 * (transition @ heat)
    heat /= np.maximum(heat.sum(axis=1, keepdims=True), 1e-12)
    top = np.argpartition(heat, -4, axis=1)[:, -4:]
    top_weights = np.take_along_axis(heat, top, axis=1)
    order = np.argsort(-top_weights, axis=1)
    joints = np.take_along_axis(top, order, axis=1).astype(np.uint16)
    weights = np.take_along_axis(top_weights, order, axis=1)
    weights /= np.maximum(weights.sum(axis=1, keepdims=True), 1e-12)
    return joints, weights.astype(np.float32)


def rotate_z(values: np.ndarray, angle: float) -> np.ndarray:
    cosine, sine = math.cos(angle), math.sin(angle)
    rotation = np.array(
        [[cosine, -sine, 0.0], [sine, cosine, 0.0], [0.0, 0.0, 1.0]],
        dtype=np.float32,
    )
    return values @ rotation.T


def normalize_arms_to_t_pose(
    vertices: np.ndarray,
    normals: np.ndarray,
    bones: list[Bone],
    joints: np.ndarray,
    weights: np.ndarray,
) -> tuple[np.ndarray, np.ndarray, list[Bone], dict[str, float]]:
    output_vertices = vertices.copy()
    output_normals = normals.copy()
    updated = list(bones)
    angles: dict[str, float] = {}
    for prefix, target_angle in (("left", 0.0), ("right", math.pi)):
        upper_index = next(i for i, bone in enumerate(bones) if bone.name == f"{prefix}UpperArm")
        shoulder = bones[upper_index].position
        direction = bones[upper_index].end - shoulder
        current_angle = math.atan2(float(direction[1]), float(direction[0]))
        delta = (target_angle - current_angle + math.pi) % (2 * math.pi) - math.pi
        arm_indices = {
            i
            for i, bone in enumerate(bones)
            if bone.name.startswith(prefix)
            and any(part in bone.name for part in ("Shoulder", "Arm", "Hand"))
        }
        influence = np.zeros(len(vertices), dtype=np.float32)
        for column in range(4):
            influence += np.where(
                np.isin(joints[:, column], list(arm_indices)), weights[:, column], 0.0
            )
        rotated = shoulder + rotate_z(vertices - shoulder, delta)
        output_vertices += influence[:, None] * (rotated - vertices)
        rotated_normals = rotate_z(normals, delta)
        output_normals += influence[:, None] * (rotated_normals - normals)
        for index in arm_indices:
            bone = bones[index]
            updated[index] = Bone(
                bone.name,
                bone.parent,
                shoulder + rotate_z((bone.position - shoulder)[None], delta)[0],
                shoulder + rotate_z((bone.end - shoulder)[None], delta)[0],
            )
        angles[prefix] = round(math.degrees(delta), 3)
    lengths = np.linalg.norm(output_normals, axis=1, keepdims=True)
    output_normals /= np.maximum(lengths, 1e-12)
    return output_vertices, output_normals, updated, angles


class BufferBuilder:
    def __init__(self) -> None:
        self.data = bytearray()
        self.views: list[dict[str, int]] = []
        self.accessors: list[dict[str, object]] = []

    def add_view(self, data: bytes, target: int | None = None) -> int:
        while len(self.data) % 4:
            self.data.append(0)
        offset = len(self.data)
        self.data.extend(data)
        view: dict[str, int] = {"buffer": 0, "byteOffset": offset, "byteLength": len(data)}
        if target is not None:
            view["target"] = target
        self.views.append(view)
        return len(self.views) - 1

    def add_accessor(
        self,
        array: np.ndarray,
        component_type: int,
        kind: str,
        target: int | None = None,
        include_bounds: bool = False,
    ) -> int:
        contiguous = np.ascontiguousarray(array)
        view = self.add_view(contiguous.tobytes(), target)
        accessor: dict[str, object] = {
            "bufferView": view,
            "componentType": component_type,
            "count": len(contiguous),
            "type": kind,
        }
        if include_bounds:
            if kind == "SCALAR":
                accessor["min"] = [float(contiguous.min())]
                accessor["max"] = [float(contiguous.max())]
            else:
                accessor["min"] = contiguous.min(axis=0).astype(float).tolist()
                accessor["max"] = contiguous.max(axis=0).astype(float).tolist()
        self.accessors.append(accessor)
        return len(self.accessors) - 1


def quaternion(axis: str, angle: float) -> list[float]:
    result = [0.0, 0.0, 0.0, math.cos(angle / 2)]
    result[{"x": 0, "y": 1, "z": 2}[axis]] = math.sin(angle / 2)
    return result


def uv_to_gltf(uv: np.ndarray) -> np.ndarray:
    converted = np.asarray(uv, dtype=np.float32).copy()
    converted[:, 1] = 1.0 - converted[:, 1]
    return converted


def build_vrm(
    output: Path,
    vertices: np.ndarray,
    normals: np.ndarray,
    uv: np.ndarray,
    faces: np.ndarray,
    joints: np.ndarray,
    weights: np.ndarray,
    bones: list[Bone],
    texture: Image.Image,
    name: str,
) -> None:
    builder = BufferBuilder()
    position_accessor = builder.add_accessor(vertices.astype(np.float32), 5126, "VEC3", 34962, True)
    normal_accessor = builder.add_accessor(normals.astype(np.float32), 5126, "VEC3", 34962)
    uv_accessor = builder.add_accessor(uv.astype(np.float32), 5126, "VEC2", 34962)
    joints_accessor = builder.add_accessor(joints.astype(np.uint16), 5123, "VEC4", 34962)
    weights_accessor = builder.add_accessor(weights.astype(np.float32), 5126, "VEC4", 34962)
    index_accessor = builder.add_accessor(faces.reshape(-1).astype(np.uint32), 5125, "SCALAR", 34963)

    bone_indices = {bone.name: index for index, bone in enumerate(bones)}
    node_indices = {name: index for index, name in enumerate(bone_indices)}
    nodes: list[dict[str, object]] = []
    for bone in bones:
        translation = bone.position if bone.parent is None else bone.position - bones[bone_indices[bone.parent]].position
        nodes.append({"name": bone.name, "translation": translation.astype(float).tolist()})
    for bone in bones:
        if bone.parent is not None:
            parent_node = nodes[node_indices[bone.parent]]
            parent_node.setdefault("children", []).append(node_indices[bone.name])

    inverse_bind = []
    for bone in bones:
        matrix = np.eye(4, dtype=np.float32)
        matrix[:3, 3] = -bone.position
        inverse_bind.append(matrix.flatten(order="F"))
    inverse_accessor = builder.add_accessor(np.asarray(inverse_bind, dtype=np.float32), 5126, "MAT4")

    texture_bytes = io.BytesIO()
    texture.save(texture_bytes, format="PNG")
    image_view = builder.add_view(texture_bytes.getvalue())
    mesh_node = len(nodes)
    nodes.append({"name": "CharacterMesh", "mesh": 0, "skin": 0})

    times = np.array([0.0, 1.5, 3.0], dtype=np.float32)
    time_accessor = builder.add_accessor(times, 5126, "SCALAR", include_bounds=True)
    animation_channels = []
    animation_samplers = []
    for bone_name, axis, amplitude in (("chest", "z", 0.018), ("head", "y", 0.028)):
        rotations = np.array(
            [quaternion(axis, -amplitude), quaternion(axis, amplitude), quaternion(axis, -amplitude)],
            dtype=np.float32,
        )
        rotation_accessor = builder.add_accessor(rotations, 5126, "VEC4")
        animation_samplers.append(
            {"input": time_accessor, "output": rotation_accessor, "interpolation": "LINEAR"}
        )
        animation_channels.append(
            {
                "sampler": len(animation_samplers) - 1,
                "target": {"node": node_indices[bone_name], "path": "rotation"},
            }
        )

    human_bones = {name: {"node": node_indices[name]} for name in node_indices}
    document = {
        "asset": {"version": "2.0", "generator": "LocalVTuberStudio"},
        "extensionsUsed": ["VRMC_vrm"],
        "extensionsRequired": ["VRMC_vrm"],
        "extensions": {
            "VRMC_vrm": {
                "specVersion": "1.0",
                "meta": {
                    "name": name,
                    "authors": ["LocalVTuberStudio user"],
                    "avatarPermission": "onlyAuthor",
                    "commercialUsage": "personalNonProfit",
                    "creditNotation": "required",
                    "allowRedistribution": False,
                    "modification": "prohibited",
                    "licenseUrl": "https://vrm.dev/licenses/1.0/",
                },
                "humanoid": {"humanBones": human_bones},
            }
        },
        "scene": 0,
        "scenes": [{"nodes": [node_indices["hips"], mesh_node]}],
        "nodes": nodes,
        "skins": [
            {
                "name": "HumanoidSkin",
                "inverseBindMatrices": inverse_accessor,
                "joints": list(range(len(bones))),
                "skeleton": node_indices["hips"],
            }
        ],
        "meshes": [
            {
                "name": "CharacterMesh",
                "primitives": [
                    {
                        "attributes": {
                            "POSITION": position_accessor,
                            "NORMAL": normal_accessor,
                            "TEXCOORD_0": uv_accessor,
                            "JOINTS_0": joints_accessor,
                            "WEIGHTS_0": weights_accessor,
                        },
                        "indices": index_accessor,
                        "material": 0,
                    }
                ],
            }
        ],
        "materials": [
            {
                "name": "CharacterUnlitSource",
                "pbrMetallicRoughness": {
                    "baseColorTexture": {"index": 0},
                    "metallicFactor": 0.0,
                    "roughnessFactor": 1.0,
                },
                "doubleSided": True,
            }
        ],
        "textures": [{"sampler": 0, "source": 0}],
        "samplers": [{"magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497}],
        "images": [{"bufferView": image_view, "mimeType": "image/png"}],
        "animations": [
            {
                "name": "Idle",
                "channels": animation_channels,
                "samplers": animation_samplers,
            }
        ],
        "bufferViews": builder.views,
        "accessors": builder.accessors,
        "buffers": [{"byteLength": len(builder.data)}],
    }
    json_bytes = json.dumps(document, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    json_bytes += b" " * ((4 - len(json_bytes) % 4) % 4)
    binary = bytes(builder.data)
    binary += b"\0" * ((4 - len(binary) % 4) % 4)
    total_length = 12 + 8 + len(json_bytes) + 8 + len(binary)
    with output.open("wb") as handle:
        handle.write(struct.pack("<III", 0x46546C67, 2, total_length))
        handle.write(struct.pack("<II", len(json_bytes), 0x4E4F534A))
        handle.write(json_bytes)
        handle.write(struct.pack("<II", len(binary), 0x004E4942))
        handle.write(binary)


def run(args: argparse.Namespace) -> None:
    started = time.perf_counter()
    input_path = args.input.resolve(strict=True)
    output_dir = args.output.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    emit("rig_progress", stage="mesh_loading", progress=0.05)
    mesh, texture = load_textured_mesh(input_path)
    vertices, scale = normalize_to_vrm_axes(np.asarray(mesh.vertices))
    normals = np.column_stack(
        (
            np.asarray(mesh.vertex_normals)[:, 1],
            np.asarray(mesh.vertex_normals)[:, 2],
            np.asarray(mesh.vertex_normals)[:, 0],
        )
    ).astype(np.float32)
    pose = validate_humanoid_pose(vertices)
    bones = estimate_bones(vertices)
    missing = sorted(set(REQUIRED_BONES) - {bone.name for bone in bones})
    if missing:
        raise RuntimeError(f"VRM必須ボーンが不足しています: {', '.join(missing)}")

    emit("rig_progress", stage="heat_diffusion", progress=0.25)
    joints, weights = automatic_heat_weights(vertices, np.asarray(mesh.faces), bones)
    vertices, normals, bones, arm_angles = normalize_arms_to_t_pose(
        vertices, normals, bones, joints, weights
    )
    if not np.allclose(weights.sum(axis=1), 1.0, atol=1e-5):
        raise RuntimeError("スキニングウェイトの正規化に失敗しました")

    emit("rig_progress", stage="vrm_export", progress=0.82)
    output_path = output_dir / "rigged.vrm"
    build_vrm(
        output_path,
        vertices,
        normals,
        uv_to_gltf(np.asarray(mesh.visual.uv)),
        np.asarray(mesh.faces),
        joints,
        weights,
        bones,
        texture,
        args.name,
    )
    metrics = {
        "input": str(input_path),
        "output": str(output_path),
        "vertices": len(vertices),
        "faces": len(mesh.faces),
        "bones": len(bones),
        "required_bones": len(REQUIRED_BONES),
        "max_influences": 4,
        "weight_sum_max_error": round(float(np.max(np.abs(weights.sum(axis=1) - 1))), 8),
        "source_to_meters_scale": round(scale, 6),
        "pose": pose,
        "arm_normalization_degrees": arm_angles,
        "total_seconds": round(time.perf_counter() - started, 3),
    }
    (output_dir / "rig-metrics.json").write_text(
        json.dumps(metrics, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    emit("rig_complete", **metrics)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--name", default="LocalVTuberStudio Character")
    return parser.parse_args()


if __name__ == "__main__":
    try:
        run(parse_args())
    except Exception as error:
        emit("rig_error", message=str(error))
        raise
