// viseme.rs - 口形6種（あ・い・う・え・お・ん）の変形指示を作る。
//
// 表情と同じく、ここが作るのは「口の領域をどう動かすか」という指示だけ。頂点の移動は
// `pack` 工程がランタイムへ委譲する。指示を Rust 側に置くことで、**6種類が本当に違う
// 形になっているか**を書き出す前に検査できる。
//
// リップシンクは口形を毎フレーム切り替える。名前だけ違って中身が同じだと、話しても
// 口が動いていないように見える。

use async_trait::async_trait;

use picovtuber_core::viseme::Viseme;

use crate::field::Field;
use crate::permissions;
use crate::pipeline::{Asset, AssetKind, Stage, StageContext, StageOutput, StageReg};
use crate::vrm::{MorphControl, MorphSpec};

pub struct VisemeStage;

#[async_trait]
impl Stage for VisemeStage {
    fn id(&self) -> &'static str {
        "viseme"
    }

    fn label(&self) -> &'static str {
        "口形生成"
    }

    fn requires(&self) -> &'static [&'static str] {
        &["rig", "masks"]
    }

    fn produces(&self) -> &'static [&'static str] {
        &["visemes"]
    }

    fn config_schema(&self) -> Vec<Field> {
        vec![Field::number(
            "PICOVTUBER_VISEME_STRENGTH",
            "口の開きの強さ",
            "1.0で標準。大きくすると口が大きく動きます（0.2〜2.0）。",
            "1.0",
        )]
    }

    async fn run(&self, ctx: &StageContext) -> anyhow::Result<StageOutput> {
        ctx.input("rig")?;
        ctx.input("masks")?;

        let strength = ctx
            .cfg
            .get_f32("PICOVTUBER_VISEME_STRENGTH", 1.0)
            .clamp(0.2, 2.0);
        let output_dir = ctx.output_dir(self.id())?;

        let specs: Vec<MorphSpec> = Viseme::ALL
            .into_iter()
            .map(|viseme| spec_for(viseme, strength))
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
                &format!("口形「{}」を作成", spec.label_for_progress()),
            );
        }

        Ok(StageOutput::new(vec![Asset::new(
            "visemes",
            self.id(),
            AssetKind::MorphSet,
            format!("{}形", specs.len()),
        )]))
    }
}

/// 進捗表示に使う短い名前。
trait ProgressLabel {
    fn label_for_progress(&self) -> String;
}

impl ProgressLabel for MorphSpec {
    fn label_for_progress(&self) -> String {
        Viseme::from_id(&self.id)
            .map(|viseme| viseme.label().to_string())
            .unwrap_or_else(|| self.id.clone())
    }
}

