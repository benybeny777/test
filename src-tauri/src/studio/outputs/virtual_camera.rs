// virtual_camera.rs - 仮想カメラデバイスへフレームを流す出力。
//
// 配信ソフトの「映像キャプチャデバイス」から選べるようになるため、ウィンドウキャプチャの
// 設定を利用者に任せずに済む。代わりに**OSごとに仮想カメラの実装方式が違い**、
// Windows は DirectShow フィルタ、macOS は CoreMediaIO DAL プラグインの登録が要る。
//
// その方式を決めるまでは、`availability()` が理由付きで未対応を返す。**選べるのに何も
// 起きない状態を作らない**ためで、`start()` を黙って成功させることはしない。
// 着手条件は docs/TASKS.md に書いてある。

use async_trait::async_trait;

use crate::config::Config;
use crate::field::Field;
use crate::studio::{Availability, Output, OutputContext, OutputReg};

pub struct VirtualCamera;

#[async_trait]
impl Output for VirtualCamera {
    fn id(&self) -> &'static str {
        "virtual_camera"
    }

    fn label(&self) -> &'static str {
        "仮想カメラ"
    }

    fn availability(&self, _cfg: &Config) -> Availability {
        if cfg!(target_os = "linux") {
            return Availability::unavailable(
                "Linux では仮想カメラ出力に対応していません。透過ウィンドウをお使いください。",
            );
        }
        Availability::unavailable(
            "仮想カメラ出力はまだ実装されていません（OSごとの登録方式を決めてから対応します）。\
             いまは透過ウィンドウをお使いください。",
        )
    }

    fn config_schema(&self) -> Vec<Field> {
        vec![Field::number(
            "PICOVTUBER_OUTPUT_FPS",
            "出力フレームレート",
            "仮想カメラへ送る1秒あたりのフレーム数。",
            "30",
        )]
    }

    async fn start(&self, ctx: &OutputContext) -> anyhow::Result<()> {
        // 未対応を黙って成功にしない。成功にすると、配信ソフト側に何も出ないまま
        // 利用者は「配信中」と思い込む。
        if let Availability::Unavailable { reason } = self.availability(&ctx.cfg) {
            anyhow::bail!("{reason}");
        }
        Ok(())
    }

    async fn stop(&self, _ctx: &OutputContext) -> anyhow::Result<()> {
        // 開始できていないので、停止は常に成功でよい（二重停止でも失敗させない）。
        Ok(())
    }
}

inventory::submit! { OutputReg { make: || Box::new(VirtualCamera) } }

#[cfg(test)]
mod tests {
    use super::VirtualCamera;
    use crate::config::Config;
    use crate::studio::{Availability, Output, OutputContext};
    use std::sync::Arc;

    #[test]
    fn 未対応であることを理由付きで返す() {
        let Availability::Unavailable { reason } = VirtualCamera.availability(&Config::new()) else {
            panic!("実装前に「使える」と答えている");
        };
        assert!(!reason.trim().is_empty());
        assert!(reason.contains("透過ウィンドウ"), "代わりの手段を案内する: {reason}");
    }

    /// 未対応を成功にすると、何も映らないまま「配信中」と思い込ませる。
    #[tokio::test]
    async fn 開始は黙って成功しない() {
        let ctx = OutputContext {
            cfg: Arc::new(Config::new()),
        };
        let error = VirtualCamera
            .start(&ctx)
            .await
            .expect_err("未対応なのに成功している");
        assert!(!error.to_string().is_empty());
    }

    #[tokio::test]
    async fn 停止は冪等() {
        let ctx = OutputContext {
            cfg: Arc::new(Config::new()),
        };
        assert!(VirtualCamera.stop(&ctx).await.is_ok());
        assert!(VirtualCamera.stop(&ctx).await.is_ok());
    }
}
