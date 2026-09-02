// vrm.rs - VRM 1.0 の組み立て条件と検証。
//
// 「書き出しは通ったが、配信ソフトで動かすと腕が動かない・口が閉じたまま」という壊れ方は、
// 利用者が本番で気づく。だから `pack` 工程は書き出す前にここで検証し、1つでも欠ければ
// **書き出さずに失敗を返す**。
//
// 欠けたまま書き出して「できました」と言わないことが、この層の唯一の役目。

use serde::{Deserialize, Serialize};

use picovtuber_core::expression::Expression;
use picovtuber_core::viseme::Viseme;

/// VRM humanoid の必須ボーン。これが揃っていないと配信ソフト側で人型として扱えない。
pub const REQUIRED_BONES: [&str; 17] = [
    "hips",
    "spine",
    "chest",
    "neck",
    "head",
    "leftUpperArm",
    "leftLowerArm",
    "leftHand",
    "rightUpperArm",
    "rightLowerArm",
    "rightHand",
    "leftUpperLeg",
    "leftLowerLeg",
    "leftFoot",
    "rightUpperLeg",
    "rightLowerLeg",
    "rightFoot",
];

/// ボーン1本。位置はモデル空間（メートル、Y上、原点は足元）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bone {
    pub name: String,
    pub parent: Option<String>,
    pub position: [f32; 3],
}

/// ボーン一式。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rig {
    pub bones: Vec<Bone>,
}

impl Rig {
    /// 必須ボーンが揃っていて、親子関係が閉じているか。
    pub fn validate(&self) -> Result<(), String> {
        let names: Vec<&str> = self.bones.iter().map(|bone| bone.name.as_str()).collect();

        let missing: Vec<&str> = REQUIRED_BONES
            .into_iter()
            .filter(|required| !names.contains(required))
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "VRM humanoid の必須ボーンが足りません: {}",
                missing.join(", ")
            ));
        }

        let mut sorted = names.clone();
        sorted.sort_unstable();
        let count = sorted.len();
        sorted.dedup();
        if sorted.len() != count {
            return Err("同じ名前のボーンが複数あります".to_string());
        }

        for bone in &self.bones {
            let Some(parent) = &bone.parent else {
                continue;
            };
            if !names.contains(&parent.as_str()) {
                return Err(format!(
                    "ボーン「{}」の親「{parent}」が見つかりません",
                    bone.name
                ));
            }
            if *parent == bone.name {
                return Err(format!("ボーン「{}」が自分自身を親にしています", bone.name));
            }
        }

        let roots = self
            .bones
            .iter()
            .filter(|bone| bone.parent.is_none())
            .count();
        if roots != 1 {
            return Err(format!(
                "根のボーンはちょうど1本にしてください（現在 {roots} 本）"
            ));
        }
        Ok(())
    }
}

/// VRM のメタデータ。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VrmMeta {
    pub title: String,
    pub author: String,
    /// 取り込み時に利用者が答えた利用許諾。**空のまま書き出させない。**
    pub license_notice: String,
}

/// 書き出し前の検証に必要な材料。
pub struct PackInput<'a> {
    pub rig: &'a Rig,
    /// 生成できた表情の識別子。
    pub expressions: &'a [String],
    /// 生成できた口形の識別子。
    pub visemes: &'a [String],
    /// テクスチャの寸法（幅, 高さ）。
    pub texture_size: (u32, u32),
    /// 入力イラストの寸法（幅, 高さ）。テクスチャがこれを超えていたら引き伸ばし。
    pub source_size: (u32, u32),
    pub meta: &'a VrmMeta,
}

