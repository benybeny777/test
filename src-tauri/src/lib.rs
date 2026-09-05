pub mod config;
pub mod engines;
pub mod facepatch;
pub mod lipsync;
pub mod pipeline;
pub mod sidecar;
pub mod store;

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::RwLock,
};

use serde::Serialize;
use tauri::{Manager, State};

use crate::{
    config::AppConfig,
    engines::{ConversationResult, EngineContext},
    pipeline::{CharacterManifest, Framing, NewCharacter, PipelineContext, STAGES},
};

struct StudioState {
    config: RwLock<AppConfig>,
    config_path: PathBuf,
    pipeline: PipelineContext,
    engines: EngineContext,
    execution: tokio::sync::Mutex<()>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewAssets {
    rig: Vec<u8>,
    parts: BTreeMap<String, Vec<u8>>,
}

const RIG2D_PARTS: &[&str] = &[
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
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeneratedBackground {
    path: String,
    image: Vec<u8>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VoiceChatResult {
    transcript: String,
    reply: String,
    expression_key: String,
}

#[tauri::command]
fn get_config(state: State<'_, StudioState>) -> Result<AppConfig, String> {
    state
        .config
        .read()
        .map_err(|_| "設定ロックが壊れました".to_owned())
        .map(|value| value.clone())
}

#[tauri::command]
fn save_config(config: AppConfig, state: State<'_, StudioState>) -> Result<(), String> {
    config.save(&state.config_path).map_err(error_text)?;
    *state
        .config
        .write()
        .map_err(|_| "設定ロックが壊れました".to_owned())? = config;
    Ok(())
}

#[tauri::command]
fn create_character(
    input: NewCharacter,
    state: State<'_, StudioState>,
) -> Result<CharacterManifest, String> {
    state.pipeline.create_character(input).map_err(error_text)
}

#[tauri::command]
fn list_characters(state: State<'_, StudioState>) -> Result<Vec<CharacterManifest>, String> {
    state.pipeline.list_characters().map_err(error_text)
}

#[tauri::command]
async fn run_pipeline_stage(
    app: tauri::AppHandle,
    character_id: String,
    stage: String,
    state: State<'_, StudioState>,
) -> Result<CharacterManifest, String> {
    let _execution = state.execution.lock().await;
    let pipeline = state.pipeline.clone();
    let config = state
        .config
        .read()
        .map_err(|_| "設定ロックが壊れました".to_owned())?
        .clone();
    tauri::async_runtime::spawn_blocking(move || {
        pipeline.run_stage(Some(&app), &config, &character_id, &stage)
    })
    .await
    .map_err(error_text)?
    .map_err(error_text)
}

#[tauri::command]
async fn run_full_pipeline(
    app: tauri::AppHandle,
    character_id: String,
    state: State<'_, StudioState>,
) -> Result<CharacterManifest, String> {
    let _execution = state.execution.lock().await;
    let pipeline = state.pipeline.clone();
    let config = state
        .config
        .read()
        .map_err(|_| "設定ロックが壊れました".to_owned())?
        .clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut manifest = pipeline.load_character(&character_id)?;
        for stage in STAGES {
            manifest = pipeline.run_stage(Some(&app), &config, &character_id, stage)?;
        }
        Ok::<_, pipeline::PipelineError>(manifest)
    })
    .await
    .map_err(error_text)?
    .map_err(error_text)
}

#[tauri::command]
fn add_expression(
    character_id: String,
    label: String,
    prompt: String,
    state: State<'_, StudioState>,
) -> Result<CharacterManifest, String> {
    state
        .pipeline
        .add_expression(&character_id, label, prompt)
        .map_err(error_text)
}

#[tauri::command]
fn remove_expression(
    character_id: String,
    key: String,
    state: State<'_, StudioState>,
) -> Result<CharacterManifest, String> {
    state
        .pipeline
        .remove_expression(&character_id, &key)
        .map_err(error_text)
}

#[tauri::command]
async fn import_expression(
    character_id: String,
    input_path: String,
    kind: String,
    key: String,
    state: State<'_, StudioState>,
) -> Result<(), String> {
    let pipeline = state.pipeline.clone();
    let config = state
        .config
        .read()
        .map_err(|_| "設定ロックが壊れました".to_owned())?
        .clone();
    tauri::async_runtime::spawn_blocking(move || {
        pipeline.import_expression(&config, &character_id, Path::new(&input_path), &kind, &key)
    })
    .await
    .map_err(error_text)?
    .map_err(error_text)
}

#[tauri::command]
fn update_framing(
    character_id: String,
    framing: Framing,
    state: State<'_, StudioState>,
) -> Result<CharacterManifest, String> {
    state
        .pipeline
        .update_framing(&character_id, "green_screen", framing)
        .map_err(error_text)
}

#[tauri::command]
async fn generate_background(
    character_id: String,
    background_id: String,
    prompt: String,
    state: State<'_, StudioState>,
) -> Result<GeneratedBackground, String> {
    let _execution = state.execution.lock().await;
    let pipeline = state.pipeline.clone();
    let config = state
        .config
        .read()
        .map_err(|_| "設定ロックが壊れました".to_owned())?
        .clone();
    tauri::async_runtime::spawn_blocking(move || {
        let path = pipeline.generate_background(&config, &character_id, &background_id, &prompt)?;
        let image = fs::read(&path)?;
        Ok::<_, pipeline::PipelineError>(GeneratedBackground { path, image })
    })
    .await
    .map_err(error_text)?
    .map_err(error_text)
}

#[tauri::command]
fn load_preview_assets(
    character_id: String,
    expression_key: String,
    mouth_key: String,
    state: State<'_, StudioState>,
) -> Result<PreviewAssets, String> {
    let _ = (&expression_key, &mouth_key);
    let directory = state
        .pipeline
        .character_dir(&character_id)
        .map_err(error_text)?;
    let mut parts = BTreeMap::new();
    for name in RIG2D_PARTS {
        parts.insert(
            (*name).to_owned(),
            fs::read(directory.join("rig2d/parts").join(format!("{name}.png")))
                .map_err(error_text)?,
        );
    }
    Ok(PreviewAssets {
        rig: fs::read(directory.join("rig2d/rig.json")).map_err(error_text)?,
        parts,
    })
}

#[tauri::command]
async fn converse(
    character_id: String,
    input: String,
    state: State<'_, StudioState>,
) -> Result<ConversationResult, String> {
    let _execution = state.execution.lock().await;
    let manifest = state
        .pipeline
        .load_character(&character_id)
        .map_err(error_text)?;
    let config = state
        .config
        .read()
        .map_err(|_| "設定ロックが壊れました".to_owned())?
        .clone();
    let engines = state.engines.clone();
    tauri::async_runtime::spawn_blocking(move || {
        engines.converse(
            &config,
            &manifest.persona_prompt,
            &input,
            &manifest.expressions,
        )
    })
    .await
    .map_err(error_text)?
    .map_err(error_text)
}

#[tauri::command]
async fn transcribe(wav_path: String, state: State<'_, StudioState>) -> Result<String, String> {
    let _execution = state.execution.lock().await;
    let config = state
        .config
        .read()
        .map_err(|_| "設定ロックが壊れました".to_owned())?
        .clone();
    let engines = state.engines.clone();
    tauri::async_runtime::spawn_blocking(move || engines.transcribe(&config, Path::new(&wav_path)))
        .await
        .map_err(error_text)?
        .map_err(error_text)
}

#[tauri::command]
async fn voice_chat(
    character_id: String,
    wav_path: String,
    state: State<'_, StudioState>,
) -> Result<VoiceChatResult, String> {
    let _execution = state.execution.lock().await;
    let manifest = state
        .pipeline
        .load_character(&character_id)
        .map_err(error_text)?;
    let config = state
        .config
        .read()
        .map_err(|_| "設定ロックが壊れました".to_owned())?
        .clone();
    let engines = state.engines.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let transcript = engines.transcribe(&config, Path::new(&wav_path))?;
        let answer = engines.converse(
            &config,
            &manifest.persona_prompt,
            &transcript,
            &manifest.expressions,
        )?;
        Ok::<_, engines::EngineError>(VoiceChatResult {
            transcript,
            reply: answer.reply,
            expression_key: answer.expression_key,
        })
    })
    .await
    .map_err(error_text)?
    .map_err(error_text)
}

