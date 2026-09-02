// pipeline/runtime.rs - 工程が使う Python ランタイムと生成モデル重みのライフサイクル。
//
// 分割・多視点生成・メッシュ化・VRM組み立ては、Rust で書き直すより既存の Python 実装を
// 呼ぶほうが確実で、モデルの差し替えにも追従できる。だが呼び方を各工程へ写すと、
// 未導入の扱い・タイムアウト・進捗の読み方が工程ごとにばらつく。ここへ集約する。
//
// **外部の推論APIは呼ばない。** ここが起動するのは PicoVTuber 管理下の Python だけで、
// 重みも利用者のPC内にあるものだけを使う。ネットワークは重みの取得にしか使わない。

use std::path::PathBuf;
use std::time::Duration;

use crate::config::Config;
use crate::pipeline::StageContext;
use crate::text;
use crate::tools::process;

/// ランタイムが使えない理由。利用者へそのまま見せる文言を持つ。
#[derive(Debug)]
pub struct Unavailable {
    pub reason: String,
    /// 利用者が次に何をすればよいか。
    pub remedy: String,
}

impl std::fmt::Display for Unavailable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}（{}）", self.reason, self.remedy)
    }
}

/// 実行デバイスの指定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    Auto,
    Cpu,
    Cuda,
}

impl Device {
    pub fn as_arg(self) -> &'static str {
        match self {
            Device::Auto => "auto",
            Device::Cpu => "cpu",
            Device::Cuda => "cuda",
        }
    }
}

/// 設定から実行デバイスを読む。未知の値は `auto` へ落とす。
pub fn device(cfg: &Config) -> Device {
    match cfg.get("PICOVTUBER_RUNTIME_DEVICE", "auto").as_str() {
        "cpu" => Device::Cpu,
        "cuda" => Device::Cuda,
        _ => Device::Auto,
    }
}

/// 生成モデルの重みを置く場所。
pub fn models_dir(cfg: &Config) -> Result<PathBuf, String> {
    let configured = cfg.get("PICOVTUBER_MODELS_DIR", "");
    if !configured.trim().is_empty() {
        return Ok(PathBuf::from(configured));
    }
    // 状態の保存先を作業フォルダや一時領域へ降格しない（次回起動で消える）。
    dirs::data_dir()
        .map(|dir| dir.join("PicoVTuber").join("models"))
        .ok_or_else(|| {
            "生成モデルの置き場所を決められません。設定の PICOVTUBER_MODELS_DIR を指定してください。"
                .to_string()
        })
}

/// Python 実行ファイルの場所。
fn python_path(cfg: &Config) -> Result<PathBuf, String> {
    let configured = cfg.get("PICOVTUBER_RUNTIME_PYTHON", "");
    if !configured.trim().is_empty() {
        return Ok(PathBuf::from(configured));
    }
    dirs::data_dir()
        .map(|dir| {
            let base = dir.join("PicoVTuber").join("python-runtime");
            if cfg!(windows) {
                base.join("python.exe")
            } else {
                base.join("bin").join("python3")
            }
        })
        .ok_or_else(|| {
            "Python ランタイムの場所を決められません。設定の PICOVTUBER_RUNTIME_PYTHON を指定してください。"
                .to_string()
        })
}

/// 工程スクリプトの置き場所。
fn scripts_dir(cfg: &Config) -> PathBuf {
    let configured = cfg.get("PICOVTUBER_SCRIPTS_DIR", "");
    if !configured.trim().is_empty() {
        return PathBuf::from(configured);
    }
    // 開発時はリポジトリ直下の scripts/。製品では同梱リソースを設定で指す。
    PathBuf::from("scripts")
}

/// ランタイムと必要な重みが揃っているか確かめる。
///
/// **揃っていない工程を「成功」にしない。** 白紙や複製で埋めて先へ進むと、利用者は
/// 配信本番になって初めて欠損に気づく。
pub fn ensure_available(cfg: &Config, required_weights: &[&str]) -> Result<(), Unavailable> {
    let python = python_path(cfg).map_err(|reason| Unavailable {
        reason,
        remedy: "設定画面で Python の場所を指定してください".to_string(),
    })?;
    if !python.exists() {
        return Err(Unavailable {
            reason: format!("Python ランタイムがありません: {}", python.display()),
            remedy: "`cargo xtask setup runtime`（製品版は設定画面の「ランタイムを取得」）を実行してください"
                .to_string(),
        });
    }

    let models = models_dir(cfg).map_err(|reason| Unavailable {
        reason,
        remedy: "設定画面で重みの置き場所を指定してください".to_string(),
    })?;
    let missing: Vec<&str> = required_weights
        .iter()
        .copied()
        .filter(|name| !models.join(name).exists())
        .collect();
    if !missing.is_empty() {
        return Err(Unavailable {
            reason: format!("生成モデルの重みがありません: {}", missing.join(", ")),
            remedy:
                "`cargo xtask setup models`（製品版は設定画面の「モデルを取得」）を実行してください"
                    .to_string(),
        });
    }
    Ok(())
}