/// 口形ごとの変形指示。
///
/// 日本語の母音は「縦の開き」と「横の広がり」の組み合わせでほぼ分けられる。
/// `mouth_open` が縦、`mouth_wide` が横、`mouth_round` が突き出しに対応する。
fn spec_for(viseme: Viseme, strength: f32) -> MorphSpec {
    let controls = match viseme {
        // あ: 大きく縦に開く。
        Viseme::A => vec![
            control("mouth_open", [0.0, 0.30], 1.30),
            control("mouth_wide", [0.0, 0.0], 1.05),
        ],
        // い: 横に引く。縦はほとんど開かない。
        Viseme::I => vec![
            control("mouth_open", [0.0, 0.05], 1.02),
            control("mouth_wide", [0.0, 0.0], 1.35),
        ],
        // う: すぼめて突き出す。
        Viseme::U => vec![
            control("mouth_open", [0.0, 0.10], 1.05),
            control("mouth_wide", [0.0, 0.0], 0.70),
            control("mouth_round", [0.0, 0.0], 1.25),
        ],
        // え: 中くらいに開き、やや横へ。
        Viseme::E => vec![
            control("mouth_open", [0.0, 0.18], 1.15),
            control("mouth_wide", [0.0, 0.0], 1.18),
        ],
        // お: 丸く開く。
        Viseme::O => vec![
            control("mouth_open", [0.0, 0.22], 1.20),
            control("mouth_wide", [0.0, 0.0], 0.85),
            control("mouth_round", [0.0, 0.0], 1.15),
        ],
        // ん: 閉じる。**変化なしにしない**（閉じ形も1つの表現として必要）。
        Viseme::N => vec![
            control("mouth_open", [0.0, 0.0], 0.90),
            control("mouth_wide", [0.0, 0.0], 0.95),
        ],
    };
    MorphSpec {
        id: viseme.id().to_string(),
        vrm_name: viseme.vrm_name().to_string(),
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

fn scale_control(control: MorphControl, strength: f32) -> MorphControl {
    MorphControl {
        region: control.region,
        offset: [control.offset[0] * strength, control.offset[1] * strength],
        scale: (1.0 + (control.scale - 1.0) * strength).clamp(0.1, 3.0),
    }
}

fn ensure_all_distinct(specs: &[MorphSpec]) -> anyhow::Result<()> {
    for (index, spec) in specs.iter().enumerate() {
        for other in specs.iter().skip(index + 1) {
            if !spec.differs_from(other) {
                anyhow::bail!(
                    "口形「{}」と「{}」が同じ形です。話しても口が動いていないように見えます。",
                    spec.id,
                    other.id
                );
            }
        }
    }
    Ok(())
}

inventory::submit! { StageReg { make: || Box::new(VisemeStage) } }

#[cfg(test)]
mod tests {
    use super::{ensure_all_distinct, spec_for, VisemeStage};
    use crate::pipeline::Stage;
    use picovtuber_core::viseme::Viseme;

    fn scale_of(viseme: Viseme, region: &str) -> f32 {
        spec_for(viseme, 1.0)
            .controls
            .into_iter()
            .find(|control| control.region == region)
            .map(|control| control.scale)
            .unwrap_or(1.0)
    }

    #[test]
    fn 六形がすべて違う形になる() {
        let specs: Vec<_> = Viseme::ALL
            .into_iter()
            .map(|viseme| spec_for(viseme, 1.0))
            .collect();
        assert_eq!(specs.len(), 6);
        assert!(ensure_all_distinct(&specs).is_ok());
    }

    /// 母音の見え方が入れ替わっていないこと（「い」が縦に開いたら別の音に見える）。
    #[test]
    fn 母音の縦横が日本語の口形と合う() {
        assert!(
            scale_of(Viseme::A, "mouth_open") > scale_of(Viseme::I, "mouth_open"),
            "「あ」より「い」のほうが縦に開いている"
        );
        assert!(
            scale_of(Viseme::I, "mouth_wide") > scale_of(Viseme::U, "mouth_wide"),
            "「い」より「う」のほうが横に広い"
        );
        assert!(
            scale_of(Viseme::U, "mouth_wide") < 1.0,
            "「う」はすぼめる"
        );
        assert!(
            scale_of(Viseme::N, "mouth_open") < scale_of(Viseme::E, "mouth_open"),
            "「ん」が「え」より開いている"
        );
    }

    /// 閉じ形を「変化なし」にすると、他と同じ形と判定されて検証に落ちる。
    #[test]
    fn 閉じ形も形として区別できる() {
        let closed = spec_for(Viseme::N, 1.0);
        assert!(closed.validate().is_ok());
        for viseme in Viseme::ALL {
            if viseme == Viseme::N {
                continue;
            }
            assert!(closed.differs_from(&spec_for(viseme, 1.0)), "{viseme:?}");
        }
    }

    #[test]
    fn 強さを上げ下げしても値域に収まる() {
        for strength in [0.2_f32, 1.0, 2.0] {
            for viseme in Viseme::ALL {
                let spec = spec_for(viseme, strength);
                assert!(spec.validate().is_ok(), "強さ {strength}: {:?}", spec.validate());
            }
        }
    }

    #[test]
    fn ボーンとマスクの両方に依存する() {
        assert_eq!(VisemeStage.requires(), &["rig", "masks"]);
        assert_eq!(VisemeStage.produces(), &["visemes"]);
        assert!(!VisemeStage.uses_runtime());
    }
}
