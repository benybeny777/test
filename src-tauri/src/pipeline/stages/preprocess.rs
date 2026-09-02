// preprocess.rs - 入力イラストの正規化。
//
// 取り込んだ絵をそのまま後続工程へ渡すと、余白の量、寸法、アルファの有無がばらばらで、
// 分割も多視点生成も結果が安定しない。ここで「余白を切る → 等倍以下へ収める → RGBA へ
// 揃える」だけを行う。
//
// **引き伸ばし拡大は行わない**（AGENTS.md の規則）。小さい絵を大きくしても情報は
// 増えず、後続工程は「解像度がある」と誤認したまま甘い結果を出す。足りない場合は
// 足りないと伝える。

use async_trait::async_trait;

use crate::field::Field;
use crate::permissions;
use crate::pipeline::{Asset, AssetKind, Stage, StageContext, StageOutput, StageReg};

pub struct Preprocess;

#[async_trait]
impl Stage for Preprocess {
    fn id(&self) -> &'static str {
        "preprocess"
    }

    fn label(&self) -> &'static str {
        "下ごしらえ"
    }

    fn requires(&self) -> &'static [&'static str] {
        &[]
    }

    fn produces(&self) -> &'static [&'static str] {
        &["source"]
    }

    fn config_schema(&self) -> Vec<Field> {
        vec![
            Field::number(
                "PICOVTUBER_PREPROCESS_MAX_EDGE",
                "正規化後の最大辺",
                "この大きさを超える絵だけ縮めます。小さい絵を引き伸ばすことはありません。",
                "2048",
            ),
            Field::number(
                "PICOVTUBER_PREPROCESS_TRIM_ALPHA",
                "余白と見なすアルファ値",
                "この値以下の透明度を余白として切り落とします（0〜255）。",
                "8",
            ),
            Field::number(
                "PICOVTUBER_PREPROCESS_MIN_EDGE",
                "必要な最小の長辺",
                "これより小さい絵はテクスチャ解像度が足りないため、生成を始めずに断ります。",
                "1024",
            ),
        ]
    }

    async fn run(&self, ctx: &StageContext) -> anyhow::Result<StageOutput> {
        ctx.report(0.0, "イラストを読み込み中");
        let input = ctx.original_input_path()?;
        let bytes = permissions::fs::read(&input).map_err(|error| anyhow::anyhow!(error))?;
        let image = image::load_from_memory(&bytes)
            .map_err(|error| anyhow::anyhow!("イラストを画像として読めません: {error}"))?;
        let mut rgba = image.to_rgba8();

        ctx.check_cancelled()?;
        ctx.report(0.3, "余白を切り落とし中");
        let alpha_threshold = ctx
            .cfg
            .get_u32("PICOVTUBER_PREPROCESS_TRIM_ALPHA", 8)
            .min(255) as u8;
        let bounds = opaque_bounds(&rgba, alpha_threshold).ok_or_else(|| {
            anyhow::anyhow!(
                "体の輪郭を取れませんでした。背景が残っていないか、全身が入っているかを確認してください。"
            )
        })?;
        rgba = image::imageops::crop_imm(
            &rgba,
            bounds.left,
            bounds.top,
            bounds.width(),
            bounds.height(),
        )
        .to_image();

        ctx.check_cancelled()?;
        let min_edge = ctx.cfg.get_u32("PICOVTUBER_PREPROCESS_MIN_EDGE", 1024);
        let longest = rgba.width().max(rgba.height());
        if longest < min_edge {
            anyhow::bail!(
                "テクスチャ解像度が足りません（切り出し後の長辺 {longest}px、必要 {min_edge}px 以上）。\
                 引き伸ばしでは補えないため、より大きい元絵を使ってください。"
            );
        }

        ctx.report(0.7, "寸法を整え中");
        let max_edge = ctx.cfg.get_u32("PICOVTUBER_PREPROCESS_MAX_EDGE", 2048);
        // 縮小だけを行う。`thumbnail` は縦横比を保ち、上限を超えない寸法に収める。
        if longest > max_edge {
            rgba = image::imageops::thumbnail(
                &rgba,
                scaled(rgba.width(), longest, max_edge),
                scaled(rgba.height(), longest, max_edge),
            );
        }

        let output_dir = ctx.output_dir(self.id())?;
        let target = output_dir
            .join("source.png")
            .map_err(|error| anyhow::anyhow!(error))?;
        let mut encoded = Vec::new();
        rgba.write_to(
            &mut std::io::Cursor::new(&mut encoded),
            image::ImageFormat::Png,
        )
        .map_err(|error| anyhow::anyhow!("正規化した画像を書き出せません: {error}"))?;
        permissions::fs::write(&target, &encoded).map_err(|error| anyhow::anyhow!(error))?;

        ctx.report(1.0, "下ごしらえ完了");
        Ok(StageOutput::new(vec![Asset::new(
            "source",
            format!("{}/source.png", self.id()),
            AssetKind::Image,
            format!("{}x{}", rgba.width(), rgba.height()),
        )]))
    }
}

/// 不透明な画素を囲む矩形。
#[derive(Debug, PartialEq, Eq)]
struct Bounds {
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
}

impl Bounds {
    fn width(&self) -> u32 {
        self.right - self.left + 1
    }
    fn bottom_inclusive_height(&self) -> u32 {
        self.bottom - self.top + 1
    }
    fn height(&self) -> u32 {
        self.bottom_inclusive_height()
    }
}

