// pipeline/mod.rs - 生成工程（Stage）のトレイト・レジストリ・実行計画。
//
// 「1枚のイラストをVRMへ変える」処理を工程コネクタの列として表す。工程は
// `src/pipeline/stages/<工程名>.rs` へ置けば `build.rs` が生成する `#[path] pub mod` と
// `inventory::submit!` で自動登録される。中央のレジストリは編集しない。
//
// **実行順は配列で直書きしない。** 各工程が `requires()` / `produces()` で申告し、
// `plan()` がその申告からトポロジカル順を決める。直書きすると、工程を1つ足すたびに
// 離れた場所を直すことになり、順序と依存がずれた状態が生まれる。循環・欠落・正本の
// 重複は `pipeline::guard` のテストが検出する。

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::field::Field;
use crate::permissions::SafePath;

pub mod job;
pub mod runtime;

// build.rs が `src/pipeline/stages/*.rs` を走査して生成する `pub mod` 宣言。
pub mod stages {
    include!(concat!(env!("OUT_DIR"), "/stage_mods.rs"));
}

/// 成果物の種別。工程間の受け渡しで「何を受け取ったか」を型で示す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetKind {
    /// 単一の画像（PNG）。
    Image,
    /// 画像の集合（マスク一式、多視点ビュー一式）。
    ImageSet,
    /// メッシュとテクスチャ。
    Mesh,
    /// ボーンとスキニングの入ったメッシュ。
    Rig,
    /// ブレンドシェイプ（表情・口形）の定義一式。
    MorphSet,
    /// 書き出したVRM。
    Vrm,
}

/// 工程が作った成果物1件。
///
/// **絶対パスを持たせない。** ジョブディレクトリごと移動できるようにするためと、
/// 診断ログへ利用者のフォルダ名を漏らさないため、パスは作業ディレクトリからの相対で持つ。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    /// 成果物の名前（`requires()` / `produces()` で使う識別子）。
    pub name: String,
    /// 作業ディレクトリからの相対パス。
    pub relative_path: String,
    pub kind: AssetKind,
    /// 寸法・頂点数など、検証と表示に使う一言。
    pub detail: String,
}

impl Asset {
    pub fn new(
        name: impl Into<String>,
        relative_path: impl Into<String>,
        kind: AssetKind,
        detail: impl Into<String>,
    ) -> Asset {
        Asset {
            name: name.into(),
            relative_path: relative_path.into(),
            kind,
            detail: detail.into(),
        }
    }
}

/// 工程の実行結果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageOutput {
    pub assets: Vec<Asset>,
}

impl StageOutput {
    pub fn new(assets: Vec<Asset>) -> StageOutput {
        StageOutput { assets }
    }
}

/// 進捗の通知口。`0.0..=1.0` と一言メッセージを送る。
pub type ProgressSink = Arc<dyn Fn(f32, &str) + Send + Sync>;

/// 何もしない進捗口（テストと、通知先が無い実行で使う）。
pub fn silent_progress() -> ProgressSink {
    Arc::new(|_, _| {})
}

/// 工程が触ってよい範囲を型で閉じ込めた実行文脈。
pub struct StageContext {
    /// この生成ジョブの作業ディレクトリ。工程が書いてよいのはこの下だけ。
    pub job_dir: SafePath,
    /// 設定の参照。値はフィールドへ固定せず毎回 `cfg.get()` で読む。
    pub cfg: Arc<Config>,
    /// 利用者が取り込んだイラストの、作業ディレクトリからの相対パス。
    ///
    /// これは工程の成果物ではないので `inputs` には入れない（入れると「誰かが作った
    /// もの」として依存解決の対象になり、最初の工程が実行不能になる）。
    /// **どの工程もこのファイルを上書きしない。**
    pub original_input: String,
    /// 先行工程が作った成果物（成果物名 → `Asset`）。
    pub inputs: HashMap<String, Asset>,
    progress: ProgressSink,
    cancel: Arc<AtomicBool>,
}

