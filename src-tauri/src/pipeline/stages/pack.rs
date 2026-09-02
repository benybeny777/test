// pack.rs - VRM 1.0 として検証して書き出す。
//
// ここは「工程が全部通った」と「配信で使える」の間にある最後の関門。ボーン、表情、
// 口形、テクスチャ、利用許諾のどれか1つでも欠けたまま書き出すと、利用者が本番で
// 気づくことになる。だから**書き出す前に検証し、欠ければ書き出さない**。
//
// 実際のVRM組み立て（glTF への morph target とボーンの埋め込み）は、メッシュを直接
// 触るため PicoVTuber 管理下の Python ランタイムへ委譲する。**外部の推論APIは呼ばない。**

use async_trait::async_trait;

use crate::field::Field;
use crate::permissions::{self, SafePath};
use crate::pipeline::runtime;
use crate::pipeline::{Asset, AssetKind, Stage, StageContext, StageOutput, StageReg};
use crate::vrm::{self, MorphSpec, PackInput, Rig, VrmMeta};

pub struct Pack;

#[async_trait]
impl Stage for Pack {
    fn id(&self) -> &'static str {
        "pack"
    }

    fn label(&self) -> &'static str {
        "書き出し"
    }

    fn requires(&self) -> &'static [&'static str] {
        &["rig", "expressions", "visemes", "source", "texture"]
    }

    fn produces(&self) -> &'static [&'static str] {
        &["vrm"]
    }

    fn uses_runtime(&self) -> bool {
        // VRM の組み立てだけ Python を使う。推論はしない（GPU も不要）。
        true
    }

    fn config_schema(&self) -> Vec<Field> {
        vec![
            Field::text(
                "PICOVTUBER_PACK_TITLE",
                "モデル名",
                "VRMのメタデータに入る名前です。空なら生成ジョブのIDを使います。",
                "",
            ),
            Field::text(
                "PICOVTUBER_PACK_AUTHOR",
                "作者名",
                "VRMのメタデータに入る作者名です。",
                "",
            ),
            Field::text(
                "PICOVTUBER_PACK_LICENSE_NOTICE",
                "利用許諾",
                "取り込み時の権利確認に答えた内容が入ります。空のままでは書き出せません。",
                "",
            ),
        ]
    }

    async fn run(&self, ctx: &StageContext) -> anyhow::Result<StageOutput> {
        runtime::ensure_available(&ctx.cfg, &[]).map_err(|error| anyhow::anyhow!("{error}"))?;

        ctx.report(0.1, "書き出せる状態か確認中");
        let rig_asset = ctx.input("rig")?;
        let rig_dir = ctx
            .resolve_asset(rig_asset)?
            .sibling("bones.json")
            .map_err(|error| anyhow::anyhow!(error))?;
        let rig = read_rig(&rig_dir)?;

        let expressions = read_morph_ids(&ctx.resolve_asset(ctx.input("expressions")?)?)?;
        let visemes = read_morph_ids(&ctx.resolve_asset(ctx.input("visemes")?)?)?;
        let texture_size = image_size(&ctx.resolve_asset(ctx.input("texture")?)?)?;
        let source_size = image_size(&ctx.resolve_asset(ctx.input("source")?)?)?;

        let meta = VrmMeta {
            title: {
                let configured = ctx.cfg.get("PICOVTUBER_PACK_TITLE", "");
                if configured.trim().is_empty() {
                    "PicoVTuber モデル".to_string()
                } else {
                    configured
                }
            },
            author: ctx.cfg.get("PICOVTUBER_PACK_AUTHOR", ""),
            license_notice: ctx.cfg.get("PICOVTUBER_PACK_LICENSE_NOTICE", ""),
        };

        vrm::validate_pack(&PackInput {
            rig: &rig,
            expressions: &expressions,
            visemes: &visemes,
            texture_size,
            source_size,
            meta: &meta,
        })
        .map_err(|error| anyhow::anyhow!(error))?;

        ctx.check_cancelled()?;
        ctx.report(0.4, "VRMを組み立て中");
        let output_dir = ctx.output_dir(self.id())?;
        let meta_file = output_dir
            .join("meta.json")
            .map_err(|error| anyhow::anyhow!(error))?;
        permissions::fs::write(&meta_file, &serde_json::to_vec_pretty(&meta)?)
            .map_err(|error| anyhow::anyhow!(error))?;

        runtime::run_script(
            ctx,
            "pack.py",
            &[
                "--rig".to_string(),
                ctx.resolve_asset(rig_asset)?.to_string(),
                "--expressions-dir".to_string(),
                ctx.resolve_asset(ctx.input("expressions")?)?.to_string(),
                "--visemes-dir".to_string(),
                ctx.resolve_asset(ctx.input("visemes")?)?.to_string(),
                "--texture".to_string(),
                ctx.resolve_asset(ctx.input("texture")?)?.to_string(),
                "--meta".to_string(),
                meta_file.to_string(),
                "--output".to_string(),
                output_dir
                    .join("model.vrm")
                    .map_err(|error| anyhow::anyhow!(error))?
                    .to_string(),
            ],
        )
        .await?;

        let model = output_dir
            .join("model.vrm")
            .map_err(|error| anyhow::anyhow!(error))?;
        if !permissions::fs::exists(&model) {
            anyhow::bail!("VRMを書き出せませんでした（組み立ての出力がありません）");
        }

        ctx.report(1.0, "書き出し完了");
        Ok(StageOutput::new(vec![Asset::new(
            "vrm",
            format!("{}/model.vrm", self.id()),
            AssetKind::Vrm,
            format!(
                "表情{}種 / 口形{}形",
                expressions.len(),
                visemes.len()
            ),
        )]))
    }
}