fn error_text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let application = tauri::Builder::default()
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            let config_path = config_dir.join("config.json");
            let config = AppConfig::load(&config_path)?;
            let repository_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .ok_or("リポジトリルートを取得できません")?
                .to_owned();
            let output = if config.pipeline.output_dir.is_empty() {
                data_dir.join("characters")
            } else {
                let path = PathBuf::from(&config.pipeline.output_dir);
                if path.is_absolute() {
                    path
                } else {
                    data_dir.join(path)
                }
            };
            eprintln!("キャラクターデータ: {}", output.display());
            app.manage(StudioState {
                config: RwLock::new(config),
                config_path,
                pipeline: PipelineContext {
                    characters_root: output,
                    repository_root: repository_root.clone(),
                },
                engines: EngineContext { repository_root },
                execution: tokio::sync::Mutex::new(()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            create_character,
            list_characters,
            run_pipeline_stage,
            run_full_pipeline,
            add_expression,
            remove_expression,
            import_expression,
            update_framing,
            generate_background,
            load_preview_assets,
            converse,
            transcribe,
            voice_chat,
        ])
        .build(tauri::generate_context!())
        .expect("LocalVTuberStudio の初期化に失敗しました");
    application.run(|_, _| {});
}

#[cfg(test)]
mod network_tests {
    use std::path::Path;

