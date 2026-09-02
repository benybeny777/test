// rig.rs - VRM humanoid の標準ボーンを置き、メッシュへスキニングする。
//
// ボーンの位置は機械学習ではなく、`segment` が作ったパーツマスクの外接矩形から決める。
// 頭のマスクの下端が首、脚のマスクの上端が腰、というように**絵から測れる**ので、
// 推論を挟むより安定し、GPU も要らない。
//
// スキニング（頂点をどのボーンにどれだけ従わせるか）だけは、メッシュを直接触るため
// PicoVTuber 管理下の Python ランタイムへ委譲する。**外部の推論APIは呼ばない。**

use async_trait::async_trait;
use image::GenericImageView;

use crate::field::Field;
use crate::permissions::{self, SafePath};
use crate::pipeline::runtime;
use crate::pipeline::{Asset, AssetKind, Stage, StageContext, StageOutput, StageReg};
use crate::vrm::{Bone, Rig};

pub struct RigStage;

#[async_trait]
impl Stage for RigStage {
    fn id(&self) -> &'static str {
        "rig"
    }

    fn label(&self) -> &'static str {
        "ボーン設定"
    }

    fn requires(&self) -> &'static [&'static str] {
        &["mesh", "masks"]
    }

    fn produces(&self) -> &'static [&'static str] {
        &["rig"]
    }

    fn uses_runtime(&self) -> bool {
        // スキニングの適用だけ Python を使う。推論はしない（GPU も不要）。
        true
    }

    fn config_schema(&self) -> Vec<Field> {
        vec![Field::number(
            "PICOVTUBER_RIG_HEIGHT_M",
            "モデルの身長（メートル）",
            "配信ソフト側の縮尺に合わせます。VRMの標準的な人型はおよそ1.6mです。",
            "1.6",
        )]
    }

    async fn run(&self, ctx: &StageContext) -> anyhow::Result<StageOutput> {
        runtime::ensure_available(&ctx.cfg, &[]).map_err(|error| anyhow::anyhow!("{error}"))?;

        ctx.report(0.1, "ボーン位置を測定中");
        let masks_dir = ctx.resolve_asset(ctx.input("masks")?)?;
        let height = {
            let value = ctx.cfg.get_f32("PICOVTUBER_RIG_HEIGHT_M", 1.6);
            if value <= 0.0 {
                anyhow::bail!("身長の設定が0以下です: {value}");
            }
            value
        };
        let rig = build_rig(&masks_dir, height)?;
        rig.validate().map_err(|error| anyhow::anyhow!(error))?;

        let output_dir = ctx.output_dir(self.id())?;
        let bones_file = output_dir
            .join("bones.json")
            .map_err(|error| anyhow::anyhow!(error))?;
        let encoded = serde_json::to_vec_pretty(&rig)?;
        permissions::fs::write(&bones_file, &encoded).map_err(|error| anyhow::anyhow!(error))?;

        ctx.check_cancelled()?;
        ctx.report(0.4, "メッシュへスキニング中");
        let mesh_path = ctx.resolve_asset(ctx.input("mesh")?)?;
        runtime::run_script(
            ctx,
            "rig.py",
            &[
                "--mesh".to_string(),
                mesh_path.to_string(),
                "--bones".to_string(),
                bones_file.to_string(),
                "--output-dir".to_string(),
                output_dir.to_string(),
            ],
        )
        .await?;

        let rigged = output_dir
            .join("rig.glb")
            .map_err(|error| anyhow::anyhow!(error))?;
        if !permissions::fs::exists(&rigged) {
            anyhow::bail!(
                "スキニング済みメッシュができていません。ボーンだけを書き出しても配信では動かないため、ここで止めます。"
            );
        }

        ctx.report(1.0, "ボーン設定完了");
        Ok(StageOutput::new(vec![Asset::new(
            "rig",
            format!("{}/rig.glb", self.id()),
            AssetKind::Rig,
            format!("{}ボーン / 身長{height}m", rig.bones.len()),
        )]))
    }
}

