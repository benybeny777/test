// mesh.rs - 多視点から素体メッシュとテクスチャを作る。
//
// 推論は PicoVTuber 管理下の Python ランタイムで行い、**外部の推論APIは呼ばない**。
//
// テクスチャは入力イラストの等倍以下に保つ（AGENTS.md の引き伸ばし禁止規則）。
// 大きくしても情報は増えず、後続の検証と利用者の両方に「高精細だ」と誤認させる。

use async_trait::async_trait;

use crate::field::Field;
use crate::permissions;
use crate::pipeline::runtime;
use crate::pipeline::{Asset, AssetKind, Stage, StageContext, StageOutput, StageReg};

pub struct Mesh;

#[async_trait]
impl Stage for Mesh {
    fn id(&self) -> &'static str {
        "mesh"
    }

    fn label(&self) -> &'static str {
        "メッシュ化"
    }

    fn requires(&self) -> &'static [&'static str] {
        &["views"]
    }

    fn produces(&self) -> &'static [&'static str] {
        // テクスチャは `pack` 工程が「入力イラストの等倍以下か」を確かめるため、
        // メッシュとは別の成果物として名前を付ける。
        &["mesh", "texture"]
    }

    fn uses_runtime(&self) -> bool {
        true
    }

    fn config_schema(&self) -> Vec<Field> {
        vec![
            Field::text(
                "PICOVTUBER_MESH_MODEL",
                "メッシュ化モデルのファイル名",
                "重みの置き場所にあるファイル名を指定します。取得は「モデルを取得」から行います。",
                "mesh.onnx",
            ),
            Field::number(
                "PICOVTUBER_MESH_TARGET_FACES",
                "目標ポリゴン数",
                "配信で軽く動かすための上限です。大きくすると重くなります。",
                "30000",
            ),
        ]
    }

    async fn run(&self, ctx: &StageContext) -> anyhow::Result<StageOutput> {
        let model = ctx.cfg.get("PICOVTUBER_MESH_MODEL", "mesh.onnx");
        runtime::ensure_available(&ctx.cfg, &[&model]).map_err(|error| anyhow::anyhow!("{error}"))?;

        let views_path = ctx.resolve_asset(ctx.input("views")?)?;
        let output_dir = ctx.output_dir(self.id())?;

        ctx.report(0.05, "メッシュを生成中");
        runtime::run_script(
            ctx,
            "mesh.py",
            &[
                "--views-dir".to_string(),
                views_path.to_string(),
                "--output-dir".to_string(),
                output_dir.to_string(),
                "--model".to_string(),
                model,
                "--target-faces".to_string(),
                ctx.cfg
                    .get_u32("PICOVTUBER_MESH_TARGET_FACES", 30000)
                    .to_string(),
            ],
        )
        .await?;

        let mesh_file = output_dir
            .join("mesh.glb")
            .map_err(|error| anyhow::anyhow!(error))?;
        let texture_file = output_dir
            .join("texture.png")
            .map_err(|error| anyhow::anyhow!(error))?;
        if !permissions::fs::exists(&mesh_file) || !permissions::fs::exists(&texture_file) {
            anyhow::bail!(
                "メッシュまたはテクスチャができていません。多視点ビューが3方向とも揃っているか確認してください。"
            );
        }

        ctx.report(1.0, "メッシュ化完了");
        Ok(StageOutput::new(vec![
            Asset::new(
                "mesh",
                format!("{}/mesh.glb", self.id()),
                AssetKind::Mesh,
                format!(
                    "上限{}ポリゴン",
                    ctx.cfg.get_u32("PICOVTUBER_MESH_TARGET_FACES", 30000)
                ),
            ),
            Asset::new(
                "texture",
                format!("{}/texture.png", self.id()),
                AssetKind::Image,
                "素体テクスチャ",
            ),
        ]))
    }
}

inventory::submit! { StageReg { make: || Box::new(Mesh) } }

#[cfg(test)]
mod tests {
    use super::Mesh;
    use crate::pipeline::Stage;

    #[test]
    fn 多視点の後ろに並ぶ() {
        assert_eq!(Mesh.requires(), &["views"]);
        assert_eq!(Mesh.produces(), &["mesh", "texture"]);
        assert!(Mesh.uses_runtime());
    }
}
