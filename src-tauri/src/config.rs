use std::{env, ffi::OsString, path::Path};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::store;

pub const SETTING_KEYS: &[&str] = &[
    "ai.blink_denoise",
    "ai.image_denoise",
    "ai.llm_model",
    "ai.mesh_model",
    "ai.models_dir",
    "ai.sam2_model",
    "ai.grounding_model",
    "ai.grounding_threshold",
    "ai.eye_context_margin",
    "ai.sam2_points_per_batch",
    "ai.sam2_pred_iou_threshold",
    "ai.sam2_stability_threshold",
    "ai.stt_model",
    "avatar.blink_duration_ms",
    "avatar.blink_max_ms",
    "avatar.blink_min_ms",
    "avatar.crossfade_ms",
    "avatar.idle_sway_degrees",
    "avatar.idle_sway_period_ms",
    "comfy.port",
    "comfy.startup_timeout_seconds",
    "comfy.unload_before_mesh",
    "comfy.workflow_dir",
    "display.language",
    "display.preview_fps",
    "display.preview_scale",
    "facepatch.alpha_threshold",
    "facepatch.depth_window",
    "facepatch.diff_threshold",
    "facepatch.face_mask_center",
    "facepatch.face_mask_radius",
    "facepatch.head_weight_threshold",
    "facepatch.max_ray_hits",
    "facepatch.normal_threshold",
    "facepatch.pipeline_version",
    "facepatch.seam_padding",
    "import.alignment_tolerance",
    "import.check_alignment",
    "import.check_mirrored",
    "import.color_tolerance",
    "lipsync.a_shape_bias",
    "lipsync.device_name",
    "lipsync.formants",
    "lipsync.interval_seconds",
    "lipsync.sample_rate",
    "lipsync.silence_hold_seconds",
    "lipsync.smoothing_frames",
    "lipsync.volume_gate_db",
    "lipsync.window_samples",
    "pipeline.atlas_resolution",
    "pipeline.capture_resolution",
    "pipeline.keep_intermediates",
    "pipeline.mesh_resolution",
    "pipeline.output_dir",
    "vad.enabled",
    "vad.end_silence_seconds",
    "vad.max_seconds",
    "vad.min_seconds",
];

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    pub ai: AiConfig,
    pub avatar: AvatarConfig,
    pub comfy: ComfyConfig,
    pub display: DisplayConfig,
    pub facepatch: FacePatchConfig,
    pub import: ImportConfig,
    pub lipsync: LipSyncConfig,
    pub vad: VadConfig,
    pub pipeline: PipelineConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct PipelineConfig {
    pub capture_resolution: u32,
    pub atlas_resolution: u32,
    pub mesh_resolution: u32,
    pub output_dir: String,
    pub keep_intermediates: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct FacePatchConfig {
    pub pipeline_version: u32,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AiConfig {
    pub models_dir: String,
    pub llm_model: String,
    pub stt_model: String,
    pub image_denoise: f32,
    pub blink_denoise: f32,
    pub mesh_model: String,
    pub sam2_model: String,
    pub grounding_model: String,
    pub grounding_threshold: f32,
    pub eye_context_margin: f32,
    pub sam2_points_per_batch: u32,
    pub sam2_pred_iou_threshold: f32,
    pub sam2_stability_threshold: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ComfyConfig {
    pub port: u16,
    pub startup_timeout_seconds: u32,
    pub workflow_dir: String,
    pub unload_before_mesh: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ImportConfig {
    pub check_alignment: bool,
    pub check_mirrored: bool,
    pub alignment_tolerance: f32,
    pub color_tolerance: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AvatarConfig {
    pub crossfade_ms: u32,
    pub blink_min_ms: u32,
    pub blink_max_ms: u32,
    pub blink_duration_ms: u32,
    pub idle_sway_degrees: f32,
    pub idle_sway_period_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct DisplayConfig {
    pub preview_fps: u32,
    pub preview_scale: f32,
    pub language: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ConfigFile {
    ai: Option<AiConfigFile>,
    avatar: Option<AvatarConfigFile>,
    comfy: Option<ComfyConfig>,
    display: Option<DisplayConfigFile>,
    facepatch: Option<FacePatchConfig>,
    import: Option<ImportConfig>,
    lipsync: Option<LipSyncConfigFile>,
    vad: Option<VadConfigFile>,
    // OBS機能廃止前の設定ファイルを壊さず読み捨てる。
    obs: Option<serde_json::Value>,
    pipeline: Option<PipelineConfig>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct AiConfigFile {
    models_dir: Option<String>,
    llm_model: Option<String>,
    stt_model: Option<String>,
    image_denoise: Option<f32>,
    blink_denoise: Option<f32>,
    mesh_model: Option<String>,
    sam2_model: Option<String>,
    grounding_model: Option<String>,
    grounding_threshold: Option<f32>,
    eye_context_margin: Option<f32>,
    sam2_points_per_batch: Option<u32>,
    sam2_pred_iou_threshold: Option<f32>,
    sam2_stability_threshold: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct AvatarConfigFile {
    crossfade_ms: Option<u32>,
    blink_min_ms: Option<u32>,
    blink_max_ms: Option<u32>,
    blink_duration_ms: Option<u32>,
    idle_sway_degrees: Option<f32>,
    idle_sway_period_ms: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct DisplayConfigFile {
    preview_fps: Option<u32>,
    preview_scale: Option<f32>,
    language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct LipSyncConfig {
    pub device_name: String,
    pub sample_rate: u32,
    pub window_samples: u32,
    pub interval_seconds: f32,
    pub volume_gate_db: f32,
    pub smoothing_frames: u32,
    pub silence_hold_seconds: f32,
    pub a_shape_bias: f32,
    pub formants: [crate::lipsync::Formant; 5],
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct LipSyncConfigFile {
    device_name: Option<String>,
    sample_rate: Option<u32>,
    window_samples: Option<u32>,
    interval_seconds: Option<f32>,
    volume_gate_db: Option<f32>,
    smoothing_frames: Option<u32>,
    silence_hold_seconds: Option<f32>,
    a_shape_bias: Option<f32>,
    formants: Option<[crate::lipsync::Formant; 5]>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct VadConfig {
    pub enabled: bool,
    pub end_silence_seconds: f32,
    pub min_seconds: f32,
    pub max_seconds: f32,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct VadConfigFile {
    enabled: Option<bool>,
    end_silence_seconds: Option<f32>,
    min_seconds: Option<f32>,
    max_seconds: Option<f32>,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            preview_fps: 30,
            preview_scale: 0.5,
            language: "ja".to_owned(),
        }
    }
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            capture_resolution: 1024,
            atlas_resolution: 2048,
            mesh_resolution: 192,
            output_dir: String::new(),
            keep_intermediates: true,
        }
    }
}

impl Default for FacePatchConfig {
    fn default() -> Self {
        let value = crate::facepatch::ProjectionSettings::default();
        Self {
            pipeline_version: 2,
            diff_threshold: value.diff_threshold,
            alpha_threshold: value.alpha_threshold,
            seam_padding: value.seam_padding,
            head_weight_threshold: value.head_weight_threshold,
            normal_threshold: value.normal_threshold,
            max_ray_hits: value.max_ray_hits,
            depth_window: value.depth_window,
            face_mask_center: value.face_mask_center,
            face_mask_radius: value.face_mask_radius,
        }
    }
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            models_dir: "models".into(),
            llm_model: "qwen2.5-1.5b-instruct-q4_k_m.gguf".into(),
            stt_model: "ggml-small.bin".into(),
            image_denoise: 0.65,
            blink_denoise: 0.85,
            mesh_model: "triposr".into(),
            sam2_model: "sam2.1-hiera-tiny".into(),
            grounding_model: "grounding-dino-base".into(),
            grounding_threshold: 0.20,
            eye_context_margin: 0.5,
            sam2_points_per_batch: 8,
            sam2_pred_iou_threshold: 0.7,
            sam2_stability_threshold: 0.85,
        }
    }
}

impl Default for ComfyConfig {
    fn default() -> Self {
        Self {
            port: 58120,
            startup_timeout_seconds: 600,
            workflow_dir: "workflows".into(),
            unload_before_mesh: true,
        }
    }
}

impl Default for ImportConfig {
    fn default() -> Self {
        Self {
            check_alignment: true,
            check_mirrored: true,
            alignment_tolerance: 12.0,
            color_tolerance: 0.08,
        }
    }
}

impl Default for AvatarConfig {
    fn default() -> Self {
        Self {
            crossfade_ms: 160,
            blink_min_ms: 2_800,
            blink_max_ms: 6_500,
            blink_duration_ms: 140,
            idle_sway_degrees: 0.7,
            idle_sway_period_ms: 4_200,
        }
    }
}

impl Default for LipSyncConfig {
    fn default() -> Self {
        let analysis = crate::lipsync::LipSyncSettings::default();
        Self {
            device_name: String::new(),
            sample_rate: analysis.sample_rate,
            window_samples: analysis.window_samples as u32,
            interval_seconds: 0.08,
            volume_gate_db: analysis.volume_gate_db,
            smoothing_frames: analysis.smoothing_frames as u32,
            silence_hold_seconds: 0.16,
            a_shape_bias: analysis.a_shape_bias,
            formants: analysis.formants,
        }
    }
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            end_silence_seconds: 0.9,
            min_seconds: 0.5,
            max_seconds: 6.0,
        }
    }
}

impl From<&LipSyncConfig> for crate::lipsync::LipSyncSettings {
    fn from(config: &LipSyncConfig) -> Self {
        Self {
            sample_rate: config.sample_rate,
            window_samples: config.window_samples as usize,
            volume_gate_db: config.volume_gate_db,
            smoothing_frames: config.smoothing_frames as usize,
            a_shape_bias: config.a_shape_bias,
            formants: config.formants,
        }
    }
}

impl From<(&VadConfig, &LipSyncConfig)> for crate::lipsync::VadSettings {
    fn from((vad, lipsync): (&VadConfig, &LipSyncConfig)) -> Self {
        Self {
            sample_rate: lipsync.sample_rate,
            volume_gate_db: lipsync.volume_gate_db,
            end_silence_seconds: vad.end_silence_seconds,
            min_seconds: vad.min_seconds,
            max_seconds: vad.max_seconds,
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("設定ファイルの読み込みに失敗しました: {0}")]
    Io(#[from] std::io::Error),
    #[error("設定JSONが不正です: {0}")]
    Json(#[from] serde_json::Error),
    #[error("設定値が不正です: {0}")]
    Validation(String),
}

impl AppConfig {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        Self::load_with_environment(path, |key| env::var_os(key))
    }

    fn load_with_environment(
        path: &Path,
        environment: impl Fn(&str) -> Option<OsString>,
    ) -> Result<Self, ConfigError> {
        let mut config = Self::default();
        config.apply_environment(environment)?;

        if path.exists() {
            let bytes = std::fs::read(path)?;
            match serde_json::from_slice::<ConfigFile>(&bytes) {
                Ok(file) => config.apply_file(file),
                Err(error) => {
                    backup_corrupt_file(path)?;
                    eprintln!("設定ファイルを退避して既定値で起動します: {error}");
                }
            }
        }

        config.validate()?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        self.validate()?;
        store::write_json_atomic(path, self)?;
        Ok(())
    }

    fn apply_environment(
        &mut self,
        environment: impl Fn(&str) -> Option<OsString>,
    ) -> Result<(), ConfigError> {
        if let Some(value) = environment("LVS_DISPLAY_PREVIEW_FPS") {
            self.display.preview_fps = value.to_string_lossy().parse().map_err(|_| {
                ConfigError::Validation("LVS_DISPLAY_PREVIEW_FPS は u32 で指定してください".into())
            })?;
        }
        if let Some(value) = environment("LVS_DISPLAY_PREVIEW_SCALE") {
            self.display.preview_scale = value.to_string_lossy().parse().map_err(|_| {
                ConfigError::Validation(
                    "LVS_DISPLAY_PREVIEW_SCALE は f32 で指定してください".into(),
                )
            })?;
        }
        if let Some(value) = environment("LVS_DISPLAY_LANGUAGE") {
            self.display.language = value.to_string_lossy().into_owned();
        }
        macro_rules! parse_environment {
            ($name:literal, $target:expr, $type:ty) => {
                if let Some(value) = environment($name) {
                    $target = value.to_string_lossy().parse::<$type>().map_err(|_| {
                        ConfigError::Validation(format!("{} の形式が不正です", $name))
                    })?;
                }
            };
        }
        macro_rules! string_environment {
            ($name:literal, $target:expr) => {
                if let Some(value) = environment($name) {
                    $target = value.to_string_lossy().into_owned();
                }
            };
        }
        parse_environment!("LVS_AVATAR_CROSSFADE_MS", self.avatar.crossfade_ms, u32);
        parse_environment!("LVS_AVATAR_BLINK_MIN_MS", self.avatar.blink_min_ms, u32);
        parse_environment!("LVS_AVATAR_BLINK_MAX_MS", self.avatar.blink_max_ms, u32);
        parse_environment!(
            "LVS_AVATAR_BLINK_DURATION_MS",
            self.avatar.blink_duration_ms,
            u32
        );
        parse_environment!(
            "LVS_AVATAR_IDLE_SWAY_DEGREES",
            self.avatar.idle_sway_degrees,
            f32
        );
        parse_environment!(
            "LVS_AVATAR_IDLE_SWAY_PERIOD_MS",
            self.avatar.idle_sway_period_ms,
            u32
        );
        parse_environment!("LVS_COMFY_PORT", self.comfy.port, u16);
        parse_environment!(
            "LVS_COMFY_STARTUP_TIMEOUT_SECONDS",
            self.comfy.startup_timeout_seconds,
            u32
        );
        parse_environment!(
            "LVS_COMFY_UNLOAD_BEFORE_MESH",
            self.comfy.unload_before_mesh,
            bool
        );
        parse_environment!(
            "LVS_PIPELINE_CAPTURE_RESOLUTION",
            self.pipeline.capture_resolution,
            u32
        );
        parse_environment!(
            "LVS_PIPELINE_ATLAS_RESOLUTION",
            self.pipeline.atlas_resolution,
            u32
        );
        parse_environment!(
            "LVS_PIPELINE_MESH_RESOLUTION",
            self.pipeline.mesh_resolution,
            u32
        );
        parse_environment!(
            "LVS_PIPELINE_KEEP_INTERMEDIATES",
            self.pipeline.keep_intermediates,
            bool
        );
        string_environment!("LVS_PIPELINE_OUTPUT_DIR", self.pipeline.output_dir);
        string_environment!("LVS_AI_MODELS_DIR", self.ai.models_dir);
        string_environment!("LVS_AI_LLM_MODEL", self.ai.llm_model);
        string_environment!("LVS_AI_STT_MODEL", self.ai.stt_model);
        string_environment!("LVS_AI_MESH_MODEL", self.ai.mesh_model);
        string_environment!("LVS_AI_SAM2_MODEL", self.ai.sam2_model);
        string_environment!("LVS_AI_GROUNDING_MODEL", self.ai.grounding_model);
        parse_environment!(
            "LVS_AI_GROUNDING_THRESHOLD",
            self.ai.grounding_threshold,
            f32
        );
        parse_environment!("LVS_AI_EYE_CONTEXT_MARGIN", self.ai.eye_context_margin, f32);
        parse_environment!(
            "LVS_AI_SAM2_POINTS_PER_BATCH",
            self.ai.sam2_points_per_batch,
            u32
        );
        parse_environment!(
            "LVS_AI_SAM2_PRED_IOU_THRESHOLD",
            self.ai.sam2_pred_iou_threshold,
            f32
        );
        parse_environment!(
            "LVS_AI_SAM2_STABILITY_THRESHOLD",
            self.ai.sam2_stability_threshold,
            f32
        );
        string_environment!("LVS_COMFY_WORKFLOW_DIR", self.comfy.workflow_dir);
        parse_environment!("LVS_AI_IMAGE_DENOISE", self.ai.image_denoise, f32);
        parse_environment!("LVS_AI_BLINK_DENOISE", self.ai.blink_denoise, f32);
        parse_environment!(
            "LVS_IMPORT_CHECK_ALIGNMENT",
            self.import.check_alignment,
            bool
        );
        parse_environment!(
            "LVS_IMPORT_CHECK_MIRRORED",
            self.import.check_mirrored,
            bool
        );
        parse_environment!(
            "LVS_IMPORT_ALIGNMENT_TOLERANCE",
            self.import.alignment_tolerance,
            f32
        );
        parse_environment!(
            "LVS_IMPORT_COLOR_TOLERANCE",
            self.import.color_tolerance,
            f32
        );
        if let Some(value) = environment("LVS_LIPSYNC_DEVICE_NAME") {
            self.lipsync.device_name = value.to_string_lossy().into_owned();
        }
        parse_environment!("LVS_LIPSYNC_SAMPLE_RATE", self.lipsync.sample_rate, u32);
        parse_environment!(
            "LVS_LIPSYNC_WINDOW_SAMPLES",
            self.lipsync.window_samples,
            u32
        );
        parse_environment!(
            "LVS_LIPSYNC_INTERVAL_SECONDS",
            self.lipsync.interval_seconds,
            f32
        );
        parse_environment!(
            "LVS_LIPSYNC_VOLUME_GATE_DB",
            self.lipsync.volume_gate_db,
            f32
        );
        parse_environment!(
            "LVS_LIPSYNC_SMOOTHING_FRAMES",
            self.lipsync.smoothing_frames,
            u32
        );
        parse_environment!(
            "LVS_LIPSYNC_SILENCE_HOLD_SECONDS",
            self.lipsync.silence_hold_seconds,
            f32
        );
        parse_environment!("LVS_LIPSYNC_A_SHAPE_BIAS", self.lipsync.a_shape_bias, f32);
        parse_environment!("LVS_VAD_ENABLED", self.vad.enabled, bool);
        parse_environment!(
            "LVS_VAD_END_SILENCE_SECONDS",
            self.vad.end_silence_seconds,
            f32
        );
        parse_environment!("LVS_VAD_MIN_SECONDS", self.vad.min_seconds, f32);
        parse_environment!("LVS_VAD_MAX_SECONDS", self.vad.max_seconds, f32);
        Ok(())
    }

    fn apply_file(&mut self, file: ConfigFile) {
        if let Some(value) = file.ai {
            apply_optional(&mut self.ai.models_dir, value.models_dir);
            apply_optional(&mut self.ai.llm_model, value.llm_model);
            apply_optional(&mut self.ai.stt_model, value.stt_model);
            apply_optional(&mut self.ai.image_denoise, value.image_denoise);
            apply_optional(&mut self.ai.blink_denoise, value.blink_denoise);
            apply_optional(&mut self.ai.mesh_model, value.mesh_model);
            apply_optional(&mut self.ai.sam2_model, value.sam2_model);
            apply_optional(&mut self.ai.grounding_model, value.grounding_model);
            apply_optional(&mut self.ai.grounding_threshold, value.grounding_threshold);
            apply_optional(&mut self.ai.eye_context_margin, value.eye_context_margin);
            apply_optional(
                &mut self.ai.sam2_points_per_batch,
                value.sam2_points_per_batch,
            );
            apply_optional(
                &mut self.ai.sam2_pred_iou_threshold,
                value.sam2_pred_iou_threshold,
            );
            apply_optional(
                &mut self.ai.sam2_stability_threshold,
                value.sam2_stability_threshold,
            );
        }
        if let Some(avatar) = file.avatar {
            apply_optional(&mut self.avatar.crossfade_ms, avatar.crossfade_ms);
            apply_optional(&mut self.avatar.blink_min_ms, avatar.blink_min_ms);
            apply_optional(&mut self.avatar.blink_max_ms, avatar.blink_max_ms);
            apply_optional(&mut self.avatar.blink_duration_ms, avatar.blink_duration_ms);
            apply_optional(&mut self.avatar.idle_sway_degrees, avatar.idle_sway_degrees);
            apply_optional(
                &mut self.avatar.idle_sway_period_ms,
                avatar.idle_sway_period_ms,
            );
        }
        if let Some(value) = file.comfy {
            self.comfy = value;
        }
        if let Some(display) = file.display {
            if let Some(value) = display.preview_fps {
                self.display.preview_fps = value;
            }
            if let Some(value) = display.preview_scale {
                self.display.preview_scale = value;
            }
            if let Some(value) = display.language {
                self.display.language = value;
            }
        }
        if let Some(value) = file.facepatch {
            self.facepatch = value;
        }
        if let Some(value) = file.import {
            self.import = value;
        }
        if let Some(lipsync) = file.lipsync {
            apply_optional(&mut self.lipsync.device_name, lipsync.device_name);
            apply_optional(&mut self.lipsync.sample_rate, lipsync.sample_rate);
            apply_optional(&mut self.lipsync.window_samples, lipsync.window_samples);
            apply_optional(&mut self.lipsync.interval_seconds, lipsync.interval_seconds);
            apply_optional(&mut self.lipsync.volume_gate_db, lipsync.volume_gate_db);
            apply_optional(&mut self.lipsync.smoothing_frames, lipsync.smoothing_frames);
            apply_optional(
                &mut self.lipsync.silence_hold_seconds,
                lipsync.silence_hold_seconds,
            );
            apply_optional(&mut self.lipsync.a_shape_bias, lipsync.a_shape_bias);
            apply_optional(&mut self.lipsync.formants, lipsync.formants);
        }
        if let Some(vad) = file.vad {
            apply_optional(&mut self.vad.enabled, vad.enabled);
            apply_optional(&mut self.vad.end_silence_seconds, vad.end_silence_seconds);
            apply_optional(&mut self.vad.min_seconds, vad.min_seconds);
            apply_optional(&mut self.vad.max_seconds, vad.max_seconds);
        }
        let _ = file.obs;
        if let Some(value) = file.pipeline {
            self.pipeline = value;
        }
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if self.pipeline.capture_resolution < 64
            || self.pipeline.capture_resolution > 4096
            || self.pipeline.atlas_resolution < 64
            || self.pipeline.atlas_resolution > 8192
            || !(32..=512).contains(&self.pipeline.mesh_resolution)
        {
            return Err(ConfigError::Validation("pipeline設定が範囲外です".into()));
        }
        if self.facepatch.pipeline_version == 0
            || !(0.0..=1.0).contains(&self.facepatch.diff_threshold)
            || !(0.0..=1.0).contains(&self.facepatch.alpha_threshold)
            || self.facepatch.max_ray_hits == 0
            || self
                .facepatch
                .face_mask_radius
                .iter()
                .any(|value| *value <= 0.0)
        {
            return Err(ConfigError::Validation("facepatch設定が範囲外です".into()));
        }
        if self.comfy.port == 8188
            || self.comfy.port == 0
            || self.comfy.startup_timeout_seconds == 0
            || !self.comfy.unload_before_mesh
            || !(0.0..=1.0).contains(&self.ai.image_denoise)
            || !(0.0..=1.0).contains(&self.ai.blink_denoise)
            || !(1..=64).contains(&self.ai.sam2_points_per_batch)
            || !(0.0..=1.0).contains(&self.ai.sam2_pred_iou_threshold)
            || !(0.0..=1.0).contains(&self.ai.grounding_threshold)
            || !(0.1..=2.0).contains(&self.ai.eye_context_margin)
            || !(0.0..=1.0).contains(&self.ai.sam2_stability_threshold)
        {
            return Err(ConfigError::Validation("AI/ComfyUI設定が範囲外です".into()));
        }
        if self.ai.models_dir.trim().is_empty()
            || self.ai.llm_model.trim().is_empty()
            || self.ai.stt_model.trim().is_empty()
            || self.ai.mesh_model.trim().is_empty()
            || self.ai.sam2_model.trim().is_empty()
            || self.ai.grounding_model.trim().is_empty()
            || self.comfy.workflow_dir.trim().is_empty()
        {
            return Err(ConfigError::Validation(
                "AI/ComfyUIのパスまたはモデル名が空です".into(),
            ));
        }
        if self.import.alignment_tolerance < 0.0
            || !(0.0..=1.0).contains(&self.import.color_tolerance)
        {
            return Err(ConfigError::Validation("import設定が範囲外です".into()));
        }
        if !self.pipeline.keep_intermediates {
            return Err(ConfigError::Validation(
                "途中再開に必要なためpipeline.keep_intermediatesはtrue固定です".into(),
            ));
        }
        if self.avatar.crossfade_ms > 5_000
            || self.avatar.blink_min_ms < 500
            || self.avatar.blink_max_ms < self.avatar.blink_min_ms
            || !(50..=2_000).contains(&self.avatar.blink_duration_ms)
            || !(0.0..=10.0).contains(&self.avatar.idle_sway_degrees)
            || !(500..=60_000).contains(&self.avatar.idle_sway_period_ms)
        {
            return Err(ConfigError::Validation("avatar設定が範囲外です".into()));
        }
        if !(1..=240).contains(&self.display.preview_fps) {
            return Err(ConfigError::Validation(
                "display.preview_fps は 1〜240 の範囲で指定してください".into(),
            ));
        }
        if !(0.1..=2.0).contains(&self.display.preview_scale) {
            return Err(ConfigError::Validation(
                "display.preview_scale は 0.1〜2.0 の範囲で指定してください".into(),
            ));
        }
        if self.display.language.trim().is_empty() {
            return Err(ConfigError::Validation(
                "display.language は空にできません".into(),
            ));
        }
        if self.lipsync.sample_rate < 8_000
            || !self.lipsync.window_samples.is_power_of_two()
            || self.lipsync.window_samples < 64
            || !(0.01..=1.0).contains(&self.lipsync.interval_seconds)
            || !(-100.0..=0.0).contains(&self.lipsync.volume_gate_db)
            || !(1..=30).contains(&self.lipsync.smoothing_frames)
            || !(0.01..=2.0).contains(&self.lipsync.silence_hold_seconds)
            || !(0.1..=2.0).contains(&self.lipsync.a_shape_bias)
            || self
                .lipsync
                .formants
                .iter()
                .any(|formant| formant.f1 <= 0.0 || formant.f2 <= formant.f1)
        {
            return Err(ConfigError::Validation("lipsync設定が範囲外です".into()));
        }
        if self.vad.end_silence_seconds <= 0.0
            || self.vad.min_seconds <= 0.0
            || self.vad.max_seconds < self.vad.min_seconds
        {
            return Err(ConfigError::Validation("vad設定が範囲外です".into()));
        }
        Ok(())
    }
}

fn apply_optional<T>(target: &mut T, value: Option<T>) {
    if let Some(value) = value {
        *target = value;
    }
}

fn backup_corrupt_file(path: &Path) -> Result<(), std::io::Error> {
    for suffix in 0..1000 {
        let extension = if suffix == 0 {
            "json.corrupt".to_owned()
        } else {
            format!("json.corrupt.{suffix}")
        };
        let backup = path.with_extension(extension);
        if !backup.exists() {
            return std::fs::rename(path, backup);
        }
    }
    Err(std::io::Error::other("破損設定の退避先を確保できません"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documented_setting_keys_match_code_in_both_directions() {
        let docs = include_str!("../../docs/SETTINGS.md");
        let begin = "<!-- implemented-settings:start -->";
        let end = "<!-- implemented-settings:end -->";
        let table = docs
            .split_once(begin)
            .and_then(|(_, rest)| rest.split_once(end).map(|(body, _)| body))
            .expect("SETTINGS.md に実装済み設定マーカーが必要です");
        let mut documented: Vec<&str> = table
            .lines()
            .filter_map(|line| line.strip_prefix("| `"))
            .filter_map(|line| line.split_once('`').map(|(key, _)| key))
            .collect();
        documented.sort_unstable();
        documented.dedup();

        let mut implemented = SETTING_KEYS.to_vec();
        implemented.sort_unstable();
        assert_eq!(documented, implemented);
    }

    #[test]
    fn persistent_file_overrides_environment() {
        let dir = std::env::temp_dir().join(format!("lvs-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let saved = AppConfig {
            display: DisplayConfig {
                preview_fps: 60,
                ..DisplayConfig::default()
            },
            ..AppConfig::default()
        };
        saved.save(&path).unwrap();

        // 永続ファイルは環境変数の後に読み込み、指定したキーを優先する。
        let loaded = AppConfig::load_with_environment(&path, |key| match key {
            "LVS_DISPLAY_PREVIEW_FPS" => Some("24".into()),
            "LVS_DISPLAY_LANGUAGE" => Some("en".into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(loaded.display.preview_fps, 60);
        assert_eq!(loaded.display.language, "ja");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn environment_overrides_defaults_when_file_is_absent() {
        let path = std::env::temp_dir().join("lvs-config-does-not-exist.json");
        let loaded = AppConfig::load_with_environment(&path, |key| match key {
            "LVS_DISPLAY_PREVIEW_FPS" => Some("48".into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(loaded.display.preview_fps, 48);
    }

    #[test]
    fn eye_context_setting_precedence_and_validation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        let environment = |key: &str| (key == "LVS_AI_EYE_CONTEXT_MARGIN").then(|| "0.75".into());
        let mut loaded = AppConfig::load_with_environment(&path, environment).unwrap();
        assert_eq!(loaded.ai.eye_context_margin, 0.75);
        loaded.ai.eye_context_margin = 0.25;
        loaded.save(&path).unwrap();
        assert_eq!(
            AppConfig::load_with_environment(&path, environment)
                .unwrap()
                .ai
                .eye_context_margin,
            0.25
        );
        for invalid in [0.0, 2.1, f32::NAN] {
            loaded.ai.eye_context_margin = invalid;
            assert!(loaded.validate().is_err());
        }
    }

    #[test]
    fn partial_ai_file_preserves_unspecified_environment_keys() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        std::fs::write(&path, r#"{"ai":{"sam2_points_per_batch":4}}"#).unwrap();
        let loaded = AppConfig::load_with_environment(&path, |key| match key {
            "LVS_AI_SAM2_POINTS_PER_BATCH" => Some("16".into()),
            "LVS_AI_SAM2_STABILITY_THRESHOLD" => Some("0.91".into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(loaded.ai.sam2_points_per_batch, 4);
        assert_eq!(loaded.ai.sam2_stability_threshold, 0.91);
    }

    #[test]
    fn environment_can_redirect_pipeline_and_model_paths() {
        let path = std::env::temp_dir().join("lvs-config-does-not-exist-paths.json");
        let loaded = AppConfig::load_with_environment(&path, |key| match key {
            "LVS_PIPELINE_OUTPUT_DIR" => Some("C:/characters".into()),
            "LVS_AI_MODELS_DIR" => Some("D:/models".into()),
            "LVS_COMFY_WORKFLOW_DIR" => Some("custom-workflows".into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(loaded.pipeline.output_dir, "C:/characters");
        assert_eq!(loaded.ai.models_dir, "D:/models");
        assert_eq!(loaded.comfy.workflow_dir, "custom-workflows");
    }

    #[test]
    fn corrupt_file_is_backed_up_before_defaults_are_used() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, b"not json").unwrap();
        let loaded = AppConfig::load_with_environment(&path, |_| None).unwrap();
        assert_eq!(loaded, AppConfig::default());
        assert!(!path.exists());
        assert!(dir.path().join("config.json.corrupt").exists());
    }

    #[test]
    fn rejects_invalid_values() {
        let config = AppConfig {
            display: DisplayConfig {
                preview_fps: 0,
                ..DisplayConfig::default()
            },
            ..AppConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn persists_avatar_lipsync_and_vad_settings() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        let mut config = AppConfig::default();
        config.avatar.crossfade_ms = 240;
        config.lipsync.smoothing_frames = 6;
        config.vad.max_seconds = 8.0;
        config.save(&path).unwrap();
        let loaded = AppConfig::load_with_environment(&path, |_| None).unwrap();
        assert_eq!(loaded.avatar.crossfade_ms, 240);
        assert_eq!(loaded.lipsync.smoothing_frames, 6);
        assert_eq!(loaded.vad.max_seconds, 8.0);
    }
}