/// 工程スクリプトを実行する。
///
/// 標準出力の `PROGRESS <0.0-1.0> <メッセージ>` 行を進捗として拾い、それ以外の行は
/// 診断用にまとめて返す。**利用者の画像そのもの・音声そのものはログへ落とさない**
/// （残すのは工程名・所要時間・寸法・失敗理由だけ。AGENTS.md の規則）。
pub async fn run_script(
    ctx: &StageContext,
    script: &str,
    args: &[String],
) -> anyhow::Result<String> {
    let python = python_path(&ctx.cfg).map_err(|error| anyhow::anyhow!(error))?;
    let script_path = scripts_dir(&ctx.cfg).join(script);
    if !script_path.exists() {
        anyhow::bail!(
            "工程スクリプトが見つかりません: {}（設定の PICOVTUBER_SCRIPTS_DIR を確認してください）",
            script_path.display()
        );
    }

    let mut command = process::command(&python.to_string_lossy());
    command.arg(&script_path);
    command.args(args);
    command.arg("--device");
    command.arg(device(&ctx.cfg).as_arg());

    let timeout = Duration::from_secs(u64::from(
        ctx.cfg.get_u32("PICOVTUBER_RUNTIME_TIMEOUT_SEC", 1800),
    ));
    ctx.check_cancelled()?;
    let output = process::output(command, timeout).await?;

    for line in output.stdout.lines() {
        if let Some((ratio, message)) = parse_progress(line) {
            ctx.report(ratio, message);
        }
    }
    if !output.success() {
        anyhow::bail!(
            "工程スクリプトが失敗しました（{script}, 終了コード {:?}）: {}",
            output.status,
            text::preview(&output.stderr, 400)
        );
    }
    Ok(output.stdout)
}

/// `PROGRESS <割合> <メッセージ>` を読む。それ以外の行は `None`。
fn parse_progress(line: &str) -> Option<(f32, &str)> {
    let rest = line.trim().strip_prefix("PROGRESS ")?;
    let (ratio, message) = rest.split_once(' ')?;
    let ratio = ratio.parse::<f32>().ok()?;
    if !ratio.is_finite() {
        return None;
    }
    Some((ratio, message))
}

#[cfg(test)]
mod tests {
    use super::{device, ensure_available, models_dir, parse_progress, Device};
    use crate::config::Config;

    #[test]
    fn 進捗行だけを拾う() {
        assert_eq!(
            parse_progress("PROGRESS 0.25 側面ビューを生成中"),
            Some((0.25, "側面ビューを生成中"))
        );
        assert_eq!(parse_progress("普通のログ行"), None);
        assert_eq!(parse_progress("PROGRESS みっつ めっせーじ"), None);
        // NaN を進捗として扱うと、比較が素通りして進捗バーが壊れる。
        assert_eq!(parse_progress("PROGRESS NaN 何か"), None);
    }

    #[test]
    fn 実行デバイスの未知の値はautoへ落とす() {
        let cfg = Config::new();
        assert_eq!(device(&cfg), Device::Auto);
        cfg.set("PICOVTUBER_RUNTIME_DEVICE", "cuda");
        assert_eq!(device(&cfg), Device::Cuda);
        cfg.set("PICOVTUBER_RUNTIME_DEVICE", "cpu");
        assert_eq!(device(&cfg), Device::Cpu);
        cfg.set("PICOVTUBER_RUNTIME_DEVICE", "とても速いやつ");
        assert_eq!(device(&cfg), Device::Auto);
    }

    #[test]
    fn 設定で重みの置き場所を上書きできる() {
        let cfg = Config::new();
        cfg.set("PICOVTUBER_MODELS_DIR", "/tmp/picovtuber-models");
        assert_eq!(
            models_dir(&cfg).unwrap(),
            std::path::PathBuf::from("/tmp/picovtuber-models")
        );
    }

    /// 未導入を「成功」にしないこと。理由と次にやることの両方を返す。
    #[test]
    fn ランタイム未導入は理由と対処を添えて断る() {
        let cfg = Config::new();
        cfg.set("PICOVTUBER_RUNTIME_PYTHON", "/存在しない/python");
        let error = ensure_available(&cfg, &[]).expect_err("未導入では通さない");
        assert!(error.reason.contains("Python ランタイムがありません"));
        assert!(error.remedy.contains("setup runtime"));
        assert!(error.to_string().contains("setup runtime"));
    }

    #[test]
    fn 重み未取得も理由と対処を添えて断る() {
        let dir = std::env::temp_dir().join("picovtuber-runtime-test");
        std::fs::create_dir_all(&dir).unwrap();
        // Python は「ある」ことにして、重みだけが無い状態を作る。
        let python = dir.join("python-stub");
        std::fs::write(&python, b"").unwrap();
        let cfg = Config::new();
        cfg.set("PICOVTUBER_RUNTIME_PYTHON", &python.display().to_string());
        cfg.set("PICOVTUBER_MODELS_DIR", &dir.display().to_string());

        let error = ensure_available(&cfg, &["segment.onnx"]).expect_err("重み未取得では通さない");
        assert!(error.reason.contains("segment.onnx"), "{}", error.reason);
        assert!(error.remedy.contains("setup models"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