    #[test]
    fn generation_runtime_has_no_outbound_client() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let files = [
            "src-tauri/src",
            "sidecar/mesh",
            "sidecar/rigging",
            "sidecar/isolate",
            "sidecar/decompose",
            "sidecar/rig2d",
            "ui",
        ];
        let forbidden = [
            "reqwest",
            "ureq",
            "hyper::Client",
            "urllib.request",
            "XMLHttpRequest",
            "fetch(",
        ];
        for relative in files {
            inspect(root, &root.join(relative), &forbidden);
        }
        let expression = std::fs::read_to_string(root.join("sidecar/expression/generate.py"))
            .expect("表情サイドカーを読めません");
        assert!(expression.contains("http://127.0.0.1"));
        assert!(!expression.contains("https://"));
        let background = std::fs::read_to_string(root.join("sidecar/background/generate.py"))
            .expect("背景サイドカーを読めません");
        assert!(background.contains("http://127.0.0.1"));
        assert!(!background.contains("https://"));
    }

    #[test]
    fn webview_three_modules_resolve_to_vendored_files() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let files = [
            "ui/shared/avatar-renderer.js",
            "ui/shared/vendor/three/addons/loaders/GLTFLoader.js",
            "ui/shared/vendor/three/addons/utils/BufferGeometryUtils.js",
            "ui/shared/vendor/three/addons/utils/SkeletonUtils.js",
        ];
        for relative in files {
            let text = std::fs::read_to_string(root.join(relative)).unwrap();
            assert!(
                !text.contains("from 'three'") && !text.contains("from \"three\""),
                "WebViewで解決できない裸のthree参照があります: {relative}"
            );
        }
    }

    fn inspect(root: &Path, path: &Path, forbidden: &[&str]) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.components().any(|value| value.as_os_str() == "vendor") {
                continue;
            }
            if path.is_dir() {
                inspect(root, &path, forbidden);
            } else if path.file_name().and_then(|value| value.to_str()) == Some("lib.rs") {
                continue;
            } else if matches!(
                path.extension().and_then(|value| value.to_str()),
                Some("rs" | "py" | "js" | "html")
            ) {
                let text = std::fs::read_to_string(&path).unwrap();
                for token in forbidden {
                    // 同一オリジン検査とリダイレクト拒否を持つ資産読込だけを許可する。
                    if path == root.join("ui/shared/local-assets.js") && *token == "fetch(" {
                        assert!(text.contains("url.origin !== location.origin"));
                        assert!(
                            text.contains("fetch(localAssetUrl(source), {redirect: \"error\"})")
                        );
                        continue;
                    }
                    assert!(
                        !text.contains(token),
                        "生成経路に外向き通信候補があります: {}: {token}",
                        path.strip_prefix(root).unwrap().display()
                    );
                }
            }
        }
    }
}
