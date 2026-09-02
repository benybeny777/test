// commands.rs - フロント（webview）から呼べる操作と、共有状態。
//
// ここには「画面の操作を、どのモジュールの機能へ繋ぐか」だけを書く。判断そのもの
// （工程順、口形の推定、表情の選択、出力の可否）は各モジュールが持つ。ここへ判断を
// 書き始めると、設定画面と配信画面で同じ判断が二重に実装される。

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::{Map, Value};

use picovtuber_core::expression::Expression;

use crate::config::Config;
use crate::field::Field;
use crate::permissions::{self, SafePath};
use crate::pipeline::{self, job};
use crate::studio::{self, StudioState};

/// 進行中の生成ジョブ1件。
struct RunningJob {
    cancel: Arc<AtomicBool>,
}

/// アプリ全体の共有状態。
pub struct AppState {
    pub cfg: Arc<Config>,
    /// 生成ジョブの置き場所（`app_data_dir/jobs/`）。
    jobs_root: Mutex<Option<SafePath>>,
    running: Mutex<HashMap<String, RunningJob>>,
    studio: Mutex<StudioState>,
}

impl AppState {
    pub fn new(cfg: Arc<Config>) -> AppState {
        AppState {
            cfg,
            jobs_root: Mutex::new(None),
            running: Mutex::new(HashMap::new()),
            studio: Mutex::new(StudioState::Unloaded),
        }
    }

    /// 起動時に生成ジョブの置き場所を決める。
    pub fn set_jobs_root(&self, root: SafePath) {
        *self.jobs_root.lock().unwrap() = Some(root);
    }

    fn jobs_root(&self) -> Result<SafePath, String> {
        self.jobs_root
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "生成ジョブの置き場所が初期化されていません。".to_string())
    }

    fn job_dir(&self, job_id: &str) -> Result<SafePath, String> {
        if !is_valid_job_id(job_id) {
            return Err(format!("ジョブIDとして使えません: {job_id}"));
        }
        self.jobs_root()?.join(job_id)
    }
}

/// ジョブIDに使える形か。フォルダ名になるので、区切り文字と空を弾く。
fn is_valid_job_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// 設定画面へ返すスキーマ1件（`Field` をそのまま渡す）。
#[derive(Serialize)]
pub struct SettingsSchema {
    pub fields: Vec<Field>,
    /// 保存済みの生の値。秘密値は含めない。
    pub values: Map<String, Value>,
}

/// 全コネクタの設定項目と現在値。
#[tauri::command]
pub fn settings_schema(state: tauri::State<'_, AppState>) -> SettingsSchema {
    let mut fields = pipeline::config_schema();
    fields.extend(studio::config_schema());
    fields.extend(app_config_schema());

    // 秘密値は画面へ返さない（返すと webview 側のログや開発者ツールへ残る）。
    let mut values = state.cfg.get_all();
    values.retain(|key, _| !crate::secret::is_secret_key(key));

    SettingsSchema { fields, values }
}

/// コネクタに属さない、アプリ本体の設定項目。
fn app_config_schema() -> Vec<Field> {
    vec![
        Field::directory(
            "PICOVTUBER_ALLOWED_ROOT",
            "許可するフォルダ",
            "イラストの読み込みと成果物の書き出しを、このフォルダの中だけに限ります。",
            "",
        ),
        Field::text(
            "PICOVTUBER_ALLOWED_PATHS",
            "追加で許可するパス",
            "OS のパス区切り文字で複数指定できます。",
            "",
        ),
        Field::text(
            "PICOVTUBER_DENIED_PATHS",
            "拒否するパス",
            "許可フォルダの中でも、ここに挙げた場所は操作しません（許可より優先されます）。",
            "",
        ),
        Field::boolean(
            "PICOVTUBER_LIPSYNC_ENABLED",
            "リップシンクを使う",
            "マイク入力を解析して口を動かします。音声はこのPCから出ません。",
            "true",
        ),
        Field::text(
            "PICOVTUBER_LIPSYNC_INPUT_DEVICE",
            "マイク（入力デバイス）",
            "空ならOSの既定の入力を使います。",
            "",
        ),
        Field::text(
            "PICOVTUBER_MODEL_PATH",
            "配信に使うモデル",
            "生成したVRM、または手元のVRM 1.0ファイルのパス。",
            "",
        ),
    ]
}