impl StageContext {
    pub fn new(job_dir: SafePath, cfg: Arc<Config>) -> StageContext {
        StageContext {
            job_dir,
            cfg,
            original_input: "input.png".to_string(),
            inputs: HashMap::new(),
            progress: silent_progress(),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn with_original_input(mut self, relative: impl Into<String>) -> StageContext {
        self.original_input = relative.into();
        self
    }

    pub fn with_inputs(mut self, inputs: HashMap<String, Asset>) -> StageContext {
        self.inputs = inputs;
        self
    }

    /// 取り込んだイラストの実体パス。読み取り専用として扱うこと。
    pub fn original_input_path(&self) -> anyhow::Result<SafePath> {
        self.job_dir
            .join(&self.original_input)
            .map_err(|error| anyhow::anyhow!(error))
    }

    pub fn with_progress(mut self, progress: ProgressSink) -> StageContext {
        self.progress = progress;
        self
    }

    pub fn with_cancel(mut self, cancel: Arc<AtomicBool>) -> StageContext {
        self.cancel = cancel;
        self
    }

    /// 進捗を送る。長い工程は定期的に呼ぶこと（無反応の画面は失敗と見分けがつかない）。
    pub fn report(&self, ratio: f32, message: &str) {
        (self.progress)(ratio.clamp(0.0, 1.0), message);
    }

    /// 利用者が中止したか。長い処理は定期的に確認して途中で戻る。
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// 中止されていたらエラーで抜ける。工程の区切りごとに呼ぶ。
    pub fn check_cancelled(&self) -> anyhow::Result<()> {
        if self.cancelled() {
            anyhow::bail!("利用者の操作で中止しました");
        }
        Ok(())
    }

    /// 先行工程の成果物を取り出す。無ければ**失敗させる**。
    ///
    /// 「無ければ空で続ける」を許すと、欠損したまま最後まで通って利用者が配信本番で
    /// 気づくことになる。
    pub fn input(&self, name: &str) -> anyhow::Result<&Asset> {
        self.inputs.get(name).ok_or_else(|| {
            anyhow::anyhow!(
                "先行工程の成果物「{name}」がありません（工程の依存申告を確認してください）"
            )
        })
    }

    /// 工程の出力先フォルダ（`<作業ディレクトリ>/<工程ID>/`）を作って返す。
    pub fn output_dir(&self, stage_id: &str) -> anyhow::Result<SafePath> {
        let dir = self
            .job_dir
            .join(stage_id)
            .map_err(|error| anyhow::anyhow!(error))?;
        crate::permissions::fs::create_dir_all(&dir).map_err(|error| anyhow::anyhow!(error))?;
        Ok(dir)
    }

    /// 成果物の相対パスから実体のパスを作る。
    pub fn resolve_asset(&self, asset: &Asset) -> anyhow::Result<SafePath> {
        self.job_dir
            .join(&asset.relative_path)
            .map_err(|error| anyhow::anyhow!(error))
    }
}

/// 生成工程コネクタ。
#[async_trait]
pub trait Stage: Send + Sync {
    /// 工程ID（英小文字・数字・アンダースコア）。ファイル名と一致させる。
    fn id(&self) -> &'static str;
    /// 設定画面と進捗表示に出す日本語名。
    fn label(&self) -> &'static str;
    /// この工程が必要とする成果物の名前。
    fn requires(&self) -> &'static [&'static str];
    /// この工程が生成する成果物の名前。
    fn produces(&self) -> &'static [&'static str];
    /// 機械学習ランタイムを使うか（未導入時の案内とGPU判定に使う）。
    fn uses_runtime(&self) -> bool {
        false
    }
    /// この工程の設定項目。設定画面へ自動で並ぶ。
    fn config_schema(&self) -> Vec<Field> {
        Vec::new()
    }
    /// 実行本体。**入力は破壊せず**、新しいファイルとして出力する。
    async fn run(&self, ctx: &StageContext) -> anyhow::Result<StageOutput>;
}

/// 自動登録の受け口。各工程ファイルの末尾で `inventory::submit!` する。
pub struct StageReg {
    pub make: fn() -> Box<dyn Stage>,
}

inventory::collect!(StageReg);

/// 登録済みの全工程。
pub fn all_stages() -> Vec<Box<dyn Stage>> {
    inventory::iter::<StageReg>
        .into_iter()
        .map(|reg| (reg.make)())
        .collect()
}

/// IDで1つ取り出す。
pub fn stage_by_id(id: &str) -> Option<Box<dyn Stage>> {
    all_stages().into_iter().find(|stage| stage.id() == id)
}

/// 依存申告から実行順を決める。
///
/// 返るのは工程IDの並び。循環依存や、誰も作らない成果物への依存はエラーにする
/// （そのまま走らせると、途中で「入力がありません」と落ちるだけで原因が分からない）。
pub fn plan() -> anyhow::Result<Vec<String>> {
    plan_from(&all_stages())
}

