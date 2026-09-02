// pipeline/job.rs - 生成ジョブの永続化と再開。
//
// 生成は時間がかかり、途中でアプリが落ちたり利用者が閉じたりする。工程単位で
// 「どこまで終わったか」を `job.json` へ残し、次の起動では未完了工程から再開する。
//
// **`job.json` の記録だけを信じない。** 記録を書いた直後に落ちると、ファイルが無いのに
// 完了になっている状態が残る。完了扱いにするのは、その工程が申告した成果物が実在する
// 場合だけにする（存在しなければ、その工程からやり直す）。

use serde::{Deserialize, Serialize};

use crate::permissions::{self, SafePath};
use crate::pipeline::{Asset, StageOutput};
use crate::store;

/// `job.json` のファイル名。作業ディレクトリ直下に置く。
const JOB_FILE: &str = "job.json";

/// 完了した工程1件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletedStage {
    pub stage_id: String,
    pub assets: Vec<Asset>,
}

/// 生成ジョブの状態。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    /// 作成時刻（RFC 3339）。
    pub created_at: String,
    /// 取り込んだイラストの、作業ディレクトリからの相対パス。工程は上書きしない。
    pub input_relative: String,
    /// 取り込み時に利用者が答えた権利情報。VRM の利用許諾メタデータへ入れる。
    /// **空のまま書き出させない**（`vrm.rs` の検証が拒否する）。
    pub license_notice: String,
    pub completed: Vec<CompletedStage>,
    /// 自動での再試行回数。利用者が明示した再試行では数え直す。
    pub auto_retries: u32,
    /// 直近の失敗理由（画面へそのまま出す）。
    pub last_error: Option<String>,
}

impl Job {
    pub fn new(
        id: impl Into<String>,
        input_relative: impl Into<String>,
        license_notice: impl Into<String>,
    ) -> Job {
        Job {
            id: id.into(),
            created_at: chrono::Local::now().to_rfc3339(),
            input_relative: input_relative.into(),
            license_notice: license_notice.into(),
            completed: Vec::new(),
            auto_retries: 0,
            last_error: None,
        }
    }

    /// 完了済み工程が作った成果物を、名前で引ける形にまとめる。
    pub fn available_assets(&self) -> std::collections::HashMap<String, Asset> {
        let mut out = std::collections::HashMap::new();
        for stage in &self.completed {
            for asset in &stage.assets {
                out.insert(asset.name.clone(), asset.clone());
            }
        }
        out
    }

    /// 工程の完了を記録する（既存の記録は置き換える＝再実行に対応する）。
    pub fn mark_completed(&mut self, stage_id: &str, output: &StageOutput) {
        self.completed.retain(|stage| stage.stage_id != stage_id);
        self.completed.push(CompletedStage {
            stage_id: stage_id.to_string(),
            assets: output.assets.clone(),
        });
        self.last_error = None;
    }

    /// ある工程以降の記録を捨てる。工程をやり直すときに使う。
    ///
    /// 後続工程の記録を残したままにすると、消えた入力を指したまま「完了」に見える。
    pub fn invalidate_from(&mut self, order: &[String], stage_id: &str) {
        let Some(from) = order.iter().position(|id| id == stage_id) else {
            return;
        };
        let dropped: Vec<&String> = order.iter().skip(from).collect();
        self.completed
            .retain(|stage| !dropped.iter().any(|id| **id == stage.stage_id));
    }
}

/// この工程は「実際に」完了しているか。
///
/// 記録があるだけでは完了にしない。申告した成果物が1つでも実在しなければ未完了。
pub fn is_really_completed(job_dir: &SafePath, job: &Job, stage_id: &str) -> bool {
    let Some(stage) = job
        .completed
        .iter()
        .find(|stage| stage.stage_id == stage_id)
    else {
        return false;
    };
    if stage.assets.is_empty() {
        // 何も作らない工程は記録がある時点で完了とみなす。
        return true;
    }
    stage.assets.iter().all(|asset| {
        job_dir
            .join(&asset.relative_path)
            .map(|path| permissions::fs::exists(&path))
            .unwrap_or(false)
    })
}

