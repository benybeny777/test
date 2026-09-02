// studio/local_ai.rs - ローカルAIによる表情の自動切替。
//
// 発話を同梱の音声認識でテキストにし、その内容から表情を選ぶ。**音声もテキストも
// このPCから出ない。** 未導入のときはクラウドサービスで代替せず、「利用不可」と理由を
// 添えて返す。代替してしまうと、利用者は自分の声が外へ出ていることに気づけない。
//
// 判定そのもの（テキスト → 表情）は `studio::expression` の語彙に寄せる。ここが持つのは
// 「使える状態か」「どれくらいの間隔で、どれだけの確信度から切り替えるか」だけ。

use std::path::PathBuf;
use std::time::Duration;

use picovtuber_core::expression::Expression;

use crate::config::Config;
use crate::studio::expression::{guess_from_text, TextGuess};

/// 機能が使えるか、使えないならその理由。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Availability {
    Ready,
    /// 使えない。画面へそのまま出す理由と、次にやること。
    Unavailable {
        reason: String,
        remedy: String,
    },
}

impl Availability {
    pub fn is_ready(&self) -> bool {
        matches!(self, Availability::Ready)
    }
}

/// 自動切替の調整値。
#[derive(Debug, Clone, Copy)]
pub struct AutoSettings {
    pub enabled: bool,
    /// 判定の間隔。短くすると顔がちらつき、長くすると話に追従しなくなる。
    pub interval: Duration,
    /// これを下回る確信度では切り替えない。
    pub min_confidence: f32,
}

impl AutoSettings {
    pub fn from_config(cfg: &Config) -> AutoSettings {
        AutoSettings {
            enabled: cfg.get_bool("PICOVTUBER_AUTO_EXPRESSION_ENABLED", false),
            interval: Duration::from_secs(u64::from(
                cfg.get_u32("PICOVTUBER_AUTO_EXPRESSION_INTERVAL_SEC", 5)
                    .clamp(1, 120),
            )),
            min_confidence: cfg
                .get_f32("PICOVTUBER_AUTO_EXPRESSION_MIN_CONFIDENCE", 0.34)
                .clamp(0.0, 1.0),
        }
    }
}

/// 音声認識モデルの場所。
fn asr_model_path(cfg: &Config) -> Option<PathBuf> {
    let configured = cfg.get("PICOVTUBER_ASR_MODEL_PATH", "");
    if configured.trim().is_empty() {
        return None;
    }
    Some(PathBuf::from(configured))
}

/// 話題・感情を判定する小型LLMの場所。
fn llm_model_path(cfg: &Config) -> Option<PathBuf> {
    let configured = cfg.get("PICOVTUBER_LLM_MODEL_PATH", "");
    if configured.trim().is_empty() {
        return None;
    }
    Some(PathBuf::from(configured))
}

/// 自動切替が使える状態か。
///
/// **片方でも欠けていれば「使えない」と返す。** 音声認識だけで動かすと、認識はできても
/// 話題の判定ができず、語彙に無い言い回しで表情が固まる。それを「動いている」と
/// 見せないため、両方が揃っていることを条件にする。
pub fn availability(cfg: &Config) -> Availability {
    let mut missing = Vec::new();
    match asr_model_path(cfg) {
        Some(path) if path.exists() => {}
        Some(path) => missing.push(format!("音声認識モデル（{}）", path.display())),
        None => missing.push("音声認識モデル（未設定）".to_string()),
    }
    match llm_model_path(cfg) {
        Some(path) if path.exists() => {}
        Some(path) => missing.push(format!("小型LLM（{}）", path.display())),
        None => missing.push("小型LLM（未設定）".to_string()),
    }
    if missing.is_empty() {
        return Availability::Ready;
    }
    Availability::Unavailable {
        reason: format!(
            "表情の自動切替に必要なものがありません: {}",
            missing.join(" / ")
        ),
        remedy:
            "`cargo xtask setup models`（製品版は設定画面の「モデルを取得」）で取得してください。\
                 クラウドの音声認識では代替しません（声をPCの外へ出さないため）"
                .to_string(),
    }
}