/// テストから任意の工程集合で計画を作れるようにした本体。
fn plan_from(stages: &[Box<dyn Stage>]) -> anyhow::Result<Vec<String>> {
    // 成果物 → それを作る工程ID。
    let mut producer: HashMap<&str, &str> = HashMap::new();
    for stage in stages {
        for asset in stage.produces() {
            if let Some(existing) = producer.insert(asset, stage.id()) {
                anyhow::bail!(
                    "成果物「{asset}」を複数の工程が作っています（{existing} と {}）。正本は1つにしてください。",
                    stage.id()
                );
            }
        }
    }
    for stage in stages {
        for need in stage.requires() {
            if !producer.contains_key(need) {
                anyhow::bail!(
                    "工程「{}」が必要とする成果物「{need}」を作る工程がありません。",
                    stage.id()
                );
            }
        }
    }

    // Kahn のトポロジカルソート。並びを安定させるため、実行可能な工程はID順で取る。
    let mut remaining: Vec<&Box<dyn Stage>> = stages.iter().collect();
    remaining.sort_by_key(|stage| stage.id());
    let mut done: HashSet<&str> = HashSet::new();
    let mut order: Vec<String> = Vec::new();

    while !remaining.is_empty() {
        let ready: Vec<usize> = remaining
            .iter()
            .enumerate()
            .filter(|(_, stage)| stage.requires().iter().all(|need| done.contains(need)))
            .map(|(index, _)| index)
            .collect();
        if ready.is_empty() {
            let stuck: Vec<&str> = remaining.iter().map(|stage| stage.id()).collect();
            anyhow::bail!(
                "工程の依存が循環しています（実行できない工程: {}）。requires と produces の申告を確認してください。",
                stuck.join(", ")
            );
        }
        // 後ろから抜くと添字がずれない。
        for index in ready.iter().rev() {
            let stage = remaining.remove(*index);
            for asset in stage.produces() {
                done.insert(asset);
            }
            order.push(stage.id().to_string());
        }
        // 同じ段で実行可能になった工程はID順に並べる（実行のたびに順序が変わらないように）。
        let settled = order.len() - ready.len();
        order.split_at_mut(settled).1.sort();
    }
    Ok(order)
}

/// 生成ジョブを、未完了の工程だけ最後まで進める。
///
/// 進捗は工程ごとの割合を全体の割合へ均して送る。工程が終わるたびに `job.json` を保存し、
/// 途中で落ちても次回は続きから始められるようにする。
///
/// **失敗した工程で止める。** 後続を「入力が無いまま」走らせても、欠損した成果物が
/// 積み上がるだけで、利用者は最後まで通ったと誤解する。
pub async fn run_job(
    job_dir: &SafePath,
    cfg: Arc<Config>,
    job: &mut job::Job,
    progress: ProgressSink,
    cancel: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let order = plan()?;
    let remaining = job::remaining_stages(job_dir, job, &order);
    let total = remaining.len().max(1);

    for (index, stage_id) in remaining.iter().enumerate() {
        let stage = stage_by_id(stage_id)
            .ok_or_else(|| anyhow::anyhow!("工程「{stage_id}」が見つかりません"))?;

        // 工程内の進捗（0..1）を、全体の進捗へ写す。
        let base = index as f32 / total as f32;
        let span = 1.0 / total as f32;
        let outer = Arc::clone(&progress);
        let label = stage.label();
        let stage_progress: ProgressSink = Arc::new(move |ratio, message| {
            outer(base + ratio * span, &format!("{label}: {message}"));
        });

        let ctx = StageContext::new(job_dir.clone(), Arc::clone(&cfg))
            .with_original_input(job.input_relative.clone())
            .with_inputs(job.available_assets())
            .with_progress(stage_progress)
            .with_cancel(Arc::clone(&cancel));

        match stage.run(&ctx).await {
            Ok(output) => {
                job.mark_completed(stage_id, &output);
                job::save(job_dir, job)?;
            }
            Err(error) => {
                // 失敗の理由は画面へそのまま出す（ログだけで済ませない）。
                job.last_error = Some(format!("{error:#}"));
                job::save(job_dir, job)?;
                return Err(error.context(format!("工程「{}」で止まりました", stage.label())));
            }
        }
    }
    progress(1.0, "生成が完了しました");
    Ok(())
}

/// 全工程の設定項目（設定画面が並べる）。
pub fn config_schema() -> Vec<Field> {
    let mut out = Vec::new();
    for stage in all_stages() {
        out.extend(stage.config_schema());
    }
    out
}

/// 工程コネクタの設計規則を検査するガード。
#[cfg(test)]
mod guard {
    use crate::guard_scan;

    #[test]
    fn 実行順を決められる() {
        // 循環・欠落・正本の重複があればここで理由付きに落ちる。
        let order = super::plan().expect("実行計画を作れること");
        assert!(!order.is_empty(), "工程が1つも登録されていない");
    }