/// 実行計画のうち、まだやる必要のある工程を返す。
pub fn remaining_stages(job_dir: &SafePath, job: &Job, order: &[String]) -> Vec<String> {
    order
        .iter()
        .filter(|stage_id| !is_really_completed(job_dir, job, stage_id))
        .cloned()
        .collect()
}

/// 自動での再試行がまだ許されるか。
///
/// 上限を設けないと、同じ理由で失敗し続ける工程がGPUを占有したまま終わらない。
pub fn can_auto_retry(cfg: &crate::config::Config, job: &Job) -> bool {
    job.auto_retries < cfg.get_u32("PICOVTUBER_JOB_MAX_AUTO_RETRY", 5)
}

/// `job.json` を読む。**読めなければエラーを返す**（空のジョブで上書きしない）。
///
/// 空で続けると、次の保存で完了記録も権利情報も消える。破損ファイルは退避して、
/// 利用者が中身を確認できるようにする。
pub fn load(job_dir: &SafePath) -> anyhow::Result<Job> {
    let path = job_dir
        .join(JOB_FILE)
        .map_err(|error| anyhow::anyhow!(error))?;
    let text = permissions::fs::read_to_string(&path).map_err(|error| anyhow::anyhow!(error))?;
    match serde_json::from_str::<Job>(&text) {
        Ok(job) => Ok(job),
        Err(error) => {
            if let Some(saved) = store::quarantine_corrupt(path.as_path()) {
                anyhow::bail!(
                    "生成ジョブの記録を読めません（{error}）。破損ファイルは {} へ退避しました。\
                     取り込みからやり直してください。",
                    saved.display()
                );
            }
            anyhow::bail!(
                "生成ジョブの記録を読めず、退避もできません（{error}）: {}",
                path.as_path().display()
            );
        }
    }
}

/// `job.json` を原子的に書く。
pub fn save(job_dir: &SafePath, job: &Job) -> anyhow::Result<()> {
    // 進捗の記録と、画面からの再試行・中止が同じファイルを触る。
    // 「読む → 直す → 書き戻す」を直列化しないと、片方の更新が消える。
    let _guard = store::lock_state(&format!("job:{}", job.id));
    let path = job_dir
        .join(JOB_FILE)
        .map_err(|error| anyhow::anyhow!(error))?;
    store::write_json_atomic(path.as_path(), job)
}

#[cfg(test)]
mod tests {
    use super::{can_auto_retry, is_really_completed, load, remaining_stages, save, Job};
    use crate::config::Config;
    use crate::permissions::SafePath;
    use crate::pipeline::{Asset, AssetKind, StageOutput};

