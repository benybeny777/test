pub mod config;
pub mod store;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let config = config::AppConfig::load(&config_dir.join("config.json"))?;
            app.manage(config);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("LocalVTuberStudio の起動に失敗しました");
}
