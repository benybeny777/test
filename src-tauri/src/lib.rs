// lib.rs - PicoVTuber のエントリと Builder。
//
// ここでやるのは「起動に必要な順序」だけ。設定を読む → 保存先を決める → 画面を出す。
// 機能そのものは各モジュールが持つ。

pub mod commands;
pub mod config;
pub mod field;
pub mod permissions;
pub mod pipeline;
pub mod secret;
pub mod store;
pub mod studio;
pub mod text;
pub mod tools;
pub mod vrm;

#[cfg(test)]
mod cloud_free;
#[cfg(test)]
mod doc_sync;
#[cfg(test)]
mod guard_scan;

use std::sync::Arc;

use crate::config::Config;
use crate::permissions::SafePath;

/// アプリを起動する。
pub fn run() {
    let cfg = Arc::new(Config::new());
    let state = commands::AppState::new(Arc::clone(&cfg));

    tauri::Builder::default()
        // 2つ目の起動は既存ウィンドウを前へ出して終わる。
        // 同時に2つ動くと、設定・ジョブ記録の書き戻しが互いを上書きし、マイクと
        // 配信出力も二重に掴む（AGENTS.md の「同時に1つしか動かさない」前提）。
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            use tauri::Manager;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .setup(|app| {
            use tauri::Manager;
            let state = app.state::<commands::AppState>();

            // 設定の読み込み。読めなくても起動は続ける（保存だけを止める）。
            match app.path().app_config_dir() {
                Ok(dir) => {
                    if let Ok(Some(warning)) = state.cfg.load(dir.join("config.json")) {
                        // ログではなく起動時の警告として残す。利用者が見る場所へは
                        // フロントが `pipeline:progress` と同じ通知経路で出す。
                        eprintln!("[picovtuber] {warning}");
                    }
                }
                Err(error) => {
                    let reason = state.cfg.start_read_only(format!(
                        "設定の保存先を取得できないため、設定を保存しない読み取り専用状態で起動します ({error})"
                    ));
                    eprintln!("[picovtuber] {reason}");
                }
            }

            // 生成ジョブの置き場所。取得できない場合は作業フォルダや一時領域へ
            // 降格せず、理由を出して生成機能だけを使えない状態にする。
            match app.path().app_data_dir() {
                Ok(dir) => state.set_jobs_root(SafePath::app_owned(dir.join("jobs"))),
                Err(error) => {
                    eprintln!(
                        "[picovtuber] 生成ジョブの置き場所を取得できません。生成機能は使えません ({error})"
                    );
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::settings_schema,
            commands::settings_save,
            commands::pipeline_stages,
            commands::pipeline_create_job,
            commands::pipeline_run,
            commands::pipeline_cancel,
            commands::pipeline_status,
            commands::studio_outputs,
            commands::studio_state,
            commands::studio_transition,
            commands::studio_auto_expression_availability,
            commands::studio_expressions,
        ])
        .run(tauri::generate_context!())
        .expect("PicoVTuber の起動に失敗しました");
}
