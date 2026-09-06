pub mod config;
pub mod engines;
pub mod facepatch;
pub mod lipsync;
pub mod pipeline;
pub mod recovery;
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
    startup_warnings: Vec<String>,
}

#[tauri::command]
fn startup_warnings(state: State<'_, StudioState>) -> Vec<String> {
    state.startup_warnings.clone()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewAssets {
    rig: Vec<u8>,
    parts: BTreeMap<String, Vec<u8>>,
}

const RIG2D_PARTS: &[&str] = &[
    "scene_torso",
    "scene_left_arm",
    "scene_right_arm",
    "scene_neck",
    "scene_face",
    "scene_hair",
    "scene_residual",
    "left_eye_backplate",
    "right_eye_backplate",
    "left_eye_iris",
    "right_eye_iris",
    "left_eye_remainder",
    "right_eye_remainder",
    "left_eye_base",
    "right_eye_base",
    "left_eyelid_upper",
    "right_eyelid_upper",
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
    let mut current = state
        .config
        .write()
        .map_err(|_| "設定ロックが壊れました".to_owned())?;
    config.save(&state.config_path).map_err(error_text)?;
    *current = config;
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
    let _execution = state
        .execution
        .try_lock()
        .map_err(|_| "生成中のため表情を変更できません。完了後に再実行してください".to_owned())?;
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
    let _execution = state
        .execution
        .try_lock()
        .map_err(|_| "生成中のため表情を変更できません。完了後に再実行してください".to_owned())?;
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
    let _execution = state
        .execution
        .try_lock()
        .map_err(|_| "生成中のため表情を取り込めません。完了後に再実行してください".to_owned())?;
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
    let _execution = state
        .execution
        .try_lock()
        .map_err(|_| "生成中のため構図を保存できません。完了後に再実行してください".to_owned())?;
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
    let _execution = state.execution.try_lock().map_err(|_| {
        "生成・公開中です。現在の表示は保持し、完了後に再読み込みしてください".to_owned()
    })?;
    let _ = (&expression_key, &mouth_key);
    let directory = state
        .pipeline
        .character_dir(&character_id)
        .map_err(error_text)?;
    read_consistent_preview(&directory, |path| read_preview_file(&directory, path))
}

fn read_preview_file(directory: &Path, path: &Path) -> std::io::Result<Vec<u8>> {
    let relative = path
        .strip_prefix(directory)
        .map_err(std::io::Error::other)?;
    let mut current = directory.to_path_buf();
    for component in std::iter::once(None).chain(relative.components().map(Some)) {
        if let Some(component) = component {
            if !matches!(component, std::path::Component::Normal(_)) {
                return Err(std::io::Error::other("プレビュー素材の格納先が不正です"));
            }
            current.push(component);
        }
        let metadata = fs::symlink_metadata(&current)?;
        let mut linked = metadata.file_type().is_symlink();
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            linked |= metadata.file_attributes()
                & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
                != 0;
        }
        if linked {
            return Err(std::io::Error::other(
                "プレビュー素材のリンク参照は禁止です",
            ));
        }
    }
    fs::read(path)
}

fn preview_part_names(bytes: &[u8]) -> Result<Vec<String>, String> {
    let rig: serde_json::Value = serde_json::from_slice(bytes).map_err(error_text)?;
    let layers = rig
        .get("layers")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "リグの素材一覧がありません".to_owned())?;
    for required in RIG2D_PARTS {
        if !layers.contains_key(*required) {
            return Err(format!("必須リグ素材がありません: {required}"));
        }
    }
    for (name, layer) in layers {
        let reserved = matches!(name.as_str(), "con" | "prn" | "aux" | "nul")
            || ["com", "lpt"].iter().any(|prefix| {
                name.strip_prefix(prefix).is_some_and(|suffix| {
                    suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9')
                })
            });
        if name.is_empty()
            || name.len() > 128
            || reserved
            || !name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        {
            return Err(format!("リグ素材の識別子が不正です: {name}"));
        }
        // URLは取得先として利用せず、正規の固定表記だけを受け付ける。
        let expected = format!("/assets/rig2d/parts/{name}.png");
        if layer.get("url").and_then(serde_json::Value::as_str) != Some(expected.as_str()) {
            return Err(format!("リグ素材URLが正規形式ではありません: {name}"));
        }
    }
    Ok(layers.keys().cloned().collect())
}

