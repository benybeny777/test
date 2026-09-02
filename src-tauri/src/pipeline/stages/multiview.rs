// multiview.rs - 正面のイラストから側面・背面のビューを起こす。
//
// メッシュ化は正面1枚では厚みを決められない。ここで側面と背面を生成して3方向を揃える。
// 推論は PicoVTuber 管理下の Python ランタイムで行い、**外部の推論APIは呼ばない**。
//
// **失敗を正面の複製で埋めない。** 複製で通すと、できあがるモデルは横から見ると
// 板のように潰れており、利用者は配信本番で気づくことになる。

use async_trait::async_trait;

use crate::field::Field;
use crate::permissions;
use crate::pipeline::runtime;
use crate::pipeline::{Asset, AssetKind, Stage, StageContext, StageOutput, StageReg};

/// 揃えるビュー。`front` は下ごしらえ済みの正面をそのまま置く。
const VIEWS: [&str; 3] = ["front", "side", "back"];

pub struct Multiview;

#[async_trait]
impl Stage for Multiview {
    fn id(&self) -> &'static str {
        "multiview"
    }

    fn label(&self) -> &'static str {
        "多視点生成"
    }

    fn requires(&self) -> &'static [&'static str] {
        &["source", "masks"]
    }

    fn produces(&self) -> &'static [&'static str] {
        &["views"]
    }

    fn uses_runtime(&self) -> bool {
        true
    }

    fn config_schema(&self) -> Vec<Field> {
        vec![
            Field::text(
                "PICOVTUBER_MULTIVIEW_MODEL",
                "多視点生成モデルのファイル名",
                "重みの置き場所にあるファイル名を指定します。取得は「モデルを取得」から行います。",
                "multiview.onnx",
            ),
            Field::number(
                "PICOVTUBER_MULTIVIEW_STEPS",
                "生成ステップ数",
                "多いほど安定しますが時間が伸びます。GPUが無い環境では小さめにしてください。",
                "30",
            ),
        ]
    }

    async fn run(&self, ctx: &StageContext) -> anyhow::Result<StageOutput> {
        let model = ctx.cfg.get("PICOVTUBER_MULTIVIEW_MODEL", "multiview.onnx");
        runtime::ensure_available(&ctx.cfg, &[&model]).map_err(|error| anyhow::anyhow!("{error}"))?;

        let source_path = ctx.resolve_asset(ctx.input("source")?)?;
        let masks_path = ctx.resolve_asset(ctx.input("masks")?)?;
        let output_dir = ctx.output_dir(self.id())?;
        let views_dir = output_dir
            .join("views")
            .map_err(|error| anyhow::anyhow!(error))?;
        permissions::fs::create_dir_all(&views_dir).map_err(|error| anyhow::anyhow!(error))?;

        ctx.report(0.05, "側面・背面を生成中");
        runtime::run_script(
            ctx,
            "multiview.py",
            &[
                "--input".to_string(),
                source_path.to_string(),
                "--masks-dir".to_string(),
                masks_path.to_string(),
                "--output-dir".to_string(),
                views_dir.to_string(),
                "--model".to_string(),
                model,
                "--steps".to_string(),
                ctx.cfg.get_u32("PICOVTUBER_MULTIVIEW_STEPS", 30).to_string(),
                "--views".to_string(),
                VIEWS.join(","),
            ],
        )
        .await?;

        let missing: Vec<&str> = VIEWS
            .into_iter()
            .filter(|view| {
                views_dir
                    .join(format!("{view}.png"))
                    .map(|path| !permissions::fs::exists(&path))
                    .unwrap_or(true)
            })
            .collect();
        if !missing.is_empty() {
            anyhow::bail!(
                "生成できなかったビューがあります: {}。正面の複製では厚みを作れないため、ここで止めます。",
                missing.join(", ")
            );
        }

        ctx.report(1.0, "多視点生成完了");
        Ok(StageOutput::new(vec![Asset::new(
            "views",
            format!("{}/views", self.id()),
            AssetKind::ImageSet,
            VIEWS.join("/"),
        )]))
    }
}

inventory::submit! { StageReg { make: || Box::new(Multiview) } }

#[cfg(test)]
mod tests {
    use super::{Multiview, VIEWS};
    use crate::pipeline::Stage;

    #[test]
    fn 三方向を揃える() {
        assert_eq!(VIEWS.len(), 3);
        assert!(VIEWS.contains(&"side"), "厚みを決めるのに側面が要る");
        assert!(VIEWS.contains(&"back"), "背面テクスチャに要る");
    }

    #[test]
    fn 分割マスクと正面の両方に依存する() {
        assert_eq!(Multiview.requires(), &["source", "masks"]);
        assert_eq!(Multiview.produces(), &["views"]);
    }
}
