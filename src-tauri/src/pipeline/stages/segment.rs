// segment.rs - 前景と体パーツの分割マスクを作る。
//
// 後続の多視点生成とボーン配置は「どこが頭で、どこが腕か」を知らないと成立しない。
// ここで頭・髪・胴・腕・脚のマスクを作る。推論は PicoVTuber 管理下の Python
// ランタイムで行い、**外部の推論APIは呼ばない**。
//
// 重みが未取得なら、白紙のマスクで先へ進まずに失敗として返す。空マスクで通すと、
// 以降の工程が「体が無い」前提で最後まで走り、利用者は配信本番で気づくことになる。

use async_trait::async_trait;

use crate::field::Field;
use crate::permissions;
use crate::pipeline::runtime;
use crate::pipeline::{Asset, AssetKind, Stage, StageContext, StageOutput, StageReg};

/// 作るマスクの種類。ボーン配置と表情生成が名前で引く。
const PARTS: [&str; 5] = ["head", "hair", "body", "arms", "legs"];

pub struct Segment;

#[async_trait]
impl Stage for Segment {
    fn id(&self) -> &'static str {
        "segment"
    }

    fn label(&self) -> &'static str {
        "パーツ分割"
    }

    fn requires(&self) -> &'static [&'static str] {
        &["source"]
    }

    fn produces(&self) -> &'static [&'static str] {
        &["masks"]
    }

    fn uses_runtime(&self) -> bool {
        true
    }

    fn config_schema(&self) -> Vec<Field> {
        vec![Field::text(
            "PICOVTUBER_SEGMENT_MODEL",
            "分割モデルのファイル名",
            "重みの置き場所にあるファイル名を指定します。取得は「モデルを取得」から行います。",
            "segment.onnx",
        )]
    }

    async fn run(&self, ctx: &StageContext) -> anyhow::Result<StageOutput> {
        let model = ctx.cfg.get("PICOVTUBER_SEGMENT_MODEL", "segment.onnx");
        runtime::ensure_available(&ctx.cfg, &[&model]).map_err(|error| anyhow::anyhow!("{error}"))?;

        let source = ctx.input("source")?;
        let source_path = ctx.resolve_asset(source)?;
        let output_dir = ctx.output_dir(self.id())?;
        let masks_dir = output_dir
            .join("masks")
            .map_err(|error| anyhow::anyhow!(error))?;
        permissions::fs::create_dir_all(&masks_dir).map_err(|error| anyhow::anyhow!(error))?;

        ctx.report(0.05, "パーツ分割を開始");
        runtime::run_script(
            ctx,
            "segment.py",
            &[
                "--input".to_string(),
                source_path.to_string(),
                "--output-dir".to_string(),
                masks_dir.to_string(),
                "--model".to_string(),
                model,
                "--parts".to_string(),
                PARTS.join(","),
            ],
        )
        .await?;

        // スクリプトが「成功」と言っても、実際に全部できているかは自分で確かめる。
        let missing: Vec<&str> = PARTS
            .into_iter()
            .filter(|part| {
                masks_dir
                    .join(format!("{part}.png"))
                    .map(|path| !permissions::fs::exists(&path))
                    .unwrap_or(true)
            })
            .collect();
        if !missing.is_empty() {
            anyhow::bail!(
                "分割できなかったパーツがあります: {}。全身が入った、背景が透過または単色の絵で試してください。",
                missing.join(", ")
            );
        }

        ctx.report(1.0, "パーツ分割完了");
        Ok(StageOutput::new(vec![Asset::new(
            "masks",
            format!("{}/masks", self.id()),
            AssetKind::ImageSet,
            format!("{}パーツ", PARTS.len()),
        )]))
    }
}

inventory::submit! { StageReg { make: || Box::new(Segment) } }

#[cfg(test)]
mod tests {
    use super::{Segment, PARTS};
    use crate::config::Config;
    use crate::permissions::SafePath;
    use crate::pipeline::{Asset, AssetKind, Stage, StageContext};
    use std::collections::HashMap;
    use std::sync::Arc;

    #[test]
    fn パーツ名が一意で表情工程が引ける名前を含む() {
        let mut parts = PARTS.to_vec();
        let count = parts.len();
        parts.sort_unstable();
        parts.dedup();
        assert_eq!(parts.len(), count, "パーツ名が重複している");
        assert!(PARTS.contains(&"head"), "表情・口形生成が頭のマスクを使う");
    }

    #[test]
    fn 依存申告が下ごしらえの成果物と噛み合う() {
        assert_eq!(Segment.requires(), &["source"]);
        assert_eq!(Segment.produces(), &["masks"]);
        assert!(Segment.uses_runtime(), "GPU判定と未導入案内の対象にする");
    }

    /// 重みが無いのに白紙のマスクで先へ進むと、以降の工程が「体が無い」前提で走る。
    #[tokio::test]
    async fn 重み未取得は成功にせず理由付きで断る() {
        let dir = std::env::temp_dir().join("picovtuber-segment-test");
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = Config::new();
        cfg.set("PICOVTUBER_RUNTIME_PYTHON", "/存在しない/python");
        let mut inputs = HashMap::new();
        inputs.insert(
            "source".to_string(),
            Asset::new("source", "preprocess/source.png", AssetKind::Image, "1x1"),
        );
        let ctx = StageContext::new(SafePath::app_owned(dir.clone()), Arc::new(cfg))
            .with_inputs(inputs);

        let error = Segment.run(&ctx).await.unwrap_err().to_string();
        assert!(error.contains("Python ランタイムがありません"), "{error}");
        assert!(error.contains("setup runtime"), "{error}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