    fn temp_job_dir(label: &str) -> SafePath {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("picovtuber-job-{label}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        SafePath::app_owned(dir)
    }

    fn output(relative: &str) -> StageOutput {
        StageOutput::new(vec![Asset::new(
            "source",
            relative,
            AssetKind::Image,
            "1024x2048",
        )])
    }

    #[test]
    fn 保存した記録を読み直せる() {
        let dir = temp_job_dir("roundtrip");
        let mut job = Job::new("job-1", "input.png", "自作イラスト");
        job.mark_completed("preprocess", &output("preprocess/source.png"));
        save(&dir, &job).unwrap();

        let reloaded = load(&dir).unwrap();
        assert_eq!(reloaded.id, "job-1");
        assert_eq!(reloaded.license_notice, "自作イラスト");
        assert_eq!(reloaded.completed.len(), 1);
        std::fs::remove_dir_all(dir.as_path()).ok();
    }

    /// 記録を書いた直後に落ちると、ファイルが無いのに完了になっている状態が残る。
    /// 成果物の実在を確かめないと、次の工程が空の入力を掴んで最後まで通ってしまう。
    #[test]
    fn 成果物が実在しない工程は完了扱いにしない() {
        let dir = temp_job_dir("missing");
        let mut job = Job::new("job-2", "input.png", "自作");
        job.mark_completed("preprocess", &output("preprocess/source.png"));

        assert!(
            !is_really_completed(&dir, &job, "preprocess"),
            "記録だけで完了にしている"
        );

        // 成果物を実際に置けば完了になる。
        let asset = dir.join("preprocess/source.png").unwrap();
        crate::permissions::fs::write(&asset, b"png").unwrap();
        assert!(is_really_completed(&dir, &job, "preprocess"));
        std::fs::remove_dir_all(dir.as_path()).ok();
    }

    #[test]
    fn 未完了の工程だけを再開対象にする() {
        let dir = temp_job_dir("resume");
        let order = vec![
            "preprocess".to_string(),
            "segment".to_string(),
            "pack".to_string(),
        ];
        let mut job = Job::new("job-3", "input.png", "自作");
        job.mark_completed("preprocess", &output("preprocess/source.png"));
        crate::permissions::fs::write(&dir.join("preprocess/source.png").unwrap(), b"png").unwrap();

        assert_eq!(
            remaining_stages(&dir, &job, &order),
            vec!["segment".to_string(), "pack".to_string()]
        );
        std::fs::remove_dir_all(dir.as_path()).ok();
    }

    /// ある工程をやり直すとき、後続の記録を残すと消えた入力を指したまま完了に見える。
    #[test]
    fn やり直す工程より後ろの記録を捨てる() {
        let order = vec![
            "preprocess".to_string(),
            "segment".to_string(),
            "pack".to_string(),
        ];
        let mut job = Job::new("job-4", "input.png", "自作");
        job.mark_completed("preprocess", &output("preprocess/source.png"));
        job.mark_completed("segment", &output("segment/masks/head.png"));
        job.mark_completed("pack", &output("pack/model.vrm"));

        job.invalidate_from(&order, "segment");
        let ids: Vec<&str> = job
            .completed
            .iter()
            .map(|stage| stage.stage_id.as_str())
            .collect();
        assert_eq!(ids, vec!["preprocess"]);
    }

    #[test]
    fn 再実行しても完了記録が二重にならない() {
        let mut job = Job::new("job-5", "input.png", "自作");
        job.mark_completed("preprocess", &output("preprocess/source.png"));
        job.mark_completed("preprocess", &output("preprocess/source.png"));
        assert_eq!(job.completed.len(), 1);
    }

    #[test]
    fn 自動再試行には上限がある() {
        let cfg = Config::new();
        let mut job = Job::new("job-6", "input.png", "自作");
        assert!(can_auto_retry(&cfg, &job));
        job.auto_retries = 5;
        assert!(
            !can_auto_retry(&cfg, &job),
            "上限を超えても再試行し続けている"
        );
        // 上限は設定で変えられる。
        cfg.set("PICOVTUBER_JOB_MAX_AUTO_RETRY", "10");
        assert!(can_auto_retry(&cfg, &job));
    }

    /// 空のジョブで上書きすると、完了記録も権利情報も消える。
    #[test]
    fn 壊れた記録は退避してエラーを返す() {
        let dir = temp_job_dir("corrupt");
        let path = dir.join("job.json").unwrap();
        crate::permissions::fs::write(&path, "{ こわれている".as_bytes()).unwrap();

        let error = load(&dir).expect_err("空のジョブで続けない").to_string();
        assert!(error.contains("退避"), "{error}");
        let quarantined: Vec<String> = std::fs::read_dir(dir.as_path())
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.contains(".corrupt-"))
            .collect();
        assert_eq!(quarantined.len(), 1, "破損ファイルが退避されていない");
        std::fs::remove_dir_all(dir.as_path()).ok();
    }
}
