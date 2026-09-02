use std::path::{Path, PathBuf};

use glam::{Mat4, Vec2, Vec3};
use image::{GrayImage, ImageBuffer, Luma, RgbaImage};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacePatchVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub head_weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeutralMeshSnapshot {
    pub model_id: String,
    pub vertices: Vec<FacePatchVertex>,
    pub triangles: Vec<[u32; 3]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureFrame {
    pub version: String,
    pub capture_resolution: u32,
    pub center: [f32; 3],
    pub forward: [f32; 3],
    pub up: [f32; 3],
    pub patch_size: [f32; 2],
    pub front_z: f32,
    pub back_z: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectionSettings {
    pub atlas_resolution: u32,
    pub diff_threshold: f32,
    pub alpha_threshold: f32,
    pub seam_padding: u32,
    pub head_weight_threshold: f32,
    pub normal_threshold: f32,
    pub max_ray_hits: usize,
    pub depth_window: f32,
    pub face_mask_center: [f32; 2],
    pub face_mask_radius: [f32; 2],
}

impl Default for ProjectionSettings {
    fn default() -> Self {
        Self {
            atlas_resolution: 2048,
            diff_threshold: 0.075,
            alpha_threshold: 0.030,
            seam_padding: 4,
            head_weight_threshold: 0.5,
            normal_threshold: 0.35,
            max_ray_hits: 3,
            depth_window: 0.05,
            face_mask_center: [0.5, 0.58],
            face_mask_radius: [0.24, 0.26],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectionStats {
    pub written_pixels: u64,
    pub padded_pixels: u64,
    pub selected_triangles: usize,
    pub bounding_box: [u32; 4],
}

pub fn projection_signature(
    pipeline_version: u32,
    mesh: &NeutralMeshSnapshot,
    frame: &CaptureFrame,
    settings: &ProjectionSettings,
    prompt_hash: &str,
) -> Result<String, serde_json::Error> {
    let payload = serde_json::to_vec(&serde_json::json!({
        "pipeline_version": pipeline_version,
        "model_id": &mesh.model_id,
        "capture_frame": frame,
        "projection_settings": settings,
        "prompt_hash": prompt_hash,
    }))?;
    Ok(format!("{:x}", Sha256::digest(payload)))
}

pub fn compose_expression_layers(
    neutral: &RgbaImage,
    eyes: &RgbaImage,
    mouth: &RgbaImage,
    mouth_scale: f32,
) -> Result<RgbaImage, FacePatchError> {
    if neutral.dimensions() != eyes.dimensions() || neutral.dimensions() != mouth.dimensions() {
        return Err(FacePatchError::InvalidInput(
            "表情・口形レイヤーと中立画像の寸法が一致しません".into(),
        ));
    }
    if !(0.0..=1.0).contains(&mouth_scale) {
        return Err(FacePatchError::InvalidInput(
            "口の開き係数は0〜1の範囲が必要です".into(),
        ));
    }
    let mut output = neutral.clone();
    for (x, y, pixel) in output.enumerate_pixels_mut() {
        let base = neutral.get_pixel(x, y).0;
        let eye = eyes.get_pixel(x, y).0;
        let mouth_pixel = mouth.get_pixel(x, y).0;
        for channel in 0..4 {
            let eye_delta = eye[channel] as f32 - base[channel] as f32;
            let mouth_delta = (mouth_pixel[channel] as f32 - base[channel] as f32) * mouth_scale;
            pixel.0[channel] = (base[channel] as f32 + eye_delta + mouth_delta)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
    Ok(output)
}

#[derive(Debug, Error)]
pub enum FacePatchError {
    #[error("GLB/VRMの読み込みに失敗しました: {0}")]
    Gltf(#[from] gltf::Error),
    #[error("画像の読み込みに失敗しました: {0}")]
    Image(#[from] image::ImageError),
    #[error("ファイル操作に失敗しました: {0}")]
    Io(#[from] std::io::Error),
    #[error("入力が不正です: {0}")]
    InvalidInput(String),
    #[error("投影画素が0です。診断画像: {diagnostics}")]
    NoPixels { diagnostics: PathBuf },
}

pub fn load_neutral_snapshot(path: &Path) -> Result<NeutralMeshSnapshot, FacePatchError> {
    let (document, buffers, _) = gltf::import(path)?;
    let mut world_transforms = vec![None; document.nodes().count()];
    for scene in document.scenes() {
        for node in scene.nodes() {
            collect_world_transforms(node, Mat4::IDENTITY, &mut world_transforms);
        }
    }

    let head_node = document
        .nodes()
        .find(|node| {
            node.name()
                .is_some_and(|name| name.to_ascii_lowercase().contains("head"))
        })
        .ok_or_else(|| FacePatchError::InvalidInput("headボーンが見つかりません".into()))?
        .index();
    let mut snapshot = NeutralMeshSnapshot {
        model_id: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("model")
            .to_owned(),
        vertices: Vec::new(),
        triangles: Vec::new(),
    };

    for node in document.nodes() {
        let (Some(mesh), Some(skin)) = (node.mesh(), node.skin()) else {
            continue;
        };
        let world = world_transforms[node.index()].ok_or_else(|| {
            FacePatchError::InvalidInput("メッシュノードのワールド変換がありません".into())
        })?;
        let joints: Vec<_> = skin.joints().collect();
        let inverse_bind: Vec<Mat4> = skin
            .reader(|buffer| Some(&buffers[buffer.index()]))
            .read_inverse_bind_matrices()
            .map(|matrices| {
                matrices
                    .map(|matrix| Mat4::from_cols_array_2d(&matrix))
                    .collect()
            })
            .unwrap_or_else(|| vec![Mat4::IDENTITY; joints.len()]);
        if inverse_bind.len() != joints.len() {
            return Err(FacePatchError::InvalidInput(
                "inverse bind matrixの数がjoint数と一致しません".into(),
            ));
        }
        let joint_matrices: Vec<Mat4> = joints
            .iter()
            .zip(inverse_bind)
            .map(|(joint, inverse)| {
                world_transforms[joint.index()].unwrap_or(Mat4::IDENTITY) * inverse
            })
            .collect();
        let head_joint = joints
            .iter()
            .position(|joint| joint.index() == head_node)
            .ok_or_else(|| {
                FacePatchError::InvalidInput("メッシュのskinにheadボーンが含まれていません".into())
            })?;

        for primitive in mesh.primitives() {
            if primitive.mode() != gltf::mesh::Mode::Triangles {
                continue;
            }
            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
            let positions: Vec<_> = reader
                .read_positions()
                .ok_or_else(|| FacePatchError::InvalidInput("POSITIONがありません".into()))?
                .collect();
            let normals: Vec<_> = reader
                .read_normals()
                .ok_or_else(|| FacePatchError::InvalidInput("NORMALがありません".into()))?
                .collect();
            let uvs: Vec<_> = reader
                .read_tex_coords(0)
                .ok_or_else(|| FacePatchError::InvalidInput("TEXCOORD_0がありません".into()))?
                .into_f32()
                .collect();
            let joint_indices: Vec<_> = reader
                .read_joints(0)
                .ok_or_else(|| FacePatchError::InvalidInput("JOINTS_0がありません".into()))?
                .into_u16()
                .collect();
            let weights: Vec<_> = reader
                .read_weights(0)
                .ok_or_else(|| FacePatchError::InvalidInput("WEIGHTS_0がありません".into()))?
                .into_f32()
                .collect();
            if [normals.len(), uvs.len(), joint_indices.len(), weights.len()]
                .iter()
                .any(|length| *length != positions.len())
            {
                return Err(FacePatchError::InvalidInput(
                    "頂点属性の要素数が一致しません".into(),
                ));
            }
            let base = snapshot.vertices.len() as u32;
            for index in 0..positions.len() {
                let mut skinned_position = Vec3::ZERO;
                let mut skinned_normal = Vec3::ZERO;
                let mut head_weight = 0.0;
                for influence in 0..4 {
                    let joint = joint_indices[index][influence] as usize;
                    let weight = weights[index][influence];
                    let matrix = *joint_matrices.get(joint).ok_or_else(|| {
                        FacePatchError::InvalidInput("jointインデックスが範囲外です".into())
                    })?;
                    skinned_position +=
                        matrix.transform_point3(Vec3::from_array(positions[index])) * weight;
                    skinned_normal +=
                        matrix.transform_vector3(Vec3::from_array(normals[index])) * weight;
                    if joint == head_joint {
                        head_weight += weight;
                    }
                }
                if weights[index].iter().copied().sum::<f32>() <= f32::EPSILON {
                    skinned_position = world.transform_point3(Vec3::from_array(positions[index]));
                    skinned_normal = world.transform_vector3(Vec3::from_array(normals[index]));
                }
                snapshot.vertices.push(FacePatchVertex {
                    position: skinned_position.to_array(),
                    normal: skinned_normal.normalize_or_zero().to_array(),
                    uv: uvs[index],
                    head_weight,
                });
            }
            let indices: Vec<u32> = reader
                .read_indices()
                .map(|values| values.into_u32().collect())
                .unwrap_or_else(|| (0..positions.len() as u32).collect());
            if indices.len() % 3 != 0 {
                return Err(FacePatchError::InvalidInput(
                    "三角形インデックス数が3の倍数ではありません".into(),
                ));
            }
            snapshot.triangles.extend(
                indices
                    .chunks_exact(3)
                    .map(|triangle| [base + triangle[0], base + triangle[1], base + triangle[2]]),
            );
        }
    }
    if snapshot.vertices.is_empty() {
        return Err(FacePatchError::InvalidInput(
            "skin付き三角形メッシュがありません".into(),
        ));
    }
    Ok(snapshot)
}

fn collect_world_transforms(node: gltf::Node<'_>, parent: Mat4, transforms: &mut [Option<Mat4>]) {
    let world = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
    transforms[node.index()] = Some(world);
    for child in node.children() {
        collect_world_transforms(child, world, transforms);
    }
}

#[derive(Clone, Copy)]
struct CameraBasis {
    center: Vec3,
    right: Vec3,
    up: Vec3,
    forward: Vec3,
    extent: f32,
}

#[derive(Clone, Copy)]
struct ProjectedVertex {
    screen: Vec2,
    depth: f32,
}

pub fn project_face_patch(
    mesh: &NeutralMeshSnapshot,
    neutral: &RgbaImage,
    expression: &RgbaImage,
    original_atlas: &RgbaImage,
    frame: &CaptureFrame,
    settings: &ProjectionSettings,
    diagnostics_dir: &Path,
) -> Result<(RgbaImage, ProjectionStats), FacePatchError> {
    validate_inputs(mesh, neutral, expression, original_atlas, frame, settings)?;
    std::fs::create_dir_all(diagnostics_dir)?;

    let basis = camera_basis(frame)?;
    let selected = select_triangles(mesh, basis, frame, settings)?;
    let source_mask = build_source_mask(neutral, expression, settings);
    let depth_layers = build_depth_layers(mesh, basis, frame, settings);
    let atlas_size = settings.atlas_resolution;
    let texel_count = (atlas_size as usize) * (atlas_size as usize);
    let mut allowed = vec![false; texel_count];
    let mut blocked = vec![false; texel_count];

    for (triangle_index, triangle) in mesh.triangles.iter().enumerate() {
        let vertices = triangle_vertices(mesh, *triangle)?;
        let target = if selected[triangle_index] {
            &mut allowed
        } else {
            &mut blocked
        };
        rasterize_uv(vertices, atlas_size, |x, y, _| {
            target[index(atlas_size, x, y)] = true;
        });
    }

    let mut result = original_atlas.clone();
    let mut written = vec![false; texel_count];
    let mut written_pixels = 0_u64;
    let mut min_x = atlas_size;
    let mut min_y = atlas_size;
    let mut max_x = 0;
    let mut max_y = 0;

    for (triangle_index, triangle) in mesh.triangles.iter().enumerate() {
        if !selected[triangle_index] {
            continue;
        }
        let vertices = triangle_vertices(mesh, *triangle)?;
        rasterize_uv(vertices, atlas_size, |x, y, barycentric| {
            let world = interpolate_position(vertices, barycentric);
            let projected = project_point(basis, world, neutral.width());
            let sx = projected.screen.x.floor() as i32;
            let sy = projected.screen.y.floor() as i32;
            if sx < 0 || sy < 0 || sx >= neutral.width() as i32 || sy >= neutral.height() as i32 {
                return;
            }
            let source_index = index(neutral.width(), sx as u32, sy as u32);
            if !source_mask[source_index]
                || !is_frontmost(projected.depth, &depth_layers[source_index], settings)
            {
                return;
            }
            let pixel = *expression.get_pixel(sx as u32, sy as u32);
            result.put_pixel(x, y, pixel);
            let target_index = index(atlas_size, x, y);
            if !written[target_index] {
                written[target_index] = true;
                written_pixels += 1;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        });
    }

    let padded_pixels = pad_seams(
        &mut result,
        &mut written,
        &blocked,
        atlas_size,
        settings.seam_padding,
    );
    let diagnostic_root = write_diagnostics(
        diagnostics_dir,
        neutral.width(),
        atlas_size,
        &selected,
        mesh,
        &source_mask,
        &written,
    )?;
    if written_pixels == 0 {
        return Err(FacePatchError::NoPixels {
            diagnostics: diagnostic_root,
        });
    }

    Ok((
        result,
        ProjectionStats {
            written_pixels,
            padded_pixels,
            selected_triangles: selected.iter().filter(|value| **value).count(),
            bounding_box: [min_x, min_y, max_x, max_y],
        },
    ))
}

fn validate_inputs(
    mesh: &NeutralMeshSnapshot,
    neutral: &RgbaImage,
    expression: &RgbaImage,
    atlas: &RgbaImage,
    frame: &CaptureFrame,
    settings: &ProjectionSettings,
) -> Result<(), FacePatchError> {
    if neutral.dimensions() != expression.dimensions() {
        return Err(FacePatchError::InvalidInput(
            "中立画像と表情画像の寸法が一致しません".into(),
        ));
    }
    if neutral.width() != frame.capture_resolution || neutral.height() != frame.capture_resolution {
        return Err(FacePatchError::InvalidInput(
            "画像寸法がキャプチャ解像度と一致しません".into(),
        ));
    }
    if atlas.width() != settings.atlas_resolution || atlas.height() != settings.atlas_resolution {
        return Err(FacePatchError::InvalidInput(
            "アトラス寸法が設定値と一致しません".into(),
        ));
    }
    if mesh.vertices.is_empty() || mesh.triangles.is_empty() {
        return Err(FacePatchError::InvalidInput("メッシュが空です".into()));
    }
    if settings.max_ray_hits == 0 {
        return Err(FacePatchError::InvalidInput(
            "max_ray_hitsは1以上が必要です".into(),
        ));
    }
    Ok(())
}

fn camera_basis(frame: &CaptureFrame) -> Result<CameraBasis, FacePatchError> {
    let forward = Vec3::from_array(frame.forward).normalize_or_zero();
    let up = Vec3::from_array(frame.up).normalize_or_zero();
    let right = up.cross(forward).normalize_or_zero();
    if forward == Vec3::ZERO || up == Vec3::ZERO || right == Vec3::ZERO {
        return Err(FacePatchError::InvalidInput(
            "キャプチャ座標軸を正規化できません".into(),
        ));
    }
    Ok(CameraBasis {
        center: Vec3::from_array(frame.center),
        right,
        up,
        forward,
        extent: frame.patch_size[0].max(frame.patch_size[1]) * 1.24,
    })
}

fn select_triangles(
    mesh: &NeutralMeshSnapshot,
    basis: CameraBasis,
    frame: &CaptureFrame,
    settings: &ProjectionSettings,
) -> Result<Vec<bool>, FacePatchError> {
    mesh.triangles
        .iter()
        .map(|triangle| {
            let vertices = triangle_vertices(mesh, *triangle)?;
            let head = vertices
                .iter()
                .all(|vertex| vertex.head_weight >= settings.head_weight_threshold);
            let normal = vertices
                .iter()
                .map(|vertex| Vec3::from_array(vertex.normal))
                .sum::<Vec3>()
                .normalize_or_zero();
            let center = vertices
                .iter()
                .map(|vertex| Vec3::from_array(vertex.position))
                .sum::<Vec3>()
                / 3.0;
            let local = center - basis.center;
            let inside = local.dot(basis.right).abs() <= frame.patch_size[0] * 0.5
                && local.dot(basis.up).abs() <= frame.patch_size[1] * 0.5
                && local.dot(basis.forward) <= frame.front_z
                && local.dot(basis.forward) >= -frame.back_z;
            Ok(head && normal.dot(basis.forward) >= settings.normal_threshold && inside)
        })
        .collect()
}

fn build_source_mask(
    neutral: &RgbaImage,
    expression: &RgbaImage,
    settings: &ProjectionSettings,
) -> Vec<bool> {
    let mut mask = vec![false; (neutral.width() * neutral.height()) as usize];
    for y in 0..neutral.height() {
        for x in 0..neutral.width() {
            let base = neutral.get_pixel(x, y).0;
            let edited = expression.get_pixel(x, y).0;
            let alpha = edited[3] as f32 / 255.0;
            let difference = (0..3)
                .map(|channel| base[channel].abs_diff(edited[channel]))
                .max()
                .unwrap_or(0) as f32
                / 255.0;
            let normalized = Vec2::new(
                (x as f32 + 0.5) / neutral.width() as f32,
                (y as f32 + 0.5) / neutral.height() as f32,
            );
            let ellipse = Vec2::new(
                (normalized.x - settings.face_mask_center[0]) / settings.face_mask_radius[0],
                (normalized.y - settings.face_mask_center[1]) / settings.face_mask_radius[1],
            );
            mask[index(neutral.width(), x, y)] = alpha >= settings.alpha_threshold
                && difference >= settings.diff_threshold
                && ellipse.length_squared() <= 1.0;
        }
    }
    mask
}

fn build_depth_layers(
    mesh: &NeutralMeshSnapshot,
    basis: CameraBasis,
    frame: &CaptureFrame,
    settings: &ProjectionSettings,
) -> Vec<Vec<f32>> {
    let size = frame.capture_resolution;
    let mut layers = vec![Vec::new(); (size * size) as usize];
    for triangle in &mesh.triangles {
        let Ok(vertices) = triangle_vertices(mesh, *triangle) else {
            continue;
        };
        let projected =
            vertices.map(|vertex| project_point(basis, Vec3::from_array(vertex.position), size));
        rasterize_screen(projected, size, |x, y, barycentric| {
            let depth = projected[0].depth * barycentric[0]
                + projected[1].depth * barycentric[1]
                + projected[2].depth * barycentric[2];
            if depth < -frame.back_z || depth > frame.front_z {
                return;
            }
            let cell = &mut layers[index(size, x, y)];
            cell.push(depth);
            cell.sort_by(|left, right| right.total_cmp(left));
            cell.dedup_by(|left, right| (*left - *right).abs() < 1.0e-5);
            cell.truncate(settings.max_ray_hits);
        });
    }
    layers
}

fn is_frontmost(depth: f32, layers: &[f32], settings: &ProjectionSettings) -> bool {
    layers
        .first()
        .is_some_and(|front| *front - depth <= settings.depth_window)
}

fn project_point(basis: CameraBasis, point: Vec3, resolution: u32) -> ProjectedVertex {
    let local = point - basis.center;
    let normalized_x = local.dot(basis.right) / basis.extent + 0.5;
    let normalized_y = 0.5 - local.dot(basis.up) / basis.extent;
    ProjectedVertex {
        screen: Vec2::new(
            normalized_x * resolution as f32,
            normalized_y * resolution as f32,
        ),
        depth: local.dot(basis.forward),
    }
}

fn triangle_vertices(
    mesh: &NeutralMeshSnapshot,
    triangle: [u32; 3],
) -> Result<[&FacePatchVertex; 3], FacePatchError> {
    Ok([
        mesh.vertices
            .get(triangle[0] as usize)
            .ok_or_else(|| FacePatchError::InvalidInput("三角形インデックスが範囲外です".into()))?,
        mesh.vertices
            .get(triangle[1] as usize)
            .ok_or_else(|| FacePatchError::InvalidInput("三角形インデックスが範囲外です".into()))?,
        mesh.vertices
            .get(triangle[2] as usize)
            .ok_or_else(|| FacePatchError::InvalidInput("三角形インデックスが範囲外です".into()))?,
    ])
}

fn interpolate_position(vertices: [&FacePatchVertex; 3], barycentric: [f32; 3]) -> Vec3 {
    Vec3::from_array(vertices[0].position) * barycentric[0]
        + Vec3::from_array(vertices[1].position) * barycentric[1]
        + Vec3::from_array(vertices[2].position) * barycentric[2]
}

fn rasterize_uv(
    vertices: [&FacePatchVertex; 3],
    resolution: u32,
    mut visit: impl FnMut(u32, u32, [f32; 3]),
) {
    let points = vertices.map(|vertex| {
        Vec2::new(
            vertex.uv[0] * resolution as f32,
            (1.0 - vertex.uv[1]) * resolution as f32,
        )
    });
    rasterize_points(points, resolution, &mut visit);
}

fn rasterize_screen(
    vertices: [ProjectedVertex; 3],
    resolution: u32,
    mut visit: impl FnMut(u32, u32, [f32; 3]),
) {
    rasterize_points(vertices.map(|vertex| vertex.screen), resolution, &mut visit);
}

fn rasterize_points(
    points: [Vec2; 3],
    resolution: u32,
    visit: &mut impl FnMut(u32, u32, [f32; 3]),
) {
    let min_x = points
        .iter()
        .map(|point| point.x.floor() as i32)
        .min()
        .unwrap_or(0)
        .clamp(0, resolution as i32 - 1);
    let max_x = points
        .iter()
        .map(|point| point.x.ceil() as i32)
        .max()
        .unwrap_or(0)
        .clamp(0, resolution as i32 - 1);
    let min_y = points
        .iter()
        .map(|point| point.y.floor() as i32)
        .min()
        .unwrap_or(0)
        .clamp(0, resolution as i32 - 1);
    let max_y = points
        .iter()
        .map(|point| point.y.ceil() as i32)
        .max()
        .unwrap_or(0)
        .clamp(0, resolution as i32 - 1);
    let area = edge(points[0], points[1], points[2]);
    if area.abs() < f32::EPSILON {
        return;
    }
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let point = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let barycentric = [
                edge(points[1], points[2], point) / area,
                edge(points[2], points[0], point) / area,
                edge(points[0], points[1], point) / area,
            ];
            if barycentric.iter().all(|value| *value >= -1.0e-5) {
                visit(x as u32, y as u32, barycentric);
            }
        }
    }
}

fn edge(a: Vec2, b: Vec2, point: Vec2) -> f32 {
    (point.x - a.x) * (b.y - a.y) - (point.y - a.y) * (b.x - a.x)
}

fn pad_seams(
    image: &mut RgbaImage,
    written: &mut [bool],
    blocked: &[bool],
    resolution: u32,
    iterations: u32,
) -> u64 {
    let mut padded = 0;
    for _ in 0..iterations {
        let previous = written.to_owned();
        let mut additions = Vec::new();
        for y in 0..resolution {
            for x in 0..resolution {
                let target = index(resolution, x, y);
                if previous[target] || blocked[target] {
                    continue;
                }
                'neighbors: for dy in -1_i32..=1 {
                    for dx in -1_i32..=1 {
                        let nx = x as i32 + dx;
                        let ny = y as i32 + dy;
                        if nx < 0 || ny < 0 || nx >= resolution as i32 || ny >= resolution as i32 {
                            continue;
                        }
                        let source = index(resolution, nx as u32, ny as u32);
                        if previous[source] {
                            additions.push((x, y, *image.get_pixel(nx as u32, ny as u32)));
                            break 'neighbors;
                        }
                    }
                }
            }
        }
        for (x, y, pixel) in additions {
            image.put_pixel(x, y, pixel);
            written[index(resolution, x, y)] = true;
            padded += 1;
        }
    }
    padded
}

fn write_diagnostics(
    directory: &Path,
    source_size: u32,
    atlas_size: u32,
    selected: &[bool],
    mesh: &NeutralMeshSnapshot,
    source_mask: &[bool],
    written: &[bool],
) -> Result<PathBuf, FacePatchError> {
    let source = bool_image(source_size, source_size, source_mask);
    source.save(directory.join("source-mask.png"))?;
    let output = bool_image(atlas_size, atlas_size, written);
    output.save(directory.join("written-mask.png"))?;
    let mut triangles = GrayImage::new(atlas_size, atlas_size);
    for (triangle_index, triangle) in mesh.triangles.iter().enumerate() {
        if !selected[triangle_index] {
            continue;
        }
        let vertices = triangle_vertices(mesh, *triangle)?;
        rasterize_uv(vertices, atlas_size, |x, y, _| {
            triangles.put_pixel(x, y, Luma([255]));
        });
    }
    triangles.save(directory.join("selected-triangles.png"))?;
    Ok(directory.to_owned())
}

fn bool_image(width: u32, height: u32, values: &[bool]) -> GrayImage {
    ImageBuffer::from_fn(width, height, |x, y| {
        Luma([if values[index(width, x, y)] { 255 } else { 0 }])
    })
}

fn index(width: u32, x: u32, y: u32) -> usize {
    (y as usize) * (width as usize) + x as usize
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    fn vertex(position: [f32; 3], uv: [f32; 2], head_weight: f32) -> FacePatchVertex {
        FacePatchVertex {
            position,
            normal: [0.0, 0.0, 1.0],
            uv,
            head_weight,
        }
    }

    fn fixture() -> (NeutralMeshSnapshot, CaptureFrame, ProjectionSettings) {
        let mesh = NeutralMeshSnapshot {
            model_id: "known-face".into(),
            vertices: vec![
                vertex([-0.5, -0.5, 0.1], [0.1, 0.1], 1.0),
                vertex([0.5, -0.5, 0.1], [0.4, 0.1], 1.0),
                vertex([0.5, 0.5, 0.1], [0.4, 0.4], 1.0),
                vertex([-0.5, 0.5, 0.1], [0.1, 0.4], 1.0),
                vertex([-0.5, -0.5, -0.1], [0.6, 0.1], 1.0),
                vertex([0.5, -0.5, -0.1], [0.9, 0.1], 1.0),
                vertex([0.5, 0.5, -0.1], [0.9, 0.4], 1.0),
                vertex([-0.5, 0.5, -0.1], [0.6, 0.4], 1.0),
            ],
            triangles: vec![[0, 1, 2], [0, 2, 3], [4, 5, 6], [4, 6, 7]],
        };
        let frame = CaptureFrame {
            version: "projection_frame_v1".into(),
            capture_resolution: 64,
            center: [0.0, 0.0, 0.0],
            forward: [0.0, 0.0, 1.0],
            up: [0.0, 1.0, 0.0],
            patch_size: [1.2, 1.2],
            front_z: 0.2,
            back_z: 0.2,
        };
        let settings = ProjectionSettings {
            atlas_resolution: 64,
            diff_threshold: 0.01,
            alpha_threshold: 0.01,
            seam_padding: 2,
            face_mask_center: [0.5, 0.5],
            face_mask_radius: [0.5, 0.5],
            ..ProjectionSettings::default()
        };
        (mesh, frame, settings)
    }

    #[test]
    fn loads_neutral_skinned_snapshot_from_gltf() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/known-face.gltf");
        let snapshot = load_neutral_snapshot(&path).unwrap();
        assert_eq!(snapshot.vertices.len(), 4);
        assert_eq!(snapshot.triangles, vec![[0, 1, 2], [0, 2, 3]]);
        assert!(
            snapshot
                .vertices
                .iter()
                .all(|vertex| (vertex.head_weight - 1.0).abs() < 1.0e-6)
        );
    }

    #[test]
    fn front_surface_is_written_and_back_surface_is_rejected() {
        let (mesh, frame, settings) = fixture();
        let neutral = RgbaImage::from_pixel(64, 64, image::Rgba([120, 100, 80, 255]));
        let expression = RgbaImage::from_pixel(64, 64, image::Rgba([220, 20, 20, 255]));
        let atlas = RgbaImage::from_pixel(64, 64, image::Rgba([10, 10, 10, 255]));
        let diagnostics = tempdir().unwrap();
        let (result, stats) = project_face_patch(
            &mesh,
            &neutral,
            &expression,
            &atlas,
            &frame,
            &settings,
            diagnostics.path(),
        )
        .unwrap();
        assert_eq!(
            stats,
            ProjectionStats {
                written_pixels: 400,
                padded_pixels: 176,
                selected_triangles: 4,
                bounding_box: [6, 38, 25, 57],
            }
        );
        assert_eq!(*result.get_pixel(16, 48), image::Rgba([220, 20, 20, 255]));
        assert_eq!(*result.get_pixel(48, 48), image::Rgba([10, 10, 10, 255]));
    }

    #[test]
    fn zero_difference_is_an_explicit_error_with_diagnostics() {
        let (mesh, frame, settings) = fixture();
        let neutral = RgbaImage::from_pixel(64, 64, image::Rgba([120, 100, 80, 255]));
        let atlas = RgbaImage::from_pixel(64, 64, image::Rgba([10, 10, 10, 255]));
        let diagnostics = tempdir().unwrap();
        let error = project_face_patch(
            &mesh,
            &neutral,
            &neutral,
            &atlas,
            &frame,
            &settings,
            diagnostics.path(),
        )
        .unwrap_err();
        assert!(matches!(error, FacePatchError::NoPixels { .. }));
        assert!(diagnostics.path().join("source-mask.png").exists());
        assert!(diagnostics.path().join("selected-triangles.png").exists());
        assert!(diagnostics.path().join("written-mask.png").exists());
    }

    #[test]
    fn signature_changes_with_prompt_or_projection_setting() {
        let (mesh, frame, mut settings) = fixture();
        let baseline = projection_signature(1, &mesh, &frame, &settings, "prompt-a").unwrap();
        let changed_prompt = projection_signature(1, &mesh, &frame, &settings, "prompt-b").unwrap();
        settings.diff_threshold += 0.001;
        let changed_setting =
            projection_signature(1, &mesh, &frame, &settings, "prompt-a").unwrap();
        assert_eq!(baseline.len(), 64);
        assert_ne!(baseline, changed_prompt);
        assert_ne!(baseline, changed_setting);
    }

    #[test]
    fn expression_and_mouth_layers_are_composed_with_limit() {
        let neutral = RgbaImage::from_pixel(2, 1, image::Rgba([100, 100, 100, 255]));
        let mut eyes = neutral.clone();
        eyes.put_pixel(0, 0, image::Rgba([140, 100, 100, 255]));
        let mut mouth = neutral.clone();
        mouth.put_pixel(1, 0, image::Rgba([100, 180, 100, 255]));
        let result = compose_expression_layers(&neutral, &eyes, &mouth, 0.5).unwrap();
        assert_eq!(result.get_pixel(0, 0).0, [140, 100, 100, 255]);
        assert_eq!(result.get_pixel(1, 0).0, [100, 140, 100, 255]);
        assert!(compose_expression_layers(&neutral, &eyes, &mouth, 1.1).is_err());
    }

    #[test]
    fn face_mask_rejects_an_intentionally_shifted_change() {
        let (mesh, frame, settings) = fixture();
        let neutral = RgbaImage::from_pixel(64, 64, image::Rgba([120, 100, 80, 255]));
        let mut shifted = neutral.clone();
        for y in 0..8 {
            for x in 0..8 {
                shifted.put_pixel(x, y, image::Rgba([220, 20, 20, 255]));
            }
        }
        let atlas = RgbaImage::from_pixel(64, 64, image::Rgba([10, 10, 10, 255]));
        let diagnostics = tempdir().unwrap();
        let error = project_face_patch(
            &mesh,
            &neutral,
            &shifted,
            &atlas,
            &frame,
            &settings,
            diagnostics.path(),
        )
        .unwrap_err();
        assert!(matches!(error, FacePatchError::NoPixels { .. }));
    }
}
