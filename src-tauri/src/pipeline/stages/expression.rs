// expression.rs - 表情ブレンドシェイプ（喜／怒／驚／悲／楽）の変形指示を作る。
//
// ここが作るのは「顔のどの領域を、どちらへ、どれだけ動かすか」という指示だけで、
// 頂点の移動は `pack` 工程がランタイムへ委譲して行う。指示を Rust 側に置くことで、
// **5種類が本当に違う形になっているか**を書き出す前に検査できる。
//
// 名前だけ違って中身が同じ表情は、配信中に切り替えても顔が変わらない。この壊れ方は
// 利用者が本番で気づくので、ここで止める。

use async_trait::async_trait;

use picovtuber_core::expression::Expression;

use crate::field::Field;
use crate::permissions;
use crate::pipeline::{Asset, AssetKind, Stage, StageContext, StageOutput, StageReg};
use crate::vrm::{MorphControl, MorphSpec};

pub struct ExpressionStage;

#[async_trait]
impl Stage for ExpressionStage {
    fn id(&self) -> &'static str {
        "expression"
    }

    fn label(&self) -> &'static str {
        "表情生成"
    }

    fn requires(&self) -> &'static [&'static str] {
        &["rig", "masks"]
    }

    fn produces(&self) -> &'static [&'static str] {
        &["expressions"]
    }

    fn config_schema(&self) -> Vec<Field> {
        vec![Field::number(
            "PICOVTUBER_EXPRESSION_STRENGTH",
            "表情の強さ",
            "1.0で標準。大きくすると表情が誇張されます（0.2〜2.0）。",
            "1.0",
        )]
    }

    async fn run(&self, ctx: &StageContext) -> anyhow::Result<StageOutput> {
        // 依存申告どおりに揃っているかを先に確かめる（無ければここで止まる）。
        ctx.input("rig")?;
        ctx.input("masks")?;

        let strength = ctx
            .cfg
            .get_f32("PICOVTUBER_EXPRESSION_STRENGTH", 1.0)
            .clamp(0.2, 2.0);
        let output_dir = ctx.output_dir(self.id())?;

        let specs: Vec<MorphSpec> = Expression::ALL
            .into_iter()
            .map(|expression| spec_for(expression, strength))
            .collect();
        ensure_all_distinct(&specs)?;

        for (index, spec) in specs.iter().enumerate() {
            ctx.check_cancelled()?;
            spec.validate().map_err(|error| anyhow::anyhow!(error))?;
            let path = output_dir
                .join(format!("{}.json", spec.id))
                .map_err(|error| anyhow::anyhow!(error))?;
            let encoded = serde_json::to_vec_pretty(spec)?;
            permissions::fs::write(&path, &encoded).map_err(|error| anyhow::anyhow!(error))?;
            ctx.report(
                (index + 1) as f32 / specs.len() as f32,
                &format!("表情「{}」を作成", spec.id),
            );
        }

        Ok(StageOutput::new(vec![Asset::new(
            "expressions",
            self.id(),
            AssetKind::MorphSet,
            format!("{}種", specs.len()),
        )]))
    }
}

/// 表情ごとの変形指示。
///
/// 目・眉・口の3領域をどう動かすかで表情を分ける。値は顔の幅・高さに対する比なので、
/// 顔の大きさが違うモデルでも同じ見え方になる。
fn spec_for(expression: Expression, strength: f32) -> MorphSpec {
    let controls = match expression {
        // 喜: 目を細め、口角を上げる。
        Expression::Joy => vec![
            control("eye_l", [0.0, -0.25], 0.75),
            control("eye_r", [0.0, -0.25], 0.75),
            control("mouth", [0.0, -0.10], 1.20),
            control("eyebrow_l", [0.0, -0.05], 1.0),
            control("eyebrow_r", [0.0, -0.05], 1.0),
        ],
        // 怒: 眉を内側へ下げ、口を結ぶ。
        Expression::Angry => vec![
            control("eyebrow_l", [0.08, 0.18], 1.0),
            control("eyebrow_r", [-0.08, 0.18], 1.0),
            control("eye_l", [0.0, 0.10], 0.85),
            control("eye_r", [0.0, 0.10], 0.85),
            control("mouth", [0.0, 0.08], 0.80),
        ],
        // 驚: 目と口を大きく開き、眉を上げる。
        Expression::Surprised => vec![
            control("eye_l", [0.0, 0.0], 1.35),
            control("eye_r", [0.0, 0.0], 1.35),
            control("eyebrow_l", [0.0, -0.22], 1.0),
            control("eyebrow_r", [0.0, -0.22], 1.0),
            control("mouth", [0.0, 0.05], 1.45),
        ],
        // 悲: 眉尻を下げ、口角を下げる。
        Expression::Sorrow => vec![
            control("eyebrow_l", [-0.06, 0.14], 1.0),
            control("eyebrow_r", [0.06, 0.14], 1.0),
            control("eye_l", [0.0, 0.06], 0.90),
            control("eye_r", [0.0, 0.06], 0.90),
            control("mouth", [0.0, 0.12], 0.85),
        ],
        // 楽: 力を抜いた笑顔。喜より控えめで、目は開き気味。
        Expression::Fun => vec![
            control("eye_l", [0.0, -0.10], 0.92),
            control("eye_r", [0.0, -0.10], 0.92),
            control("mouth", [0.0, -0.05], 1.10),
            control("eyebrow_l", [0.0, -0.02], 1.0),
            control("eyebrow_r", [0.0, -0.02], 1.0),
        ],
    };
    MorphSpec {
        id: expression.id().to_string(),
        vrm_name: expression.vrm_name().to_string(),
        controls: controls
            .into_iter()
            .map(|control| scale_control(control, strength))
            .collect(),
    }
}