/// 書き出してよいかを検査する。理由は利用者へそのまま見せられる文言で返す。
pub fn validate_pack(input: &PackInput<'_>) -> Result<(), String> {
    input.rig.validate()?;

    let missing_expressions: Vec<&str> = Expression::ALL
        .into_iter()
        .map(|expression| expression.id())
        .filter(|id| !input.expressions.iter().any(|made| made == id))
        .collect();
    if !missing_expressions.is_empty() {
        return Err(format!(
            "表情が足りません: {}。欠けたまま書き出すと、配信中に切り替えても顔が変わりません。",
            missing_expressions.join(", ")
        ));
    }

    let missing_visemes: Vec<&str> = Viseme::ALL
        .into_iter()
        .map(|viseme| viseme.id())
        .filter(|id| !input.visemes.iter().any(|made| made == id))
        .collect();
    if !missing_visemes.is_empty() {
        return Err(format!(
            "口形が足りません: {}。欠けたまま書き出すと、リップシンクで口が動きません。",
            missing_visemes.join(", ")
        ));
    }

    // 引き伸ばし拡大の禁止（AGENTS.md）。大きくしても情報は増えず、「高精細」と誤認させる。
    if input.texture_size.0 > input.source_size.0 || input.texture_size.1 > input.source_size.1 {
        return Err(format!(
            "テクスチャが入力イラストより大きくなっています（テクスチャ {}x{}、入力 {}x{}）。\
             引き伸ばしでは細部は増えないため、書き出しません。",
            input.texture_size.0, input.texture_size.1, input.source_size.0, input.source_size.1
        ));
    }
    if input.texture_size.0 == 0 || input.texture_size.1 == 0 {
        return Err("テクスチャの寸法が0です".to_string());
    }

    if input.meta.license_notice.trim().is_empty() {
        return Err(
            "利用許諾が空です。取り込み時の権利確認に答えた内容を入れてから書き出してください。"
                .to_string(),
        );
    }
    Ok(())
}

/// 顔のどこをどう動かすか、1箇所ぶんの指示。
///
/// 実際の頂点移動はランタイム側が行う。ここが持つのは「顔のどの領域を、どちらへ、
/// どれだけ」という指示だけで、メッシュの形には依存しない。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MorphControl {
    /// 動かす領域（`eyebrow_l` `eye_r` `mouth` など）。
    pub region: String,
    /// 移動量（顔の幅・高さに対する比）。
    pub offset: [f32; 2],
    /// 拡大縮小率。1.0 が変化なし。
    pub scale: f32,
}

/// 表情または口形1つぶんの変形指示。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MorphSpec {
    /// 成果物のファイル名になる識別子（`joy` `a` など）。
    pub id: String,
    /// VRM の標準表現名（`happy` `aa` など）。
    pub vrm_name: String,
    pub controls: Vec<MorphControl>,
}

impl MorphSpec {
    pub fn validate(&self) -> Result<(), String> {
        if self.controls.is_empty() {
            return Err(format!(
                "表現「{}」に動かす箇所がありません（名前だけあって形が変わりません）",
                self.id
            ));
        }
        for control in &self.controls {
            if !control.offset[0].is_finite() || !control.offset[1].is_finite() {
                return Err(format!("表現「{}」の移動量が数値ではありません", self.id));
            }
            if !(0.1..=3.0).contains(&control.scale) {
                return Err(format!(
                    "表現「{}」の拡大率が範囲外です: {}",
                    self.id, control.scale
                ));
            }
        }
        Ok(())
    }

    /// 別の表現と実際に違う形か。
    ///
    /// 名前だけ違って中身が同じだと、配信中に切り替えても顔が変わらない。生成工程は
    /// 書き出す前にこれを確かめる。
    pub fn differs_from(&self, other: &MorphSpec) -> bool {
        self.controls != other.controls
    }
}