fn read_rig(path: &SafePath) -> anyhow::Result<Rig> {
    let text = permissions::fs::read_to_string(path).map_err(|error| anyhow::anyhow!(error))?;
    serde_json::from_str(&text)
        .map_err(|error| anyhow::anyhow!("ボーン定義を読めません: {error}"))
}

/// 変形指示のフォルダから、実際に作れている識別子を集める。
///
/// **ファイルがあるだけでは数えない。** 中身が読めて検証を通ったものだけを「作れた」と
/// 数える（空ファイルや壊れたJSONを数えると、欠損したまま書き出しへ進む）。
fn read_morph_ids(dir: &SafePath) -> anyhow::Result<Vec<String>> {
    let entries = std::fs::read_dir(dir.as_path())
        .map_err(|error| anyhow::anyhow!("変形指示を読めません: {dir} ({error})"))?;
    let mut ids = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(spec) = serde_json::from_str::<MorphSpec>(&text) else {
            continue;
        };
        if spec.validate().is_ok() {
            ids.push(spec.id);
        }
    }
    ids.sort();
    Ok(ids)
}

fn image_size(path: &SafePath) -> anyhow::Result<(u32, u32)> {
    let bytes = permissions::fs::read(path).map_err(|error| anyhow::anyhow!(error))?;
    let image = image::load_from_memory(&bytes)
        .map_err(|error| anyhow::anyhow!("画像を読めません: {path} ({error})"))?;
    Ok((image.width(), image.height()))
}

inventory::submit! { StageReg { make: || Box::new(Pack) } }

#[cfg(test)]
mod tests {
    use super::{read_morph_ids, Pack};
    use crate::permissions::SafePath;
    use crate::pipeline::Stage;
    use crate::vrm::{MorphControl, MorphSpec};

    fn temp_dir(label: &str) -> SafePath {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("picovtuber-pack-{label}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        SafePath::app_owned(dir)
    }

    fn write_spec(dir: &SafePath, id: &str, valid: bool) {
        let spec = MorphSpec {
            id: id.to_string(),
            vrm_name: format!("vrm_{id}"),
            controls: if valid {
                vec![MorphControl {
                    region: "mouth".to_string(),
                    offset: [0.0, 0.2],
                    scale: 1.2,
                }]
            } else {
                Vec::new()
            },
        };
        crate::permissions::fs::write(
            &dir.join(format!("{id}.json")).unwrap(),
            &serde_json::to_vec_pretty(&spec).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn 最後の関門として全部の成果物に依存する() {
        assert_eq!(
            Pack.requires(),
            &["rig", "expressions", "visemes", "source", "texture"]
        );
        assert_eq!(Pack.produces(), &["vrm"]);
    }

    /// ファイルがあるだけで数えると、空や壊れた指示を含んだまま書き出しへ進む。
    #[test]
    fn 中身が検証を通った指示だけを数える() {
        let dir = temp_dir("count");
        write_spec(&dir, "a", true);
        write_spec(&dir, "i", true);
        write_spec(&dir, "u", false); // 動かす箇所が空＝形が変わらない
        crate::permissions::fs::write(&dir.join("e.json").unwrap(), "{ 壊れている".as_bytes()).unwrap();
        crate::permissions::fs::write(&dir.join("メモ.txt").unwrap(), "json ではない".as_bytes()).unwrap();

        let ids = read_morph_ids(&dir).unwrap();
        assert_eq!(ids, vec!["a".to_string(), "i".to_string()]);
        std::fs::remove_dir_all(dir.as_path()).ok();
    }

    #[test]
    fn 読めないフォルダは空扱いにせず失敗する() {
        let dir = temp_dir("missing");
        let absent = dir.join("ここには無い").unwrap();
        assert!(
            read_morph_ids(&absent).is_err(),
            "空の一覧を返すと、欠損したまま書き出しへ進む"
        );
        std::fs::remove_dir_all(dir.as_path()).ok();
    }
}
