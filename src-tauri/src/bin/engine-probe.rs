use std::{env, path::PathBuf};

use local_vtuber_studio::{
    config::AppConfig, engines::EngineContext, pipeline::ExpressionDefinition,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let repository_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("リポジトリルートを取得できません")?
        .to_owned();
    let context = EngineContext { repository_root };
    let config = AppConfig::default();
    match arguments.first().map(String::as_str) {
        Some("chat") => {
            let input = arguments.get(1).ok_or("chatには入力文が必要です")?;
            let result = context.converse(
                &config,
                "明るく親しみやすい女性VTuber",
                input,
                &[
                    expression("smile", "笑顔"),
                    expression("sad", "悲しみ"),
                    expression("surprised", "驚き"),
                ],
            )?;
            println!("{}", serde_json::to_string(&result)?);
        }
        Some("stt") => {
            let path = arguments.get(1).ok_or("sttにはWAVパスが必要です")?;
            println!(
                "{}",
                context.transcribe(&config, PathBuf::from(path).as_path())?
            );
        }
        Some("voice") => {
            let path = arguments.get(1).ok_or("voiceにはWAVパスが必要です")?;
            let transcript = context.transcribe(&config, PathBuf::from(path).as_path())?;
            let answer = context.converse(
                &config,
                "明るく親しみやすい女性VTuber",
                &transcript,
                &[
                    expression("smile", "笑顔"),
                    expression("sad", "悲しみ"),
                    expression("surprised", "驚き"),
                ],
            )?;
            println!(
                "{}",
                serde_json::json!({"transcript": transcript, "answer": answer})
            );
        }
        _ => return Err("usage: engine-probe <chat TEXT|stt WAV|voice WAV>".into()),
    }
    Ok(())
}

fn expression(key: &str, label: &str) -> ExpressionDefinition {
    ExpressionDefinition {
        key: key.into(),
        label: label.into(),
        prompt: String::new(),
        is_built_in: true,
    }
}
