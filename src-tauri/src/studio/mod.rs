// studio/mod.rs - 配信モードの状態機械と、配信出力（Output）のレジストリ。
//
// 出力は `src/studio/outputs/<出力名>.rs` へ置けば `build.rs` が生成する
// `#[path] pub mod` と `inventory::submit!` で自動登録される。中央のレジストリは編集しない。
//
// 状態遷移をここ1箇所に持つ理由は、配信中の操作が「押した順」に依存しないようにするため。
// ボタン連打や、モデル切替と出力開始が同時に来ても壊れない形にしておく。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::field::Field;

pub mod expression;
pub mod lipsync;
pub mod local_ai;

// build.rs が `src/studio/outputs/*.rs` を走査して生成する `pub mod` 宣言。
pub mod outputs {
    include!(concat!(env!("OUT_DIR"), "/output_mods.rs"));
}

/// 配信モードの状態。
///
/// 前へ進むのは1段ずつ。飛ばして進めないことで、「モデルを読まずに配信開始」のような
/// 順序の穴を型で塞ぐ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StudioState {
    /// モデル未読込。
    Unloaded,
    /// モデルは読んだが、入力も出力も動いていない。
    Idle,
    /// マイク入力を受けて表情と口が動いている。まだ配信ソフトへは出していない。
    Tracking,
    /// 配信出力まで動いている。
    Streaming,
}

impl StudioState {
    /// 遷移の可否と、できない理由。
    ///
    /// **できない操作を黙って無視しない。** 無視すると、利用者は押したのに何も起きない
    /// 理由が分からないまま配信を始めてしまう。
    pub fn can_transition_to(self, next: StudioState) -> Result<(), String> {
        use StudioState::*;
        match (self, next) {
            // 同じ状態への遷移は成功（冪等。ボタン連打で失敗しない）。
            (current, target) if current == target => Ok(()),
            (Unloaded, Idle) => Ok(()),
            (Idle, Tracking) => Ok(()),
            (Tracking, Streaming) => Ok(()),
            // 戻る方向はどこからでも許す（止められない配信は事故になる）。
            (_, Idle) | (_, Unloaded) => Ok(()),
            (Unloaded, _) => Err("先にモデルを読み込んでください。".to_string()),
            (Idle, Streaming) => Err(
                "先にマイク入力を開始してください（口と表情が動かないまま配信になります）。"
                    .to_string(),
            ),
            (from, to) => Err(format!("{from:?} から {to:?} へは進めません。")),
        }
    }
}

/// 出力が使えるか、使えないならその理由。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Availability {
    Available,
    /// 使えない。設定画面で理由付きの無効表示にする。
    /// **選べるのに何も起きない状態を作らない。**
    Unavailable {
        reason: String,
    },
}

impl Availability {
    pub fn unavailable(reason: impl Into<String>) -> Availability {
        Availability::Unavailable {
            reason: reason.into(),
        }
    }

    pub fn is_available(&self) -> bool {
        matches!(self, Availability::Available)
    }
}

/// 出力の実行文脈。
pub struct OutputContext {
    pub cfg: std::sync::Arc<Config>,
}

/// 配信出力コネクタ。
#[async_trait]
pub trait Output: Send + Sync {
    fn id(&self) -> &'static str;
    fn label(&self) -> &'static str;
    /// この出力が現在のOS・環境で使えるか。
    fn availability(&self, cfg: &Config) -> Availability;
    fn config_schema(&self) -> Vec<Field> {
        Vec::new()
    }
    /// 開始。**冪等**にすること（すでに動いていれば成功で返る）。
    async fn start(&self, ctx: &OutputContext) -> anyhow::Result<()>;
    /// 停止。**冪等**にすること（動いていなければ成功で返る）。
    async fn stop(&self, ctx: &OutputContext) -> anyhow::Result<()>;
}

/// 自動登録の受け口。各出力ファイルの末尾で `inventory::submit!` する。
pub struct OutputReg {
    pub make: fn() -> Box<dyn Output>,
}

inventory::collect!(OutputReg);