    #[test]
    fn 登録数とファイル数が一致する() {
        let registered = super::all_stages().len();
        let files = guard_scan::stage_sources().len();
        assert_eq!(
            registered, files,
            "工程ファイルはあるのに inventory::submit! が漏れている（または逆）"
        );
    }

    #[test]
    fn 工程idはファイル名と一致する() {
        let mut offenders = Vec::new();
        for source in guard_scan::stage_sources() {
            let file_stem = source.name.trim_end_matches(".rs").to_string();
            if super::stage_by_id(&file_stem).is_none() {
                offenders.push(source.path.display().to_string());
            }
        }
        assert!(
            offenders.is_empty(),
            "工程IDはファイル名と一致させてください（一致しないと再実行の指定先が分からなくなります）: {offenders:?}"
        );
    }

    /// 工程IDを列挙したファイルがあれば、それは実行順か分岐の直書き。
    ///
    /// 工程を1つ足すたびに離れた場所を直すことになり、順序と依存がずれる。順序は
    /// `requires()` の申告から `plan()` が決めるので、列挙は要らない。
    #[test]
    fn 工程idを他所で列挙しない() {
        let ids: Vec<String> = super::all_stages()
            .iter()
            .map(|stage| format!("\"{}\"", stage.id()))
            .collect();
        let mut offenders = Vec::new();
        for source in guard_scan::all_sources() {
            if source.is_stage() || source.name == "mod.rs" {
                continue; // 工程本体と、レジストリ側の定義
            }
            let body = source.body();
            let hits = ids.iter().filter(|id| body.contains(id.as_str())).count();
            if hits > 2 {
                offenders.push(format!("{} ({hits}件)", source.path.display()));
            }
        }
        assert!(
            offenders.is_empty(),
            "工程IDの列挙は実行順や分岐の直書きです。requires() の申告に寄せてください: {offenders:?}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{plan_from, Asset, AssetKind, Stage, StageContext, StageOutput};
    use crate::config::Config;
    use crate::permissions::SafePath;
    use async_trait::async_trait;
    use std::sync::Arc;

    struct Fake {
        id: &'static str,
        requires: &'static [&'static str],
        produces: &'static [&'static str],
    }