/// 設定を保存する。保存後は再起動なしで反映される（各所が `cfg.get()` で都度読むため）。
#[tauri::command]
pub fn settings_save(
    state: tauri::State<'_, AppState>,
    patch: Map<String, Value>,
) -> Result<(), String> {
    // 秘密値はOSの資格情報保護へ預けてから保存する。
    let mut prepared = Map::new();
    for (key, value) in patch {
        if crate::secret::is_secret_key(&key) {
            let text = value.as_str().unwrap_or_default();
            if text.is_empty() {
                // 空にしたら実体も消す（参照だけ消しても鍵は残る）。
                crate::secret::forget(&key);
                prepared.insert(key, Value::String(String::new()));
                continue;
            }
            let stored = crate::secret::encrypt(&key, text).map_err(|error| error.to_string())?;
            prepared.insert(key, Value::String(stored));
        } else {
            prepared.insert(key, value);
        }
    }
    state
        .cfg
        .set_all(prepared)
        .map_err(|error| format!("設定を保存できません: {error:#}"))
}

/// 工程の一覧（生成ウィザードの進捗表示に使う）。
#[derive(Serialize)]
pub struct StageInfo {
    pub id: String,
    pub label: String,
    pub uses_runtime: bool,
}

#[tauri::command]
pub fn pipeline_stages() -> Result<Vec<StageInfo>, String> {
    let order = pipeline::plan().map_err(|error| format!("{error:#}"))?;
    Ok(order
        .into_iter()
        .filter_map(|id| {
            pipeline::stage_by_id(&id).map(|stage| StageInfo {
                id: stage.id().to_string(),
                label: stage.label().to_string(),
                uses_runtime: stage.uses_runtime(),
            })
        })
        .collect())
}

/// 生成ジョブを作る。**権利の確認に答えていなければ作らない。**
#[tauri::command]
pub fn pipeline_create_job(
    state: tauri::State<'_, AppState>,
    job_id: String,
    input_path: String,
    license_notice: String,
) -> Result<String, String> {
    if license_notice.trim().is_empty() {
        return Err(
            "取り込む画像の権利について確認に答えてください。自分が権利を持つ、または\
             権利者から許諾を得た画像だけを使えます。"
                .to_string(),
        );
    }
    let source = permissions::resolve(&state.cfg, &input_path)?;
    let dir = state.job_dir(&job_id)?;
    permissions::fs::create_dir_all(&dir)?;

    // 取り込んだ絵は作業ディレクトリへ写して、以後どの工程も上書きしない。
    let bytes = permissions::fs::read(&source)?;
    let target = dir.join("input.png")?;
    permissions::fs::write(&target, &bytes)?;

    let job = job::Job::new(&job_id, "input.png", license_notice.trim());
    job::save(&dir, &job).map_err(|error| format!("{error:#}"))?;
    Ok(job_id)
}

/// 生成の進捗（画面へそのまま出す）。
#[derive(Serialize, Clone)]
pub struct JobProgress {
    pub job_id: String,
    pub ratio: f32,
    pub message: String,
}

/// 生成を開始（または再開）する。
///
/// 完了済みの工程は飛ばす。**記録だけでなく成果物の実在を確かめて**飛ばすので、
/// 途中で落ちたジョブも壊れた状態から進まない。
#[tauri::command]
pub async fn pipeline_run(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    job_id: String,
) -> Result<(), String> {
    use tauri::Emitter;

    let dir = state.job_dir(&job_id)?;
    let mut job = job::load(&dir).map_err(|error| format!("{error:#}"))?;
    let cancel = Arc::new(AtomicBool::new(false));
    state.running.lock().unwrap().insert(
        job_id.clone(),
        RunningJob {
            cancel: Arc::clone(&cancel),
        },
    );

    let emitted_id = job_id.clone();
    let progress: pipeline::ProgressSink = Arc::new(move |ratio, message| {
        // 失敗も進捗も画面へ届ける。ログだけで済ませると、利用者は止まった理由が分からない。
        let _ = app.emit(
            "pipeline:progress",
            JobProgress {
                job_id: emitted_id.clone(),
                ratio,
                message: message.to_string(),
            },
        );
    });

    let result = pipeline::run_job(
        &dir,
        Arc::clone(&state.cfg),
        &mut job,
        progress,
        Arc::clone(&cancel),
    )
    .await;

    state.running.lock().unwrap().remove(&job_id);
    result.map_err(|error| format!("{error:#}"))
}