/// 登録済みの全出力。
pub fn all_outputs() -> Vec<Box<dyn Output>> {
    inventory::iter::<OutputReg>
        .into_iter()
        .map(|reg| (reg.make)())
        .collect()
}

pub fn output_by_id(id: &str) -> Option<Box<dyn Output>> {
    all_outputs().into_iter().find(|output| output.id() == id)
}

/// 全出力の設定項目（設定画面が並べる）。
pub fn config_schema() -> Vec<Field> {
    let mut out = Vec::new();
    for output in all_outputs() {
        out.extend(output.config_schema());
    }
    out
}

/// 配信出力コネクタの設計規則を検査するガード。
#[cfg(test)]
mod guard {
    use crate::guard_scan;

    #[test]
    fn 登録数とファイル数が一致する() {
        let registered = super::all_outputs().len();
        let files = guard_scan::output_sources().len();
        assert_eq!(
            registered, files,
            "出力ファイルはあるのに inventory::submit! が漏れている（または逆）"
        );
    }

    #[test]
    fn 出力idはファイル名と一致する() {
        let mut offenders = Vec::new();
        for source in guard_scan::output_sources() {
            let file_stem = source.name.trim_end_matches(".rs").to_string();
            if super::output_by_id(&file_stem).is_none() {
                offenders.push(source.path.display().to_string());
            }
        }
        assert!(
            offenders.is_empty(),
            "出力IDはファイル名と一致させてください: {offenders:?}"
        );
    }

    /// 使えない環境で `availability()` が「使える」と答えると、押しても何も起きない
    /// 出力を利用者に選ばせることになる。
    #[test]
    fn すべての出力が可否を答えられる() {
        let cfg = crate::config::Config::new();
        for output in super::all_outputs() {
            let availability = output.availability(&cfg);
            if let super::Availability::Unavailable { reason } = &availability {
                assert!(
                    !reason.trim().is_empty(),
                    "出力「{}」が理由なしで利用不可を返している",
                    output.id()
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Availability, StudioState};

    #[test]
    fn 順を追って配信まで進める() {
        assert!(StudioState::Unloaded
            .can_transition_to(StudioState::Idle)
            .is_ok());
        assert!(StudioState::Idle
            .can_transition_to(StudioState::Tracking)
            .is_ok());
        assert!(StudioState::Tracking
            .can_transition_to(StudioState::Streaming)
            .is_ok());
    }

    /// ボタン連打で失敗しないこと（冪等）。
    #[test]
    fn 同じ状態への遷移は成功する() {
        for state in [
            StudioState::Unloaded,
            StudioState::Idle,
            StudioState::Tracking,
            StudioState::Streaming,
        ] {
            assert!(state.can_transition_to(state).is_ok(), "{state:?}");
        }
    }

    /// 口も表情も動かないまま配信が始まると、利用者は静止画を配信してしまう。
    #[test]
    fn 入力を飛ばして配信へ進めない() {
        let error = StudioState::Idle
            .can_transition_to(StudioState::Streaming)
            .expect_err("飛ばせてはいけない");
        assert!(error.contains("マイク入力"), "{error}");
    }

    #[test]
    fn モデル未読込では先へ進めない() {
        let error = StudioState::Unloaded
            .can_transition_to(StudioState::Tracking)
            .expect_err("読み込み前に進めてはいけない");
        assert!(error.contains("モデル"), "{error}");
    }

    /// 停止はどの状態からでもできるべき（止められない配信は事故になる）。
    #[test]
    fn 停止はいつでもできる() {
        for state in [
            StudioState::Idle,
            StudioState::Tracking,
            StudioState::Streaming,
        ] {
            assert!(
                state.can_transition_to(StudioState::Idle).is_ok(),
                "{state:?}"
            );
            assert!(
                state.can_transition_to(StudioState::Unloaded).is_ok(),
                "{state:?}"
            );
        }
    }

    #[test]
    fn 利用不可には必ず理由が付く() {
        let availability = Availability::unavailable("このOSでは未対応です");
        assert!(!availability.is_available());
        let Availability::Unavailable { reason } = availability else {
            unreachable!()
        };
        assert!(!reason.is_empty());
    }
}
