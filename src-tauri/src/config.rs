use std::{env, ffi::OsString, path::Path};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::store;

pub const SETTING_KEYS: &[&str] = &[
    "display.language",
    "display.preview_fps",
    "display.preview_scale",
];

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    pub display: DisplayConfig,
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
    display: Option<DisplayConfigFile>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct DisplayConfigFile {
    preview_fps: Option<u32>,
    preview_scale: Option<f32>,
    language: Option<String>,
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
        Ok(())
    }

    fn apply_file(&mut self, file: ConfigFile) {
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
    }

    fn validate(&self) -> Result<(), ConfigError> {
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
        Ok(())
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
        };
        saved.save(&path).unwrap();

        // The file is loaded after environment defaults, so it remains authoritative.
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
        };
        assert!(config.validate().is_err());
    }
}