    #[async_trait]
    impl Stage for Fake {
        fn id(&self) -> &'static str {
            self.id
        }
        fn label(&self) -> &'static str {
            "テスト工程"
        }
        fn requires(&self) -> &'static [&'static str] {
            self.requires
        }
        fn produces(&self) -> &'static [&'static str] {
            self.produces
        }
        async fn run(&self, _ctx: &StageContext) -> anyhow::Result<StageOutput> {
            Ok(StageOutput::new(Vec::new()))
        }
    }

    fn stage(
        id: &'static str,
        requires: &'static [&'static str],
        produces: &'static [&'static str],
    ) -> Box<dyn Stage> {
        Box::new(Fake {
            id,
            requires,
            produces,
        })
    }

    #[test]
    fn 依存申告から実行順が決まる() {
        let stages = vec![
            stage("third", &["middle"], &["last"]),
            stage("first", &[], &["start"]),
            stage("second", &["start"], &["middle"]),
        ];
        let order = plan_from(&stages).unwrap();
        assert_eq!(order, vec!["first", "second", "third"]);
    }

    /// 実行のたびに順序が変わると、進捗表示も再開位置も再現しなくなる。
    #[test]
    fn 同時に実行可能な工程も順序が安定する() {
        let stages = vec![
            stage("beta", &["start"], &["b"]),
            stage("alpha", &["start"], &["a"]),
            stage("root", &[], &["start"]),
        ];
        let first = plan_from(&stages).unwrap();
        let second = plan_from(&stages).unwrap();
        assert_eq!(first, second);
        assert_eq!(first, vec!["root", "alpha", "beta"]);
    }

    #[test]
    fn 循環依存は理由付きで断る() {
        let stages = vec![stage("a", &["y"], &["x"]), stage("b", &["x"], &["y"])];
        let error = plan_from(&stages).unwrap_err().to_string();
        assert!(error.contains("循環"), "{error}");
    }

    #[test]
    fn 誰も作らない成果物への依存を断る() {
        let stages = vec![stage("a", &["どこにもない"], &["x"])];
        let error = plan_from(&stages).unwrap_err().to_string();
        assert!(error.contains("作る工程がありません"), "{error}");
    }

    /// 同じ成果物を2つの工程が名乗ると、あとから走った方が前の出力を上書きする。
    #[test]
    fn 成果物の正本が重複したら断る() {
        let stages = vec![stage("a", &[], &["x"]), stage("b", &[], &["x"])];
        let error = plan_from(&stages).unwrap_err().to_string();
        assert!(error.contains("複数の工程"), "{error}");
    }

    #[test]
    fn 足りない入力は空で続けずに失敗する() {
        let ctx = StageContext::new(
            SafePath::app_owned(std::env::temp_dir()),
            Arc::new(Config::new()),
        );
        let error = ctx.input("source").unwrap_err().to_string();
        assert!(error.contains("ありません"), "{error}");
    }

    #[test]
    fn 中止フラグは工程の区切りで効く() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let cancel = Arc::new(AtomicBool::new(false));
        let ctx = StageContext::new(
            SafePath::app_owned(std::env::temp_dir()),
            Arc::new(Config::new()),
        )
        .with_cancel(Arc::clone(&cancel));
        assert!(ctx.check_cancelled().is_ok());
        cancel.store(true, Ordering::Relaxed);
        assert!(ctx.check_cancelled().is_err());
    }

    /// 失敗した工程で止まり、理由が記録されること。
    ///
    /// 止まらずに後続を走らせると、欠損した成果物が積み上がったまま最後まで通り、
    /// 利用者は「できた」と誤解する。ここでは重み未取得で `segment` が失敗する経路を
    /// 使い、`preprocess` までは完了として残ることを確かめる。
    #[tokio::test]
    async fn 失敗した工程で止まり理由を残す() {
        use crate::config::Config;
        use crate::pipeline::job;

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("picovtuber-runjob-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let job_dir = SafePath::app_owned(dir.clone());

        // 下ごしらえを通せる大きさの、全身が入った体で立ち絵を模す。
        let mut image = image::RgbaImage::new(1600, 2400);
        for y in 100..2300 {
            for x in 200..1400 {
                image.put_pixel(x, y, image::Rgba([210, 190, 170, 255]));
            }
        }
        let mut encoded = Vec::new();
        image
            .write_to(
                &mut std::io::Cursor::new(&mut encoded),
                image::ImageFormat::Png,
            )
            .unwrap();
        crate::permissions::fs::write(&job_dir.join("input.png").unwrap(), &encoded).unwrap();

        let cfg = Config::new();
        // ランタイム未導入にして、2つ目の工程で必ず止まるようにする。
        cfg.set("PICOVTUBER_RUNTIME_PYTHON", "/存在しない/python");
        let cfg = Arc::new(cfg);

        let mut job = job::Job::new("job-run", "input.png", "自作イラスト");
        let error = super::run_job(
            &job_dir,
            Arc::clone(&cfg),
            &mut job,
            super::silent_progress(),
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .await
        .expect_err("重みが無いので途中で止まる");
        assert!(error.to_string().contains("止まりました"), "{error:#}");

        // 通った工程は完了として残り、失敗の理由も記録されている。
        let saved = job::load(&job_dir).unwrap();
        let completed: Vec<&str> = saved
            .completed
            .iter()
            .map(|stage| stage.stage_id.as_str())
            .collect();
        assert_eq!(completed, vec!["preprocess"]);
        assert!(saved.last_error.is_some(), "失敗理由が残っていない");

        // 再開すると、完了済みの工程は飛ばして未完了から始まる。
        let order = super::plan().unwrap();
        let remaining = job::remaining_stages(&job_dir, &saved, &order);
        assert_eq!(remaining.first().map(String::as_str), Some("segment"));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// 中止したら、その場で止まって後続の工程を走らせないこと。
    #[tokio::test]
    async fn 中止すると工程を進めない() {
        use crate::config::Config;
        use crate::pipeline::job;

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("picovtuber-cancel-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let job_dir = SafePath::app_owned(dir.clone());
        crate::permissions::fs::write(&job_dir.join("input.png").unwrap(), b"not-an-image")
            .unwrap();

        let mut job = job::Job::new("job-cancel", "input.png", "自作");
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let error = super::run_job(
            &job_dir,
            Arc::new(Config::new()),
            &mut job,
            super::silent_progress(),
            cancel,
        )
        .await
        .expect_err("中止済みなら進めない");
        assert!(error.to_string().contains("止まりました"), "{error:#}");
        assert!(job.completed.is_empty(), "中止したのに工程が完了している");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn 成果物は作業ディレクトリ相対のパスを持つ() {
        let asset = Asset::new(
            "source",
            "preprocess/source.png",
            AssetKind::Image,
            "1024x2048",
        );
        assert!(
            !asset.relative_path.starts_with('/'),
            "絶対パスを持たせない（ジョブごと移動できなくなり、ログにも利用者のフォルダ名が出る）"
        );
    }
}
