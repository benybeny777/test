// transparent_window.rs - 背景を透過した専用ウィンドウを出し、配信ソフトのウィンドウ
// キャプチャで取り込ませる出力。
//
// 仮想カメラより先にこれを用意する理由は、**OS固有のドライバを一切必要としない**から。
// Windows / macOS / Linux のどれでも、配信ソフト側の「ウィンドウキャプチャ」だけで成立する。
//
// 代わりに、配信ソフト側の設定を利用者が正しく選ぶ必要がある（キャプチャ方法と
// クライアント領域）。この前提は MANUAL.md に書き、ここでは黙って劣化させない。

use async_trait::async_trait;

use crate::config::Config;
use crate::field::Field;
use crate::studio::{Availability, Output, OutputContext, OutputReg};

pub struct TransparentWindow;

#[async_trait]
impl Output for TransparentWindow {
    fn id(&self) -> &'static str {
        "transparent_window"
    }

    fn label(&self) -> &'static str {
        "透過ウィンドウ（OBSのウィンドウキャプチャ用）"
    }

    fn availability(&self, _cfg: &Config) -> Availability {
        // 対応3OSすべてで、追加のドライバなしに成立する。
        Availability::Available
    }

    fn config_schema(&self) -> Vec<Field> {
        vec![
            Field::number(
                "PICOVTUBER_OUTPUT_WINDOW_WIDTH",
                "透過ウィンドウの幅",
                "配信ソフトで取り込むウィンドウの幅（ピクセル）。",
                "1280",
            ),
            Field::number(
                "PICOVTUBER_OUTPUT_WINDOW_HEIGHT",
                "透過ウィンドウの高さ",
                "配信ソフトで取り込むウィンドウの高さ（ピクセル）。",
                "720",
            ),
            Field::boolean(
                "PICOVTUBER_OUTPUT_WINDOW_ALWAYS_ON_TOP",
                "常に最前面に置く",
                "ウィンドウキャプチャの方式によっては、隠れると更新が止まります。",
                "false",
            ),
        ]
    }

    async fn start(&self, ctx: &OutputContext) -> anyhow::Result<()> {
        let size = window_size(&ctx.cfg);
        if size.0 == 0 || size.1 == 0 {
            anyhow::bail!("透過ウィンドウの寸法が0です（幅と高さを設定してください）");
        }
        // ウィンドウの生成そのものはフロント側（`studio_start_output` command）が行う。
        // ここでは寸法の妥当性だけを見て、壊れた値でウィンドウを作らせない。
        Ok(())
    }

    async fn stop(&self, _ctx: &OutputContext) -> anyhow::Result<()> {
        // 動いていなくても成功で返る（二重停止で失敗させない）。
        Ok(())
    }
}

/// 設定から寸法を読む。極端な値は配信ソフト側で扱えないため収める。
pub fn window_size(cfg: &Config) -> (u32, u32) {
    (
        cfg.get_u32("PICOVTUBER_OUTPUT_WINDOW_WIDTH", 1280)
            .clamp(160, 7680),
        cfg.get_u32("PICOVTUBER_OUTPUT_WINDOW_HEIGHT", 720)
            .clamp(160, 4320),
    )
}

inventory::submit! { OutputReg { make: || Box::new(TransparentWindow) } }

#[cfg(test)]
mod tests {
    use super::{window_size, TransparentWindow};
    use crate::config::Config;
    use crate::studio::{Output, OutputContext};
    use std::sync::Arc;

    #[test]
    fn どの環境でも使える() {
        assert!(TransparentWindow
            .availability(&Config::new())
            .is_available());
    }

    #[test]
    fn 寸法の設定は範囲へ収まる() {
        let cfg = Config::new();
        assert_eq!(window_size(&cfg), (1280, 720));
        cfg.set("PICOVTUBER_OUTPUT_WINDOW_WIDTH", "1920");
        cfg.set("PICOVTUBER_OUTPUT_WINDOW_HEIGHT", "1080");
        assert_eq!(window_size(&cfg), (1920, 1080));
        // 0 や巨大な値でウィンドウを作らせない。
        cfg.set("PICOVTUBER_OUTPUT_WINDOW_WIDTH", "0");
        cfg.set("PICOVTUBER_OUTPUT_WINDOW_HEIGHT", "99999");
        assert_eq!(window_size(&cfg), (160, 4320));
    }

    /// ボタン連打や、停止済みの再停止で失敗させない。
    #[tokio::test]
    async fn 開始と停止は冪等() {
        let ctx = OutputContext {
            cfg: Arc::new(Config::new()),
        };
        assert!(TransparentWindow.start(&ctx).await.is_ok());
        assert!(TransparentWindow.start(&ctx).await.is_ok());
        assert!(TransparentWindow.stop(&ctx).await.is_ok());
        assert!(TransparentWindow.stop(&ctx).await.is_ok());
    }
}
