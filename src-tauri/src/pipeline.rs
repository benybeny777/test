use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::{SystemTime, UNIX_EPOCH},
};

use image::{DynamicImage, ImageFormat, RgbaImage};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::Emitter;
use thiserror::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    config::AppConfig,
    facepatch::{
        ProjectionSettings, compose_expression_layers, create_capture_frame, load_neutral_snapshot,
        project_face_patch, projection_signature, render_neutral_capture,
    },
    sidecar::{SidecarError, SidecarProcess},
    store,
};

pub const STAGES: &[&str] = &["mesh", "rig", "capture", "expression", "facepatch"];
const BUILT_IN_EXPRESSIONS: &[(&str, &str, &str)] = &[
    ("smile", "笑顔", "smile, happy"),
    ("blink", "閉眼", "both eyelids shut"),
    ("angry", "怒り", "angry, furrowed brow"),
    ("sad", "悲しみ", "sad, worried eyebrows"),
    ("surprised", "驚き", "surprised, wide eyes"),
];
const VISEMES: &[&str] = &["a", "i", "u", "e", "o", "close"];

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("ファイル操作に失敗しました: {0}")]
    Io(#[from] std::io::Error),
    #[error("状態JSONが不正です: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Sidecar(#[from] SidecarError),
    #[error("画像処理に失敗しました: {0}")]
    Image(#[from] image::ImageError),
    #[error("フェイスパッチ処理に失敗しました: {0}")]
    FacePatch(#[from] crate::facepatch::FacePatchError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpressionDefinition {
    pub key: String,
    pub label: String,
    pub prompt: String,
    pub is_built_in: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArmPose {
    pub left_upper_arm: [f32; 3],
    pub left_lower_arm: [f32; 3],
    pub right_upper_arm: [f32; 3],
    pub right_lower_arm: [f32; 3],
    pub head: [f32; 3],
}

impl Default for ArmPose {
    fn default() -> Self {
        Self {
            left_upper_arm: [0.0; 3],
            left_lower_arm: [0.0; 3],
            right_upper_arm: [0.0; 3],
            right_lower_arm: [0.0; 3],
            head: [0.0; 3],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Framing {
    pub yaw: f32,
    pub pitch: f32,
    pub scale: f32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub arm_pose: ArmPose,
}

impl Default for Framing {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
            scale: 1.0,
            offset_x: 0.0,
            offset_y: 0.0,
            arm_pose: ArmPose::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterManifest {
    pub schema_version: u32,
    pub character_id: String,
    pub display_name: String,
    pub created_at_iso: String,
    pub updated_at_iso: String,
    pub persona_prompt: String,
    pub identity_tags: String,
    pub model: BTreeMap<String, String>,
    pub expressions: Vec<ExpressionDefinition>,
    pub framings: BTreeMap<String, Framing>,
    pub stages: BTreeMap<String, StageState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageState {
    pub status: String,
    pub message: String,
    pub updated_at_iso: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewCharacter {
    pub display_name: String,
    pub source_path: String,
    pub persona_prompt: String,
    pub identity_tags: String,
}

#[derive(Clone)]
pub struct PipelineContext {
    pub characters_root: PathBuf,
    pub repository_root: PathBuf,
}

impl PipelineContext {
    pub fn character_dir(&self, id: &str) -> Result<PathBuf, PipelineError> {
        validate_key(id, "characterId")?;
        Ok(self.characters_root.join(id))
    }

    pub fn create_character(
        &self,
        input: NewCharacter,
    ) -> Result<CharacterManifest, PipelineError> {
        if input.display_name.trim().is_empty() {
            return Err(PipelineError::Invalid("表示名を入力してください".into()));
        }
        let source = PathBuf::from(&input.source_path);
        if !source.is_file() {
            return Err(PipelineError::Invalid(format!(
                "入力画像が見つかりません: {}",
                source.display()
            )));
        }
        let now = now_iso();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let digest = Sha256::digest(format!("{}:{nonce}", source.display()).as_bytes());
        let id = format!("c_{}", &format!("{digest:x}")[..12]);
        let directory = self.character_dir(&id)?;
        let source_target = directory.join("source/input.png");
        store::write_bytes_atomic(&source_target, &fs::read(source)?)?;
        let mut framings = BTreeMap::new();
        framings.insert("green_screen".into(), Framing::default());
        let expressions = BUILT_IN_EXPRESSIONS
            .iter()
            .map(|(key, label, prompt)| ExpressionDefinition {
                key: (*key).into(),
                label: (*label).into(),
                prompt: (*prompt).into(),
                is_built_in: true,
            })
            .collect();
        let manifest = CharacterManifest {
            schema_version: 1,
            character_id: id,
            display_name: input.display_name,
            created_at_iso: now.clone(),
            updated_at_iso: now,
            persona_prompt: input.persona_prompt,
            identity_tags: input.identity_tags,
            model: BTreeMap::from([
                ("mesh".into(), "model/mesh.glb".into()),
                ("rigged".into(), "model/rigged.vrm".into()),
                ("thumbnail".into(), "model/thumbnail.png".into()),
            ]),
            expressions,
            framings,
            stages: BTreeMap::new(),
        };
        self.save_character(&manifest)?;
        Ok(manifest)
    }

    pub fn list_characters(&self) -> Result<Vec<CharacterManifest>, PipelineError> {
        if !self.characters_root.exists() {
            return Ok(Vec::new());
        }
        let mut values = Vec::new();
        for entry in fs::read_dir(&self.characters_root)? {
            let path = entry?.path().join("character.json");
            if path.is_file() {
                values.push(self.load_character_file(&path)?);
            }
        }
        values.sort_by(|a, b| b.updated_at_iso.cmp(&a.updated_at_iso));
        Ok(values)
    }

    pub fn load_character(&self, id: &str) -> Result<CharacterManifest, PipelineError> {
        self.load_character_file(&self.character_dir(id)?.join("character.json"))
    }

    fn load_character_file(&self, path: &Path) -> Result<CharacterManifest, PipelineError> {
        let bytes = fs::read(path)?;
        match serde_json::from_slice(&bytes) {
            Ok(value) => Ok(value),
            Err(error) => {
                let backup = path.with_extension(format!("json.corrupt.{}", unix_seconds()));
                fs::rename(path, &backup)?;
                Err(PipelineError::Invalid(format!(
                    "壊れたキャラクター状態を {} へ退避しました: {error}",
                    backup.display()
                )))
            }
        }
    }

    pub fn save_character(&self, manifest: &CharacterManifest) -> Result<(), PipelineError> {
        validate_key(&manifest.character_id, "characterId")?;
        for expression in &manifest.expressions {
            validate_key(&expression.key, "exprKey")?;
        }
        store::write_json_atomic(
            &self
                .character_dir(&manifest.character_id)?
                .join("character.json"),
            manifest,
        )?;
        Ok(())
    }

    pub fn add_expression(
        &self,
        id: &str,
        label: String,
        prompt: String,
    ) -> Result<CharacterManifest, PipelineError> {
        if label.trim().is_empty() || prompt.trim().is_empty() {
            return Err(PipelineError::Invalid(
                "表情の表示名とプロンプトを入力してください".into(),
            ));
        }
        let key = format!(
            "e_{}",
            &format!("{:x}", Sha256::digest(prompt.as_bytes()))[..8]
        );
        let mut manifest = self.load_character(id)?;
        if !manifest.expressions.iter().any(|value| value.key == key) {
            manifest.expressions.push(ExpressionDefinition {
                key,
                label,
                prompt,
                is_built_in: false,
            });
        }
        manifest.updated_at_iso = now_iso();
        self.save_character(&manifest)?;
        Ok(manifest)
    }

    pub fn remove_expression(
        &self,
        id: &str,
        key: &str,
    ) -> Result<CharacterManifest, PipelineError> {
        validate_key(key, "exprKey")?;
        let mut manifest = self.load_character(id)?;
        if manifest
            .expressions
            .iter()
            .find(|value| value.key == key)
            .is_some_and(|value| value.is_built_in)
        {
            return Err(PipelineError::Invalid(
                "組み込み表情は削除できません".into(),
            ));
        }
        manifest.expressions.retain(|value| value.key != key);
        let directory = self.character_dir(id)?.join("facepatch");
        for path in [
            directory.join("expr/eyes").join(format!("{key}.png")),
            directory
                .join("expr/eyes")
                .join(format!("{key}.import.json")),
            directory.join("projected").join(key),
        ] {
            if path.is_dir() {
                fs::remove_dir_all(path)?;
            } else if path.exists() {
                fs::remove_file(path)?;
            }
        }
        manifest.updated_at_iso = now_iso();
        self.save_character(&manifest)?;
        Ok(manifest)
    }

    pub fn update_framing(
        &self,
        id: &str,
        name: &str,
        framing: Framing,
    ) -> Result<CharacterManifest, PipelineError> {
        validate_key(name, "framingId")?;
        if !(-180.0..=180.0).contains(&framing.yaw)
            || !(-90.0..=90.0).contains(&framing.pitch)
            || !(0.1..=4.0).contains(&framing.scale)
            || framing.offset_x.abs() > 3.0
            || framing.offset_y.abs() > 3.0
        {
            return Err(PipelineError::Invalid("構図設定が範囲外です".into()));
        }
        let mut manifest = self.load_character(id)?;
        manifest.framings.insert(name.into(), framing);
        manifest.updated_at_iso = now_iso();
        self.save_character(&manifest)?;
        Ok(manifest)
    }

    pub fn generate_background(
        &self,
        config: &AppConfig,
        id: &str,
        background_id: &str,
        prompt: &str,
    ) -> Result<String, PipelineError> {
        validate_key(background_id, "bgId")?;
        if prompt.trim().is_empty() {
            return Err(PipelineError::Invalid(
                "背景プロンプトを入力してください".into(),
            ));
        }
        let path = self
            .character_dir(id)?
            .join("backgrounds")
            .join(format!("{background_id}.png"));
        let arguments = vec![
            OsString::from(self.repository_root.join("sidecar/background/generate.py")),
            "--prompt".into(),
            prompt.into(),
            "--output".into(),
            OsString::from(&path),
            "--port".into(),
            config.comfy.port.to_string().into(),
            "--startup-timeout".into(),
            config.comfy.startup_timeout_seconds.to_string().into(),
            "--workflow".into(),
            OsString::from(
                self.repository_root
                    .join(&config.comfy.workflow_dir)
                    .join("background-txt2img-api.json"),
            ),
        ];
        self.run_sidecar(arguments, |_| {})?;
        Ok(path.to_string_lossy().into_owned())
    }

    pub fn import_expression(
        &self,
        config: &AppConfig,
        id: &str,
        input: &Path,
        kind: &str,
        key: &str,
    ) -> Result<(), PipelineError> {
        validate_key(key, "exprKey")?;
        if !matches!(kind, "eyes" | "mouth") {
            return Err(PipelineError::Invalid(
                "表情画像の種別はeyesまたはmouthです".into(),
            ));
        }
        let directory = self.character_dir(id)?;
        let args = vec![
            OsString::from(
                self.repository_root
                    .join("sidecar/expression/import_image.py"),
            ),
            "--neutral".into(),
            OsString::from(directory.join("facepatch/neutral.png")),
            "--input".into(),
            OsString::from(input),
            "--output".into(),
            OsString::from(directory.join("facepatch/expr")),
            "--kind".into(),
            kind.into(),
            "--key".into(),
            key.into(),
            "--alignment-tolerance".into(),
            config.import.alignment_tolerance.to_string().into(),
            "--color-tolerance".into(),
            config.import.color_tolerance.to_string().into(),
        ];
        let mut args = args;
        if !config.import.check_alignment {
            args.push("--skip-alignment-check".into());
        }
        if !config.import.check_mirrored {
            args.push("--skip-mirror-check".into());
        }
        self.run_sidecar(args, |_| {})?;
        Ok(())
    }

    pub fn run_stage(
        &self,
        app: Option<&tauri::AppHandle>,
        config: &AppConfig,
        id: &str,
        stage: &str,
    ) -> Result<CharacterManifest, PipelineError> {
        if !STAGES.contains(&stage) {
            return Err(PipelineError::Invalid(format!("未知の工程です: {stage}")));
        }
        let mut manifest = self.load_character(id)?;
        set_stage(&mut manifest, stage, "running", "実行中");
        self.save_character(&manifest)?;
        let result = match stage {
            "mesh" => self.run_mesh(app, config, &manifest),
            "rig" => self.run_rig(app, &manifest),
            "capture" => self.run_capture(config, &manifest),
            "expression" => self.run_expression(app, config, &manifest),
            "facepatch" => self.run_facepatch(config, &manifest),
            _ => unreachable!(),
        };
        match result {
            Ok(message) => {
                set_stage(&mut manifest, stage, "complete", &message);
                manifest.updated_at_iso = now_iso();
                self.save_character(&manifest)?;
                Ok(manifest)
            }
            Err(error) => {
                set_stage(&mut manifest, stage, "failed", &error.to_string());
                manifest.updated_at_iso = now_iso();
                self.save_character(&manifest)?;
                Err(error)
            }
        }
    }

    fn run_mesh(
        &self,
        app: Option<&tauri::AppHandle>,
        config: &AppConfig,
        manifest: &CharacterManifest,
    ) -> Result<String, PipelineError> {
        let directory = self.character_dir(&manifest.character_id)?;
        let args = vec![
            OsString::from(self.repository_root.join("sidecar/mesh/generate.py")),
            "--input".into(),
            OsString::from(directory.join("source/input.png")),
            "--output".into(),
            OsString::from(directory.join("model")),
            "--model".into(),
            OsString::from(
                self.repository_root
                    .join(&config.ai.models_dir)
                    .join(&config.ai.mesh_model),
            ),
            "--mc-resolution".into(),
            config.pipeline.mesh_resolution.to_string().into(),
            "--texture-resolution".into(),
            config.pipeline.atlas_resolution.to_string().into(),
        ];
        self.run_sidecar(args, |value| {
            if let Some(app) = app {
                let _ = app.emit("pipeline-progress", value);
            }
        })?;
        let isolated = directory.join("model/foreground.png");
        store::write_bytes_atomic(&directory.join("source/isolated.png"), &fs::read(isolated)?)?;
        Ok("背景除去と3Dメッシュ生成が完了しました".into())
    }

    fn run_rig(
        &self,
        app: Option<&tauri::AppHandle>,
        manifest: &CharacterManifest,
    ) -> Result<String, PipelineError> {
        let directory = self.character_dir(&manifest.character_id)?;
        let args = vec![
            OsString::from(self.repository_root.join("sidecar/rigging/generate.py")),
            "--input".into(),
            OsString::from(directory.join("model/mesh.glb")),
            "--output".into(),
            OsString::from(directory.join("model")),
            "--name".into(),
            manifest.display_name.clone().into(),
        ];
        self.run_sidecar(args, |value| {
            if let Some(app) = app {
                let _ = app.emit("pipeline-progress", value);
            }
        })?;
        Ok("自動リギングとVRM出力が完了しました".into())
    }

    fn run_capture(
        &self,
        config: &AppConfig,
        manifest: &CharacterManifest,
    ) -> Result<String, PipelineError> {
        let directory = self.character_dir(&manifest.character_id)?;
        let mesh = load_neutral_snapshot(&directory.join("model/rigged.vrm"))?;
        let atlas = image::open(directory.join("model/texture.png"))?.into_rgba8();
        let frame = create_capture_frame(&mesh, config.pipeline.capture_resolution)?;
        let neutral = render_neutral_capture(&mesh, &atlas, &frame)?;
        save_png_atomic(&directory.join("facepatch/neutral.png"), &neutral)?;
        save_png_atomic(&directory.join("model/thumbnail.png"), &neutral)?;
        store::write_json_atomic(&directory.join("facepatch/capture_frame.json"), &frame)?;
        Ok("中立顔キャプチャと投影座標を保存しました".into())
    }

    fn run_expression(
        &self,
        app: Option<&tauri::AppHandle>,
        config: &AppConfig,
        manifest: &CharacterManifest,
    ) -> Result<String, PipelineError> {
        let directory = self.character_dir(&manifest.character_id)?;
        let mut args = vec![
            OsString::from(self.repository_root.join("sidecar/expression/generate.py")),
            "--input".into(),
            OsString::from(directory.join("facepatch/neutral.png")),
            "--output".into(),
            OsString::from(directory.join("facepatch/expr")),
            "--port".into(),
            config.comfy.port.to_string().into(),
            "--startup-timeout".into(),
            config.comfy.startup_timeout_seconds.to_string().into(),
            "--denoise".into(),
            config.ai.image_denoise.to_string().into(),
            "--blink-denoise".into(),
            config.ai.blink_denoise.to_string().into(),
            "--identity-tags".into(),
            manifest.identity_tags.clone().into(),
            "--workflow".into(),
            OsString::from(
                self.repository_root
                    .join(&config.comfy.workflow_dir)
                    .join("expression-inpaint-api.json"),
            ),
        ];
        for expression in manifest
            .expressions
            .iter()
            .filter(|value| !value.is_built_in)
        {
            args.push("--custom-expression".into());
            args.push(format!("{}={}", expression.key, expression.prompt).into());
        }
        self.run_sidecar(args, |value| {
            if let Some(app) = app {
                let _ = app.emit("pipeline-progress", value);
            }
        })?;
        Ok(format!(
            "{}枚の直交表情レイヤーを生成しました",
            manifest.expressions.len() + VISEMES.len()
        ))
    }

    fn run_facepatch(
        &self,
        config: &AppConfig,
        manifest: &CharacterManifest,
    ) -> Result<String, PipelineError> {
        let directory = self.character_dir(&manifest.character_id)?;
        let mesh = load_neutral_snapshot(&directory.join("model/rigged.vrm"))?;
        let neutral = image::open(directory.join("facepatch/neutral.png"))?.into_rgba8();
        let atlas = image::open(directory.join("model/texture.png"))?.into_rgba8();
        let frame =
            serde_json::from_slice(&fs::read(directory.join("facepatch/capture_frame.json"))?)?;
        let settings = ProjectionSettings {
            atlas_resolution: config.pipeline.atlas_resolution,
            diff_threshold: config.facepatch.diff_threshold,
            alpha_threshold: config.facepatch.alpha_threshold,
            seam_padding: config.facepatch.seam_padding,
            head_weight_threshold: config.facepatch.head_weight_threshold,
            normal_threshold: config.facepatch.normal_threshold,
            max_ray_hits: config.facepatch.max_ray_hits,
            depth_window: config.facepatch.depth_window,
            face_mask_center: config.facepatch.face_mask_center,
            face_mask_radius: config.facepatch.face_mask_radius,
        };
        let expression_root = directory.join("facepatch/expr");
        let output_root = directory.join("facepatch/projected");
        let diagnostics_root = directory.join("facepatch/diagnostics");
        let mut count = 0_u32;
        for expression in &manifest.expressions {
            let eye_path = expression_root
                .join("eyes")
                .join(format!("{}.png", expression.key));
            if !eye_path.exists() {
                continue;
            }
            let eye = image::open(eye_path)?.into_rgba8();
            for viseme in VISEMES {
                let mouth =
                    image::open(expression_root.join("mouth").join(format!("{viseme}.png")))?
                        .into_rgba8();
                let scale = match expression.key.as_str() {
                    "sad" => 0.55,
                    "angry" => 0.72,
                    "blink" => 0.65,
                    _ => 1.0,
                };
                let composite = compose_expression_layers(&neutral, &eye, &mouth, scale)?;
                let (projected, stats) = project_face_patch(
                    &mesh,
                    &neutral,
                    &composite,
                    &atlas,
                    &frame,
                    &settings,
                    &diagnostics_root.join(&expression.key).join(viseme),
                )?;
                if stats.written_pixels == 0 {
                    return Err(PipelineError::Invalid("投影画素が0です".into()));
                }
                save_png_atomic(
                    &output_root
                        .join(&expression.key)
                        .join(format!("{viseme}.png")),
                    &projected,
                )?;
                count += 1;
            }
        }
        if count == 0 {
            return Err(PipelineError::Invalid(
                "投影できる目・眉レイヤーがありません".into(),
            ));
        }
        let signature = projection_signature(
            config.facepatch.pipeline_version,
            &mesh,
            &frame,
            &settings,
            &format!("{:x}", Sha256::digest(manifest.identity_tags.as_bytes())),
        )?;
        store::write_bytes_atomic(
            &directory.join("facepatch/signature.txt"),
            signature.as_bytes(),
        )?;
        Ok(format!("{count}組の表情アトラスを逆投影しました"))
    }

    fn run_sidecar(
        &self,
        arguments: Vec<OsString>,
        relay: impl FnMut(serde_json::Value),
    ) -> Result<(), PipelineError> {
        let python = self
            .repository_root
            .join("sidecar/.venv/Scripts/python.exe");
        if !python.is_file() {
            return Err(PipelineError::Invalid(
                "Python環境がありません。cargo xtask setup sidecar を実行してください".into(),
            ));
        }
        SidecarProcess::spawn(&python, &arguments, &self.repository_root)?
            .relay_json_lines(Arc::new(AtomicBool::new(false)), relay)?;
        Ok(())
    }
}

fn set_stage(manifest: &mut CharacterManifest, stage: &str, status: &str, message: &str) {
    manifest.stages.insert(
        stage.into(),
        StageState {
            status: status.into(),
            message: message.into(),
            updated_at_iso: now_iso(),
        },
    );
}

fn validate_key(value: &str, name: &str) -> Result<(), PipelineError> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte))
    {
        return Err(PipelineError::Invalid(format!(
            "{name}はASCII小文字・数字・_・-だけにしてください"
        )));
    }
    Ok(())
}

fn save_png_atomic(path: &Path, image: &RgbaImage) -> Result<(), PipelineError> {
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image.clone()).write_to(&mut bytes, ImageFormat::Png)?;
    store::write_bytes_atomic(path, bytes.get_ref())?;
    Ok(())
}

fn now_iso() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| format!("unix:{}", unix_seconds()))
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn sample_image() -> RgbaImage {
        RgbaImage::from_pixel(8, 8, Rgba([12, 34, 56, 255]))
    }

    #[test]
    fn character_state_is_atomic_and_ascii_keyed() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("input.png");
        sample_image().save(&source).unwrap();
        let context = PipelineContext {
            characters_root: root.path().join("characters"),
            repository_root: root.path().to_owned(),
        };
        let character = context
            .create_character(NewCharacter {
                display_name: "女性テスト".into(),
                source_path: source.to_string_lossy().into_owned(),
                persona_prompt: "明るい".into(),
                identity_tags: "green hair".into(),
            })
            .unwrap();
        assert!(character.character_id.starts_with("c_"));
        assert!(
            context
                .character_dir(&character.character_id)
                .unwrap()
                .join("source/input.png")
                .is_file()
        );
        assert_eq!(context.list_characters().unwrap().len(), 1);
    }

    #[test]
    fn expression_key_is_stable_and_builtins_are_protected() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("input.png");
        sample_image().save(&source).unwrap();
        let context = PipelineContext {
            characters_root: root.path().join("characters"),
            repository_root: root.path().to_owned(),
        };
        let character = context
            .create_character(NewCharacter {
                display_name: "女性テスト".into(),
                source_path: source.to_string_lossy().into_owned(),
                persona_prompt: String::new(),
                identity_tags: String::new(),
            })
            .unwrap();
        let first = context
            .add_expression(
                &character.character_id,
                "ジト目".into(),
                "half closed".into(),
            )
            .unwrap();
        let second = context
            .add_expression(
                &character.character_id,
                "ジト目".into(),
                "half closed".into(),
            )
            .unwrap();
        assert_eq!(first.expressions.len(), second.expressions.len());
        assert!(
            context
                .remove_expression(&character.character_id, "smile")
                .is_err()
        );
    }

    #[test]
    fn background_rejects_empty_prompt_before_starting_comfyui() {
        let root = tempfile::tempdir().unwrap();
        let context = PipelineContext {
            characters_root: root.path().join("characters"),
            repository_root: root.path().to_owned(),
        };
        fs::create_dir_all(context.character_dir("c_test").unwrap()).unwrap();
        let error = context
            .generate_background(&AppConfig::default(), "c_test", "night", "")
            .unwrap_err();
        assert!(error.to_string().contains("背景プロンプト"));
    }
}