/// アルファが閾値を超える画素の外接矩形。すべて透明なら `None`。
fn opaque_bounds(image: &image::RgbaImage, alpha_threshold: u8) -> Option<Bounds> {
    let (mut left, mut top) = (u32::MAX, u32::MAX);
    let (mut right, mut bottom) = (0_u32, 0_u32);
    let mut found = false;
    for (x, y, pixel) in image.enumerate_pixels() {
        if pixel.0[3] <= alpha_threshold {
            continue;
        }
        found = true;
        left = left.min(x);
        top = top.min(y);
        right = right.max(x);
        bottom = bottom.max(y);
    }
    found.then_some(Bounds {
        left,
        top,
        right,
        bottom,
    })
}

/// 長辺を `max_edge` に収めたときの、この辺の長さ。0 にはしない。
fn scaled(edge: u32, longest: u32, max_edge: u32) -> u32 {
    let scaled = (u64::from(edge) * u64::from(max_edge) / u64::from(longest.max(1))) as u32;
    scaled.max(1)
}

inventory::submit! { StageReg { make: || Box::new(Preprocess) } }

#[cfg(test)]
mod tests {
    use super::{opaque_bounds, scaled, Preprocess};
    use crate::config::Config;
    use crate::permissions::SafePath;
    use crate::pipeline::{Stage, StageContext};
    use std::sync::Arc;

    fn temp_dir(label: &str) -> SafePath {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("picovtuber-pre-{label}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        SafePath::app_owned(dir)
    }

    /// 中央に不透明な四角を置いた、周囲が透明な画像を作る。
    fn sample(width: u32, height: u32, inset: u32) -> image::RgbaImage {
        let mut image = image::RgbaImage::new(width, height);
        for y in inset..height - inset {
            for x in inset..width - inset {
                image.put_pixel(x, y, image::Rgba([200, 180, 160, 255]));
            }
        }
        image
    }

    fn write_input(dir: &SafePath, image: &image::RgbaImage) {
        let mut encoded = Vec::new();
        image
            .write_to(
                &mut std::io::Cursor::new(&mut encoded),
                image::ImageFormat::Png,
            )
            .unwrap();
        crate::permissions::fs::write(&dir.join("input.png").unwrap(), &encoded).unwrap();
    }

    #[test]
    fn 余白の外接矩形を取る() {
        let image = sample(100, 200, 10);
        let bounds = opaque_bounds(&image, 8).expect("不透明な領域がある");
        assert_eq!(bounds.left, 10);
        assert_eq!(bounds.top, 10);
        assert_eq!(bounds.width(), 80);
        assert_eq!(bounds.height(), 180);

        // 全部透明なら「輪郭を取れない」として None（勝手に全面を採用しない）。
        let empty = image::RgbaImage::new(10, 10);
        assert!(opaque_bounds(&empty, 8).is_none());
    }

    #[test]
    fn 縮小の計算は縦横比を保ち0にしない() {
        assert_eq!(scaled(4000, 4000, 2048), 2048);
        assert_eq!(scaled(2000, 4000, 2048), 1024);
        // 極端に細い辺でも 0 にしない（0 幅の画像は書き出せない）。
        assert_eq!(scaled(1, 4000, 2048), 1);
    }

    #[tokio::test]
    async fn 余白を切って等倍以下へ整える() {
        let dir = temp_dir("normalize");
        write_input(&dir, &sample(3000, 4000, 100));
        let cfg = Arc::new(Config::new());
        let ctx = StageContext::new(dir.clone(), cfg);

        let output = Preprocess.run(&ctx).await.unwrap();
        assert_eq!(output.assets.len(), 1);
        let asset = &output.assets[0];
        assert_eq!(asset.name, "source");
        assert_eq!(asset.relative_path, "preprocess/source.png");
        // 余白を切ると 2800x3800。長辺 3800px が上限 2048px へ収まり、縦横比は保たれる。
        assert_eq!(asset.detail, "1509x2048");

        // 入力は上書きしない。
        let input = dir.join("input.png").unwrap();
        assert!(crate::permissions::fs::exists(&input));
        std::fs::remove_dir_all(dir.as_path()).ok();
    }

    /// 引き伸ばし拡大は禁止。小さい絵は「足りない」と伝えて止まる。
    #[tokio::test]
    async fn 小さい絵を引き伸ばさず理由付きで断る() {
        let dir = temp_dir("small");
        write_input(&dir, &sample(300, 400, 10));
        let ctx = StageContext::new(dir.clone(), Arc::new(Config::new()));

        let error = Preprocess.run(&ctx).await.unwrap_err().to_string();
        assert!(error.contains("テクスチャ解像度が足りません"), "{error}");
        assert!(error.contains("引き伸ばし"), "{error}");
        std::fs::remove_dir_all(dir.as_path()).ok();
    }

    /// 背景が残っている絵は輪郭が取れない。白紙で続けず理由を返す。
    #[tokio::test]
    async fn 輪郭を取れない絵は理由付きで断る() {
        let dir = temp_dir("nooutline");
        write_input(&dir, &image::RgbaImage::new(2000, 2000));
        let ctx = StageContext::new(dir.clone(), Arc::new(Config::new()));

        let error = Preprocess.run(&ctx).await.unwrap_err().to_string();
        assert!(error.contains("体の輪郭を取れませんでした"), "{error}");
        std::fs::remove_dir_all(dir.as_path()).ok();
    }

    #[tokio::test]
    async fn 上限より小さい絵は寸法を変えない() {
        let dir = temp_dir("keep");
        write_input(&dir, &sample(1200, 1600, 0));
        let ctx = StageContext::new(dir.clone(), Arc::new(Config::new()));

        let output = Preprocess.run(&ctx).await.unwrap();
        assert_eq!(output.assets[0].detail, "1200x1600");
        std::fs::remove_dir_all(dir.as_path()).ok();
    }
}
