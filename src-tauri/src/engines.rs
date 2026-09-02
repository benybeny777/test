use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{config::AppConfig, pipeline::ExpressionDefinition};

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("ローカルAIエンジンがありません。cargo xtask setup engines を実行してください: {0}")]
    Missing(String),
    #[error("ローカルAIエンジンを起動できません: {0}")]
    Io(#[from] std::io::Error),
    #[error("ローカルAIエンジンが失敗しました: {0}")]
    Failed(String),
    #[error("入力が長すぎます")]
    InputTooLong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationResult {
    pub reply: String,
    pub expression_key: String,
}

#[derive(Debug, Deserialize)]
struct ModelAnswer {
    reply: String,
    expression_key: String,
}

#[derive(Clone)]
pub struct EngineContext {
    pub repository_root: PathBuf,
}

impl EngineContext {
    pub fn converse(
        &self,
        config: &AppConfig,
        persona: &str,
        input: &str,
        expressions: &[ExpressionDefinition],
    ) -> Result<ConversationResult, EngineError> {
        if input.chars().count() > 2_000 || persona.chars().count() > 4_000 {
            return Err(EngineError::InputTooLong);
        }
        let executable = self.repository_root.join("engines/llama/llama-cli.exe");
        let model = resolve_model(
            &self.repository_root,
            &config.ai.models_dir,
            "llm",
            &config.ai.llm_model,
        );
        require_file(&executable)?;
        require_file(&model)?;
        let keys = expressions
            .iter()
            .map(|value| value.key.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let prompt = format!(
            "あなたはVTuberです。人格: {persona}\n利用者: {input}\n表情候補: {keys}\n日本語で短く返答し、最後はJSONだけを出力してください。形式: {{\"reply\":\"返答\",\"expression_key\":\"候補の一つ\"}}"
        );
        let output = Command::new(executable)
            .args([
                "--model",
                model.to_string_lossy().as_ref(),
                "--prompt",
                &prompt,
                "--n-predict",
                "160",
                "--temp",
                "0.7",
                "--no-display-prompt",
                "--single-turn",
                "--simple-io",
            ])
            .stdin(Stdio::null())
            .output()?;
        if !output.status.success() {
            return Err(EngineError::Failed(
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            ));
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let answer =
            extract_json(&text).and_then(|value| serde_json::from_str::<ModelAnswer>(value).ok());
        // フォールバック許可: 小型LLMが固定JSONを破った場合も会話本文は利用者へ返す。
        let result = answer
            .filter(|value| {
                expressions
                    .iter()
                    .any(|item| item.key == value.expression_key)
            })
            .map(|value| ConversationResult {
                reply: value.reply,
                expression_key: value.expression_key,
            })
            .unwrap_or_else(|| ConversationResult {
                reply: text.trim().to_owned(),
                expression_key: expressions
                    .first()
                    .map(|value| value.key.clone())
                    .unwrap_or_else(|| "smile".into()),
            });
        Ok(result)
    }

    pub fn transcribe(&self, config: &AppConfig, wav: &Path) -> Result<String, EngineError> {
        let executable = self.repository_root.join("engines/whisper/whisper-cli.exe");
        let model = resolve_model(
            &self.repository_root,
            &config.ai.models_dir,
            "stt",
            &config.ai.stt_model,
        );
        require_file(&executable)?;
        require_file(&model)?;
        require_file(wav)?;
        let output = Command::new(executable)
            .args([
                "--model",
                model.to_string_lossy().as_ref(),
                "--file",
                wav.to_string_lossy().as_ref(),
                "--language",
                "ja",
                "--no-timestamps",
            ])
            .stdin(Stdio::null())
            .output()?;
        if !output.status.success() {
            return Err(EngineError::Failed(
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
}

fn resolve_model(root: &Path, models_dir: &str, kind: &str, name: &str) -> PathBuf {
    let base = PathBuf::from(models_dir);
    let base = if base.is_absolute() {
        base
    } else {
        root.join(base)
    };
    base.join(kind).join(name)
}

fn require_file(path: &Path) -> Result<(), EngineError> {
    if path.is_file() {
        Ok(())
    } else {
        Err(EngineError::Missing(path.to_string_lossy().into_owned()))
    }
}

fn extract_json(text: &str) -> Option<&str> {
    let begin = text.rfind('{')?;
    let end = text[begin..].find('}')? + begin + 1;
    Some(&text[begin..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_last_json_object() {
        assert_eq!(
            extract_json("説明\n{\"reply\":\"はい\",\"expression_key\":\"smile\"}"),
            Some("{\"reply\":\"はい\",\"expression_key\":\"smile\"}")
        );
    }
}