/// 認識したテキストから、切り替えるべき表情を決める。
///
/// `None` を返すのは「切り替えない」という意味。無理に当てない。
pub fn decide(settings: &AutoSettings, text: &str) -> Option<Expression> {
    if !settings.enabled {
        return None;
    }
    let TextGuess {
        expression,
        confidence,
    } = guess_from_text(text)?;
    (confidence >= settings.min_confidence).then_some(expression)
}

#[cfg(test)]
mod tests {
    use super::{availability, decide, AutoSettings, Availability};
    use crate::config::Config;
    use picovtuber_core::expression::Expression;
    use std::time::Duration;

    fn enabled() -> AutoSettings {
        AutoSettings {
            enabled: true,
            interval: Duration::from_secs(5),
            min_confidence: 0.34,
        }
    }

    /// 未導入をクラウドで代替すると、利用者は声が外へ出ていることに気づけない。
    #[test]
    fn 未導入は理由と対処を添えて利用不可を返す() {
        let cfg = Config::new();
        let result = availability(&cfg);
        let Availability::Unavailable { reason, remedy } = result else {
            panic!("未設定なのに利用可能になっている");
        };
        assert!(reason.contains("音声認識モデル"), "{reason}");
        assert!(reason.contains("小型LLM"), "{reason}");
        assert!(
            remedy.contains("クラウド"),
            "代替しないことを伝える: {remedy}"
        );
    }

    /// 片方だけでは「動いている」と見せない。
    #[test]
    fn 片方だけ揃っていても利用不可() {
        let dir = std::env::temp_dir().join("picovtuber-localai-test");
        std::fs::create_dir_all(&dir).unwrap();
        let asr = dir.join("asr.bin");
        std::fs::write(&asr, b"").unwrap();

        let cfg = Config::new();
        cfg.set("PICOVTUBER_ASR_MODEL_PATH", &asr.display().to_string());
        let result = availability(&cfg);
        assert!(!result.is_ready());
        let Availability::Unavailable { reason, .. } = result else {
            unreachable!()
        };
        assert!(reason.contains("小型LLM"), "{reason}");
        assert!(
            !reason.contains("音声認識モデル"),
            "揃っている側まで挙げている: {reason}"
        );

        // 両方そろえば利用可能。
        let llm = dir.join("llm.gguf");
        std::fs::write(&llm, b"").unwrap();
        cfg.set("PICOVTUBER_LLM_MODEL_PATH", &llm.display().to_string());
        assert!(availability(&cfg).is_ready());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn 無効なら何も切り替えない() {
        let settings = AutoSettings {
            enabled: false,
            ..enabled()
        };
        assert_eq!(decide(&settings, "やった、うれしい！"), None);
    }

    #[test]
    fn 確信度が閾値を超えたときだけ切り替える() {
        let settings = enabled();
        assert_eq!(
            decide(&settings, "やった、うれしい、最高"),
            Some(Expression::Joy)
        );

        let strict = AutoSettings {
            min_confidence: 0.9,
            ..enabled()
        };
        assert_eq!(
            decide(&strict, "たのしい"),
            None,
            "弱い根拠で切り替えている"
        );
        assert_eq!(
            decide(&strict, "たのしい、うれしい、最高"),
            Some(Expression::Joy)
        );
    }

    #[test]
    fn 手がかりの無い発話では切り替えない() {
        assert_eq!(decide(&enabled(), "今日は水曜日です"), None);
    }

    #[test]
    fn 設定の異常値でも判定が破綻しない() {
        let cfg = Config::new();
        cfg.set("PICOVTUBER_AUTO_EXPRESSION_ENABLED", "true");
        cfg.set("PICOVTUBER_AUTO_EXPRESSION_INTERVAL_SEC", "0");
        cfg.set("PICOVTUBER_AUTO_EXPRESSION_MIN_CONFIDENCE", "5");
        let settings = AutoSettings::from_config(&cfg);
        assert!(settings.enabled);
        assert!(
            settings.interval >= Duration::from_secs(1),
            "間隔0で回り続ける"
        );
        assert!(settings.min_confidence <= 1.0);
    }
}