/// 表情・口形の識別子から、VRM の標準表現名への対応表を作る。
///
/// 対応表をあちこちへ写すと、名前が1つずれただけで「切り替えても顔が変わらない」壊れ方に
/// なる。ここだけが対応の定義元。
pub fn vrm_expression_names() -> Vec<(&'static str, &'static str)> {
    let mut out: Vec<(&'static str, &'static str)> = Expression::ALL
        .into_iter()
        .map(|expression| (expression.id(), expression.vrm_name()))
        .collect();
    out.extend(
        Viseme::ALL
            .into_iter()
            .map(|viseme| (viseme.id(), viseme.vrm_name())),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::{validate_pack, Bone, PackInput, Rig, VrmMeta, REQUIRED_BONES};

    fn full_rig() -> Rig {
        let bones = REQUIRED_BONES
            .into_iter()
            .enumerate()
            .map(|(index, name)| Bone {
                name: name.to_string(),
                parent: (index > 0).then(|| "hips".to_string()),
                position: [0.0, index as f32 * 0.1, 0.0],
            })
            .collect();
        Rig { bones }
    }

    fn meta() -> VrmMeta {
        VrmMeta {
            title: "テストモデル".to_string(),
            author: "利用者".to_string(),
            license_notice: "自作イラストから生成".to_string(),
        }
    }

    fn all_expressions() -> Vec<String> {
        picovtuber_core::expression::Expression::ALL
            .into_iter()
            .map(|expression| expression.id().to_string())
            .collect()
    }

    fn all_visemes() -> Vec<String> {
        picovtuber_core::viseme::Viseme::ALL
            .into_iter()
            .map(|viseme| viseme.id().to_string())
            .collect()
    }

    #[test]
    fn 揃っていれば書き出してよい() {
        let rig = full_rig();
        let expressions = all_expressions();
        let visemes = all_visemes();
        let meta = meta();
        let input = PackInput {
            rig: &rig,
            expressions: &expressions,
            visemes: &visemes,
            texture_size: (2048, 2048),
            source_size: (2048, 4096),
            meta: &meta,
        };
        assert!(validate_pack(&input).is_ok());
    }

    #[test]
    fn 必須ボーンが欠けたら書き出さない() {
        let mut rig = full_rig();
        rig.bones.retain(|bone| bone.name != "leftHand");
        let error = rig.validate().unwrap_err();
        assert!(error.contains("leftHand"), "{error}");
    }

    #[test]
    fn 親が存在しないボーンを許さない() {
        let mut rig = full_rig();
        rig.bones.push(Bone {
            name: "extra".to_string(),
            parent: Some("いないボーン".to_string()),
            position: [0.0; 3],
        });
        let error = rig.validate().unwrap_err();
        assert!(error.contains("見つかりません"), "{error}");
    }

    #[test]
    fn 根のボーンは一本だけ() {
        let mut rig = full_rig();
        rig.bones.push(Bone {
            name: "another_root".to_string(),
            parent: None,
            position: [0.0; 3],
        });
        let error = rig.validate().unwrap_err();
        assert!(error.contains("根のボーン"), "{error}");
    }

    #[test]
    fn 表情や口形が欠けたら書き出さない() {
        let rig = full_rig();
        let meta = meta();
        let visemes = all_visemes();

        let mut expressions = all_expressions();
        expressions.pop();
        let input = PackInput {
            rig: &rig,
            expressions: &expressions,
            visemes: &visemes,
            texture_size: (1024, 1024),
            source_size: (1024, 2048),
            meta: &meta,
        };
        let error = validate_pack(&input).unwrap_err();
        assert!(error.contains("表情が足りません"), "{error}");

        let expressions = all_expressions();
        let mut short_visemes = all_visemes();
        short_visemes.retain(|id| id != "n");
        let input = PackInput {
            rig: &rig,
            expressions: &expressions,
            visemes: &short_visemes,
            texture_size: (1024, 1024),
            source_size: (1024, 2048),
            meta: &meta,
        };
        let error = validate_pack(&input).unwrap_err();
        assert!(error.contains("口形が足りません"), "{error}");
    }

    /// 引き伸ばしたテクスチャを「高精細」として通さない。
    #[test]
    fn テクスチャが入力より大きければ書き出さない() {
        let rig = full_rig();
        let meta = meta();
        let expressions = all_expressions();
        let visemes = all_visemes();
        let input = PackInput {
            rig: &rig,
            expressions: &expressions,
            visemes: &visemes,
            texture_size: (4096, 4096),
            source_size: (2048, 2048),
            meta: &meta,
        };
        let error = validate_pack(&input).unwrap_err();
        assert!(error.contains("引き伸ばし"), "{error}");
    }

    #[test]
    fn 利用許諾が空なら書き出さない() {
        let rig = full_rig();
        let expressions = all_expressions();
        let visemes = all_visemes();
        let meta = VrmMeta {
            license_notice: "   ".to_string(),
            ..meta()
        };
        let input = PackInput {
            rig: &rig,
            expressions: &expressions,
            visemes: &visemes,
            texture_size: (1024, 1024),
            source_size: (1024, 1024),
            meta: &meta,
        };
        let error = validate_pack(&input).unwrap_err();
        assert!(error.contains("利用許諾"), "{error}");
    }

    /// 対応表が1つずれると「切り替えても顔が変わらない」壊れ方になる。
    #[test]
    fn 表現名の対応表に重複がない() {
        let pairs = super::vrm_expression_names();
        assert_eq!(pairs.len(), 11, "表情5種と口形6種");
        let mut ids: Vec<&str> = pairs.iter().map(|(id, _)| *id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "識別子が重複している");
    }
}