fn control(region: &str, offset: [f32; 2], scale: f32) -> MorphControl {
    MorphControl {
        region: region.to_string(),
        offset,
        scale,
    }
}

/// 表情の強さを反映する。拡大率は 1.0 からの差を伸ばし、範囲を外れないよう収める。
fn scale_control(control: MorphControl, strength: f32) -> MorphControl {
    MorphControl {
        region: control.region,
        offset: [control.offset[0] * strength, control.offset[1] * strength],
        scale: (1.0 + (control.scale - 1.0) * strength).clamp(0.1, 3.0),
    }
}

/// どの2つを取っても形が違うことを確かめる。
fn ensure_all_distinct(specs: &[MorphSpec]) -> anyhow::Result<()> {
    for (index, spec) in specs.iter().enumerate() {
        for other in specs.iter().skip(index + 1) {
            if !spec.differs_from(other) {
                anyhow::bail!(
                    "表現「{}」と「{}」が同じ形です。名前だけ違う表現は、切り替えても顔が変わりません。",
                    spec.id,
                    other.id
                );
            }
        }
    }
    Ok(())
}

inventory::submit! { StageReg { make: || Box::new(ExpressionStage) } }

#[cfg(test)]
mod tests {
    use super::{ensure_all_distinct, spec_for, ExpressionStage};
    use crate::pipeline::Stage;
    use picovtuber_core::expression::Expression;

    #[test]
    fn 五種類がすべて違う形になる() {
        let specs: Vec<_> = Expression::ALL
            .into_iter()
            .map(|expression| spec_for(expression, 1.0))
            .collect();
        assert_eq!(specs.len(), 5);
        assert!(ensure_all_distinct(&specs).is_ok());
    }

    #[test]
    fn 同じ形が混ざれば理由付きで断る() {
        let one = spec_for(Expression::Joy, 1.0);
        let mut copy = spec_for(Expression::Angry, 1.0);
        copy.controls = one.controls.clone();
        let error = ensure_all_distinct(&[one, copy]).unwrap_err().to_string();
        assert!(error.contains("同じ形"), "{error}");
    }

    #[test]
    fn 各表情がvrmの表現名へ対応する() {
        for expression in Expression::ALL {
            let spec = spec_for(expression, 1.0);
            assert_eq!(spec.id, expression.id());
            assert_eq!(spec.vrm_name, expression.vrm_name());
            assert!(spec.validate().is_ok(), "{:?}", spec.validate());
        }
    }

    /// 強さの設定が極端でも、拡大率が範囲を外れて検証に落ちないこと。
    #[test]
    fn 強さを上げ下げしても値域に収まる() {
        for strength in [0.2_f32, 1.0, 2.0] {
            for expression in Expression::ALL {
                let spec = spec_for(expression, strength);
                assert!(
                    spec.validate().is_ok(),
                    "強さ {strength} で範囲外: {:?}",
                    spec.validate()
                );
            }
        }
        // 強さ0.2でも「変化なし」にはならない（表情が消えると切り替えの意味がなくなる）。
        let weak = spec_for(Expression::Surprised, 0.2);
        assert!(weak.controls.iter().any(|control| control.scale != 1.0));
    }

    #[test]
    fn ボーンとマスクの両方に依存する() {
        assert_eq!(ExpressionStage.requires(), &["rig", "masks"]);
        assert_eq!(ExpressionStage.produces(), &["expressions"]);
        assert!(!ExpressionStage.uses_runtime(), "推論もPythonも要らない");
    }
}