/// パーツマスクの外接矩形から、VRM humanoid のボーン位置を決める。
///
/// 座標はモデル空間（メートル、Y上、原点は足元、X右）。絵から測れる量だけを使い、
/// 推測で埋める部分は左右対称の仮定に限る。
fn build_rig(masks_dir: &SafePath, height_m: f32) -> anyhow::Result<Rig> {
    let head = mask_bounds(masks_dir, "head")?;
    let body = mask_bounds(masks_dir, "body")?;
    let arms = mask_bounds(masks_dir, "arms")?;
    let legs = mask_bounds(masks_dir, "legs")?;

    // 全パーツを合わせた範囲を「立ち姿の全体」とみなし、そこを身長へ写す。
    let top = head.top.min(body.top).min(arms.top).min(legs.top);
    let bottom = head.bottom.max(body.bottom).max(arms.bottom).max(legs.bottom);
    let span = (bottom - top).max(1.0);
    let to_model_y = |pixel_y: f32| (bottom - pixel_y) / span * height_m;

    let center_x = (body.left + body.right) / 2.0;
    let to_model_x = |pixel_x: f32| (pixel_x - center_x) / span * height_m;

    let hips_y = to_model_y(legs.top);
    let neck_y = to_model_y(head.bottom);
    let head_y = to_model_y((head.top + head.bottom) / 2.0);
    let shoulder_y = to_model_y(arms.top);

    // 腕は左右対称に置く。片腕しか描かれていない絵でも破綻させないため、
    // 幅ではなく「胴の中心からの距離」を使う。
    let arm_reach = ((arms.right - arms.left) / 2.0).max(1.0);
    let arm_x = arm_reach / span * height_m;
    let elbow_y = shoulder_y - (shoulder_y - hips_y) * 0.45;
    let hand_y = shoulder_y - (shoulder_y - hips_y) * 0.9;

    let leg_x = ((legs.right - legs.left) / 4.0).max(1.0) / span * height_m;
    let knee_y = hips_y * 0.5;
    let foot_y = 0.0_f32;

    let bones = vec![
        bone("hips", None, [0.0, hips_y, 0.0]),
        bone("spine", Some("hips"), [0.0, hips_y + (neck_y - hips_y) * 0.3, 0.0]),
        bone(
            "chest",
            Some("spine"),
            [0.0, hips_y + (neck_y - hips_y) * 0.65, 0.0],
        ),
        bone("neck", Some("chest"), [0.0, neck_y, 0.0]),
        bone("head", Some("neck"), [0.0, head_y, 0.0]),
        bone("leftUpperArm", Some("chest"), [arm_x, shoulder_y, 0.0]),
        bone("leftLowerArm", Some("leftUpperArm"), [arm_x, elbow_y, 0.0]),
        bone("leftHand", Some("leftLowerArm"), [arm_x, hand_y, 0.0]),
        bone("rightUpperArm", Some("chest"), [-arm_x, shoulder_y, 0.0]),
        bone("rightLowerArm", Some("rightUpperArm"), [-arm_x, elbow_y, 0.0]),
        bone("rightHand", Some("rightLowerArm"), [-arm_x, hand_y, 0.0]),
        bone("leftUpperLeg", Some("hips"), [leg_x, hips_y, 0.0]),
        bone("leftLowerLeg", Some("leftUpperLeg"), [leg_x, knee_y, 0.0]),
        bone("leftFoot", Some("leftLowerLeg"), [leg_x, foot_y, 0.0]),
        bone("rightUpperLeg", Some("hips"), [-leg_x, hips_y, 0.0]),
        bone("rightLowerLeg", Some("rightUpperLeg"), [-leg_x, knee_y, 0.0]),
        bone("rightFoot", Some("rightLowerLeg"), [-leg_x, foot_y, 0.0]),
    ];
    // 中心のX（絵の中心とモデル原点のずれ）は上の `to_model_x` で吸収済み。
    let _ = to_model_x(center_x);
    Ok(Rig { bones })
}

fn bone(name: &str, parent: Option<&str>, position: [f32; 3]) -> Bone {
    Bone {
        name: name.to_string(),
        parent: parent.map(str::to_string),
        position,
    }
}

/// マスク画像の不透明部分の外接矩形（画素座標、Y下向き）。
#[derive(Debug, Clone, Copy, PartialEq)]
struct MaskBounds {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

fn mask_bounds(masks_dir: &SafePath, part: &str) -> anyhow::Result<MaskBounds> {
    let path = masks_dir
        .join(format!("{part}.png"))
        .map_err(|error| anyhow::anyhow!(error))?;
    let bytes = permissions::fs::read(&path).map_err(|error| anyhow::anyhow!(error))?;
    let image = image::load_from_memory(&bytes)
        .map_err(|error| anyhow::anyhow!("マスク「{part}」を読めません: {error}"))?;
    let (mut left, mut top) = (u32::MAX, u32::MAX);
    let (mut right, mut bottom) = (0_u32, 0_u32);
    let mut found = false;
    for (x, y, pixel) in image.pixels() {
        // マスクはアルファでも輝度でも表せる。どちらでも「そこにある」と読めるようにする。
        let present = pixel.0[3] > 8 && (pixel.0[0] > 8 || pixel.0[1] > 8 || pixel.0[2] > 8);
        if !present {
            continue;
        }
        found = true;
        left = left.min(x);
        top = top.min(y);
        right = right.max(x);
        bottom = bottom.max(y);
    }
    if !found {
        anyhow::bail!(
            "パーツ「{part}」のマスクが空です。ボーンを置く位置を決められないため、ここで止めます。"
        );
    }
    Ok(MaskBounds {
        left: left as f32,
        top: top as f32,
        right: right as f32,
        bottom: bottom as f32,
    })
}

inventory::submit! { StageReg { make: || Box::new(RigStage) } }

#[cfg(test)]
mod tests {
    use super::{build_rig, RigStage};
    use crate::permissions::SafePath;
    use crate::pipeline::Stage;
    use crate::vrm::REQUIRED_BONES;