/// 生成を中止する。工程は区切りで中止を確認して途中で戻る。
#[tauri::command]
pub fn pipeline_cancel(state: tauri::State<'_, AppState>, job_id: String) -> Result<(), String> {
    let running = state.running.lock().unwrap();
    let Some(job) = running.get(&job_id) else {
        // 動いていないものを止めても失敗にしない（連打で失敗させない）。
        return Ok(());
    };
    job.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

/// ジョブの現在の状態。
#[derive(Serialize)]
pub struct JobStatus {
    pub job_id: String,
    pub completed: Vec<String>,
    pub remaining: Vec<String>,
    pub last_error: Option<String>,
}

#[tauri::command]
pub fn pipeline_status(
    state: tauri::State<'_, AppState>,
    job_id: String,
) -> Result<JobStatus, String> {
    let dir = state.job_dir(&job_id)?;
    let job = job::load(&dir).map_err(|error| format!("{error:#}"))?;
    let order = pipeline::plan().map_err(|error| format!("{error:#}"))?;
    let remaining = job::remaining_stages(&dir, &job, &order);
    let completed = order
        .into_iter()
        .filter(|id| !remaining.contains(id))
        .collect();
    Ok(JobStatus {
        job_id,
        completed,
        remaining,
        last_error: job.last_error,
    })
}

/// 配信出力の一覧と、使えるかどうか。
#[derive(Serialize)]
pub struct OutputInfo {
    pub id: String,
    pub label: String,
    pub availability: studio::Availability,
}

#[tauri::command]
pub fn studio_outputs(state: tauri::State<'_, AppState>) -> Vec<OutputInfo> {
    studio::all_outputs()
        .into_iter()
        .map(|output| OutputInfo {
            id: output.id().to_string(),
            label: output.label().to_string(),
            availability: output.availability(&state.cfg),
        })
        .collect()
}

/// 配信モードの状態。
#[tauri::command]
pub fn studio_state(state: tauri::State<'_, AppState>) -> StudioState {
    *state.studio.lock().unwrap()
}

/// 配信モードの状態を進める（または戻す）。
///
/// できない遷移は理由付きで断る。黙って無視すると、押したのに何も起きない理由が
/// 利用者に分からない。
#[tauri::command]
pub fn studio_transition(
    state: tauri::State<'_, AppState>,
    next: StudioState,
) -> Result<StudioState, String> {
    let mut current = state.studio.lock().unwrap();
    current.can_transition_to(next)?;
    *current = next;
    Ok(*current)
}

/// 表情の自動切替が使えるか。
#[tauri::command]
pub fn studio_auto_expression_availability(
    state: tauri::State<'_, AppState>,
) -> studio::local_ai::Availability {
    studio::local_ai::availability(&state.cfg)
}

/// 表情プリセットの一覧（画面のボタンを組み立てる）。
#[derive(Serialize)]
pub struct ExpressionInfo {
    pub id: String,
    pub label: String,
    pub vrm_name: String,
}

#[tauri::command]
pub fn studio_expressions() -> Vec<ExpressionInfo> {
    Expression::ALL
        .into_iter()
        .map(|expression| ExpressionInfo {
            id: expression.id().to_string(),
            label: expression.label().to_string(),
            vrm_name: expression.vrm_name().to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{app_config_schema, is_valid_job_id, AppState};
    use crate::config::Config;
    use crate::field::FieldKind;
    use crate::permissions::SafePath;
    use std::sync::Arc;

    #[test]
    fn ジョブidはフォルダ名として安全なものだけ通す() {
        assert!(is_valid_job_id("job-2026-01-01_01"));
        assert!(!is_valid_job_id(""));
        assert!(!is_valid_job_id("../escape"));
        assert!(!is_valid_job_id("a/b"));
        assert!(!is_valid_job_id("a\\b"));
        assert!(!is_valid_job_id(&"x".repeat(65)));
    }

    #[test]
    fn 置き場所が未設定なら理由付きで断る() {
        let state = AppState::new(Arc::new(Config::new()));
        let error = state.job_dir("job-1").expect_err("未初期化では通さない");
        assert!(error.contains("初期化されていません"), "{error}");
    }

    #[test]
    fn 置き場所を決めればジョブごとのフォルダを作れる() {
        let state = AppState::new(Arc::new(Config::new()));
        state.set_jobs_root(SafePath::app_owned(
            std::env::temp_dir().join("picovtuber-jobs"),
        ));
        let dir = state.job_dir("job-1").unwrap();
        assert!(dir.as_path().ends_with("job-1"));
        // 区切り文字入りは弾かれるので、ここから外へは出られない。
        assert!(state.job_dir("../outside").is_err());
    }

    /// 秘密値らしいキーは、画面へ返す前に落とす経路に乗っていること。
    #[test]
    fn アプリ設定に秘密値らしいキーを平文で置かない() {
        for field in app_config_schema() {
            if crate::secret::is_secret_key(field.key) {
                assert_eq!(
                    field.kind,
                    FieldKind::Password,
                    "{} が password 型で申告されていない",
                    field.key
                );
            }
        }
    }

    /// 既定値の書き間違いは「設定画面の表示」と「実装の既定」がずれる原因になる。
    #[test]
    fn アプリ設定のキーがすべて接頭辞を持つ() {
        for field in app_config_schema() {
            assert!(
                field.key.starts_with("PICOVTUBER_"),
                "{} に接頭辞がない",
                field.key
            );
            assert!(!field.label.is_empty());
            assert!(!field.help.is_empty(), "{} に説明がない", field.key);
        }
    }
}