fn preview_generation(bytes: &[u8]) -> Result<(String, String), String> {
    let manifest: CharacterManifest = serde_json::from_slice(bytes).map_err(error_text)?;
    let name = if manifest.model.contains_key("rig2d_base") {
        "complete"
    } else {
        "rig2d"
    };
    let stage = manifest
        .stages
        .get(name)
        .filter(|stage| stage.status == "complete" && !stage.updated_at_iso.is_empty())
        .ok_or_else(|| {
            "リグ生成・局所補完が未完了です。完了後に再読み込みしてください".to_owned()
        })?;
    Ok((name.to_owned(), stage.updated_at_iso.clone()))
}

fn read_consistent_preview(
    directory: &Path,
    mut read: impl FnMut(&Path) -> std::io::Result<Vec<u8>>,
) -> Result<PreviewAssets, String> {
    // 正規probeは公開前に状態を失効する。別プロセスでも世代を前後照合する。
    // character.jsonを更新しない診断サイドカーの直接公開はこの保証に含まない。
    let manifest_path = directory.join("character.json");
    let before = preview_generation(&read(&manifest_path).map_err(error_text)?)?;
    let rig = read(&directory.join("rig2d/rig.json")).map_err(error_text)?;
    let names = preview_part_names(&rig)?;
    let mut parts = BTreeMap::new();
    for name in names {
        parts.insert(
            name.clone(),
            read(&directory.join("rig2d/parts").join(format!("{name}.png"))).map_err(error_text)?,
        );
    }
    let assets = PreviewAssets { rig, parts };
    let after = preview_generation(&read(&manifest_path).map_err(error_text)?)?;
    if before != after {
        return Err("読み込み中に生成世代が変わりました。完了後に再読み込みしてください".into());
    }
    Ok(assets)
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
            let startup_warnings = recovery::recover_final_rigs(&output);
            app.manage(StudioState {
                config: RwLock::new(config),
                config_path,
                pipeline: PipelineContext {
                    characters_root: output,
                    repository_root: repository_root.clone(),
                },
                engines: EngineContext { repository_root },
                execution: tokio::sync::Mutex::new(()),
                startup_warnings,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            startup_warnings,
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
mod preview_tests {
    use super::*;

    fn rig_bytes(extra: &[&str]) -> Vec<u8> {
        let layers: BTreeMap<_, _> = RIG2D_PARTS
            .iter()
            .chain(extra.iter())
            .map(|name| {
                (
                    *name,
                    serde_json::json!({"url":format!("/assets/rig2d/parts/{name}.png")}),
                )
            })
            .collect();
        serde_json::to_vec(&serde_json::json!({"layers":layers})).unwrap()
    }

    fn manifest(stage: &str, status: &str, timestamp: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "schemaVersion": 1, "characterId": "fixture", "displayName": "確認",
            "createdAtIso": "first", "updatedAtIso": timestamp,
            "personaPrompt": "", "identityTags": "", "expressions": [], "framings": {},
            "model": if stage == "complete" { serde_json::json!({"rig2d_base":"rig2d-base/rig.json"}) } else { serde_json::json!({}) },
            "stages": {stage: {"status":status,"message":"","updatedAtIso":timestamp}}
        })).unwrap()
    }

    #[test]
    fn preview_accepts_consistent_legacy_and_completed_generations() {
        for stage in ["rig2d", "complete"] {
            let mut state_reads = 0;
            let assets = read_consistent_preview(Path::new("fixture"), |path| {
                if path.file_name().unwrap() == "character.json" {
                    state_reads += 1;
                    Ok(manifest(stage, "complete", "generation-1"))
                } else if path.file_name().unwrap() == "rig.json" {
                    Ok(rig_bytes(&["scene_hidden_face", "neck", "collar"]))
                } else {
                    Ok(b"original material".to_vec())
                }
            })
            .unwrap();
            assert_eq!(state_reads, 2);
            assert_eq!(assets.parts.len(), RIG2D_PARTS.len() + 3);
            assert_eq!(assets.parts["scene_hidden_face"], b"original material");
            assert_eq!(
                assets.rig,
                rig_bytes(&["scene_hidden_face", "neck", "collar"])
            );
        }
    }

    #[test]
    fn preview_rejects_publication_during_material_reads() {
        for (stage, status, timestamp) in [
            ("complete", "running", "generation-2"),
            ("complete", "failed", "generation-2"),
            ("complete", "complete", "generation-2"),
            ("rig2d", "complete", "generation-1"),
        ] {
            let mut state_reads = 0;
            let result = read_consistent_preview(Path::new("fixture"), |path| {
                if path.file_name().unwrap() == "character.json" {
                    state_reads += 1;
                    Ok(if state_reads == 1 {
                        manifest("complete", "complete", "generation-1")
                    } else {
                        manifest(stage, status, timestamp)
                    })
                } else if path.file_name().unwrap() == "rig.json" {
                    Ok(rig_bytes(&[]))
                } else {
                    Ok(b"potentially mixed material".to_vec())
                }
            });
            assert!(result.is_err(), "{stage}/{status}/{timestamp}");
        }
    }

    #[test]
    fn preview_rejects_incomplete_state_before_opening_materials() {
        let result = read_consistent_preview(Path::new("fixture"), |path| {
            assert_eq!(path.file_name().unwrap(), "character.json");
            Ok(manifest("complete", "running", "generation-2"))
        });
        assert!(result.is_err());
    }

    #[test]
    fn preview_rejects_missing_parts_unsafe_names_and_foreign_urls() {
        for name in [
            "../secret",
            "folder/secret",
            "C:\\secret",
            "part:stream",
            "nul",
            "com1",
            "name.png",
            "",
        ] {
            assert!(preview_part_names(&rig_bytes(&[name])).is_err(), "{name}");
        }
        let mut rig: serde_json::Value = serde_json::from_slice(&rig_bytes(&[])).unwrap();
        rig["layers"]
            .as_object_mut()
            .unwrap()
            .remove("mouth_closed");
        assert!(preview_part_names(&serde_json::to_vec(&rig).unwrap()).is_err());
        let mut rig: serde_json::Value = serde_json::from_slice(&rig_bytes(&[])).unwrap();
        rig["layers"]["mouth_closed"]["url"] =
            serde_json::json!("https://example.invalid/image.png");
        assert!(preview_part_names(&serde_json::to_vec(&rig).unwrap()).is_err());
    }

    #[test]
    fn preview_file_reader_restricts_directory_and_missing_files() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../temp");
        fs::create_dir_all(&workspace).unwrap();
        let fixture = tempfile::tempdir_in(workspace).unwrap();
        let file = fixture.path().join("material.png");
        fs::write(&file, b"fixture").unwrap();
        assert_eq!(
            read_preview_file(fixture.path(), &file).unwrap(),
            b"fixture"
        );
        assert!(read_preview_file(fixture.path(), &fixture.path().join("../secret.png")).is_err());
        assert!(read_preview_file(fixture.path(), &fixture.path().join("missing.png")).is_err());
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "Windowsのリンク作成特権が必要。開発者モードまたは特権環境で明示実行する"]
    fn preview_file_reader_rejects_real_symlink() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../temp");
        fs::create_dir_all(&workspace).unwrap();
        let fixture = tempfile::tempdir_in(workspace).unwrap();
        let file = fixture.path().join("material.png");
        fs::write(&file, b"fixture").unwrap();
        let linked = fixture.path().join("linked.png");
        std::os::windows::fs::symlink_file(&file, &linked).unwrap();
        assert!(read_preview_file(fixture.path(), &linked).is_err());
    }
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
            "sidecar/completion",
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
                        assert!(text.contains(
                            "fetch(localAssetUrl(source), {redirect: \"error\", signal, cache})"
                        ));
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