    fn temp_masks(label: &str) -> SafePath {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("picovtuber-rig-{label}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        SafePath::app_owned(dir)
    }

    /// `top..bottom`（画素座標、Y下向き）に白い帯を置いたマスクを作る。
    fn write_mask(dir: &SafePath, part: &str, left: u32, top: u32, right: u32, bottom: u32) {
        let mut image = image::RgbaImage::new(400, 1000);
        for y in top..=bottom {
            for x in left..=right {
                image.put_pixel(x, y, image::Rgba([255, 255, 255, 255]));
            }
        }
        let mut encoded = Vec::new();
        image
            .write_to(
                &mut std::io::Cursor::new(&mut encoded),
                image::ImageFormat::Png,
            )
            .unwrap();
        crate::permissions::fs::write(&dir.join(format!("{part}.png")).unwrap(), &encoded).unwrap();
    }

    /// 立ち姿を模したマスク一式（頭が上、脚が下）。
    fn standing(dir: &SafePath) {
        write_mask(dir, "head", 160, 0, 240, 180);
        write_mask(dir, "hair", 150, 0, 250, 200);
        write_mask(dir, "body", 150, 180, 250, 520);
        write_mask(dir, "arms", 100, 200, 300, 520);
        write_mask(dir, "legs", 160, 520, 240, 960);
    }

    #[test]
    fn 立ち姿のマスクから必須ボーンを全部置ける() {
        let dir = temp_masks("full");
        standing(&dir);
        let rig = build_rig(&dir, 1.6).unwrap();
        assert_eq!(rig.bones.len(), REQUIRED_BONES.len());
        assert!(rig.validate().is_ok(), "{:?}", rig.validate());
        std::fs::remove_dir_all(dir.as_path()).ok();
    }

    /// 上下が入れ替わったボーンは、配信ソフトで首が折れたモデルになる。
    #[test]
    fn ボーンの高さが体の順序どおりになる() {
        let dir = temp_masks("order");
        standing(&dir);
        let rig = build_rig(&dir, 1.6).unwrap();
        let y = |name: &str| {
            rig.bones
                .iter()
                .find(|bone| bone.name == name)
                .expect(name)
                .position[1]
        };
        assert!(y("head") > y("neck"), "頭が首より下にある");
        assert!(y("neck") > y("chest"));
        assert!(y("chest") > y("spine"));
        assert!(y("spine") > y("hips"));
        assert!(y("hips") > y("leftLowerLeg"));
        assert!(y("leftLowerLeg") > y("leftFoot"));
        assert_eq!(y("leftFoot"), 0.0, "足元が原点");
        std::fs::remove_dir_all(dir.as_path()).ok();
    }

    #[test]
    fn 腕と脚は左右対称に置く() {
        let dir = temp_masks("mirror");
        standing(&dir);
        let rig = build_rig(&dir, 1.6).unwrap();
        let x = |name: &str| {
            rig.bones
                .iter()
                .find(|bone| bone.name == name)
                .expect(name)
                .position[0]
        };
        assert!(x("leftHand") > 0.0);
        assert_eq!(x("leftHand"), -x("rightHand"));
        assert_eq!(x("leftFoot"), -x("rightFoot"));
        std::fs::remove_dir_all(dir.as_path()).ok();
    }

    #[test]
    fn 身長の指定が縮尺に効く() {
        let dir = temp_masks("scale");
        standing(&dir);
        let small = build_rig(&dir, 1.0).unwrap();
        let large = build_rig(&dir, 2.0).unwrap();
        let head_y = |rig: &crate::vrm::Rig| {
            rig.bones
                .iter()
                .find(|bone| bone.name == "head")
                .unwrap()
                .position[1]
        };
        assert!((head_y(&large) - head_y(&small) * 2.0).abs() < 1e-4);
        std::fs::remove_dir_all(dir.as_path()).ok();
    }

    /// 空のマスクで先へ進むと、原点に全ボーンが重なった潰れたモデルができる。
    #[test]
    fn 空のマスクは推測で埋めずに断る() {
        let dir = temp_masks("empty");
        standing(&dir);
        // legs だけ空にする。
        let empty = image::RgbaImage::new(400, 1000);
        let mut encoded = Vec::new();
        empty
            .write_to(
                &mut std::io::Cursor::new(&mut encoded),
                image::ImageFormat::Png,
            )
            .unwrap();
        crate::permissions::fs::write(&dir.join("legs.png").unwrap(), &encoded).unwrap();

        let error = build_rig(&dir, 1.6).unwrap_err().to_string();
        assert!(error.contains("legs"), "{error}");
        assert!(error.contains("空です"), "{error}");
        std::fs::remove_dir_all(dir.as_path()).ok();
    }

    #[test]
    fn メッシュとマスクの両方に依存する() {
        assert_eq!(RigStage.requires(), &["mesh", "masks"]);
        assert_eq!(RigStage.produces(), &["rig"]);
    }
}
