use std::{env, path::PathBuf};

use local_vtuber_studio::{
    config::AppConfig,
    pipeline::{NewCharacter, PipelineContext, STAGES},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let repository_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("リポジトリルートを取得できません")?
        .to_owned();
    let context = PipelineContext {
        characters_root: repository_root.join("temp/t7-characters"),
        repository_root,
    };
    if arguments.first().map(String::as_str) == Some("--background") {
        let id = arguments.get(1).ok_or("characterIdが必要です")?;
        let background_id = arguments.get(2).ok_or("backgroundIdが必要です")?;
        let prompt = arguments.get(3).ok_or("背景プロンプトが必要です")?;
        println!(
            "{}",
            context.generate_background(&AppConfig::default(), id, background_id, prompt)?
        );
        return Ok(());
    }
    if arguments.first().map(String::as_str) == Some("--add-expression") {
        let id = arguments.get(1).ok_or("characterIdが必要です")?;
        let label = arguments.get(2).ok_or("表示名が必要です")?.clone();
        let prompt = arguments.get(3).ok_or("プロンプトが必要です")?.clone();
        println!(
            "{}",
            serde_json::to_string(&context.add_expression(id, label, prompt)?)?
        );
        return Ok(());
    }
    let only = arguments.first().map(String::as_str) == Some("--only");
    let resume = arguments.first().map(String::as_str) == Some("--resume") || only;
    let (character, first_stage) = if resume {
        let id = arguments
            .get(1)
            .ok_or("--resumeにはcharacterIdが必要です")?;
        let stage = arguments.get(2).map(String::as_str).unwrap_or("mesh");
        (context.load_character(id)?, stage)
    } else {
        let source = arguments.first().ok_or("入力画像が必要です")?;
        let id = arguments.get(1).map(String::as_str).unwrap_or("female");
        let identity = arguments.get(2).cloned().unwrap_or_default();
        (
            context.create_character(NewCharacter {
                display_name: format!("女性キャラクター{id}"),
                source_path: source.clone(),
                persona_prompt: "明るく親しみやすい女性VTuber".into(),
                identity_tags: identity,
            })?,
            "mesh",
        )
    };
    // 実アプリと同じ優先順位で環境変数を反映する。検証用設定は temp 内に閉じる。
    let config = AppConfig::load(
        &context
            .repository_root
            .join("temp/pipeline-probe-config.json"),
    )?;
    let start = STAGES
        .iter()
        .position(|stage| *stage == first_stage)
        .ok_or("開始工程が不正です")?;
    let stages = if only {
        &STAGES[start..=start]
    } else {
        &STAGES[start..]
    };
    for stage in stages {
        let result = context.run_stage(None, &config, &character.character_id, stage)?;
        println!(
            "{}",
            serde_json::json!({
                "characterId": result.character_id,
                "stage": stage,
                "state": result.stages.get(*stage),
            })
        );
    }
    println!(
        "{}",
        serde_json::to_string(&context.load_character(&character.character_id)?)?
    );
    Ok(())
}
