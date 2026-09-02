// store.rs - アプリ状態ファイル（config.json / jobs.json / models.json …）の永続化の共通層。
//
// 状態ファイルはどれも「読み込み → 全体を書き戻す」形で更新する。ここで `std::fs::write`
// を使うと、書き込み中にアプリが落ちる・電源が切れると**中身が途中まで**のファイルが残る。
// 各ストアは破損時に「空で継続」するフォールバックを持っているため、次の保存でその空の
// 状態がそのまま書き戻され、**設定や生成ジョブが恒久的に消える**。1回の中断が全損に
// つながる作りになってしまう。
//
// そこで書き込みは必ず「同じディレクトリの一時ファイルへ書く → fsync → rename」で行う。
// rename は同一ファイルシステム上で原子的なので、どの瞬間に落ちても「更新前の完全な
// ファイル」か「更新後の完全なファイル」のどちらかしか残らない。
//
// 新しい状態ファイルを足すときも必ずここを通すこと。直接 `fs::write` していないかは
// `store::guard` のテストが走査して検出する。

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;

use crate::config::Config;

/// 状態ファイルを原子的に書き換える。
///
/// 途中で落ちても更新前の内容が壊れない。書き込みに失敗した場合は一時ファイルを
/// 残さずに片付ける。
pub fn write_atomic(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("保存先の親ディレクトリを特定できません"))?;
    std::fs::create_dir_all(dir)?;

    let temp = temp_path(path);
    let result = (|| -> anyhow::Result<()> {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(bytes)?;
        // rename の前に中身をディスクへ確定させる。ここを省くと、rename だけが先に
        // 永続化されて「名前はあるが中身が空」のファイルが残りうる。
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)?;
        Ok(())
    })();
    match result {
        Ok(()) => Ok(()),
        Err(error) => match std::fs::remove_file(&temp) {
            Ok(()) => Err(error),
            Err(cleanup) if cleanup.kind() == std::io::ErrorKind::NotFound => Err(error),
            Err(cleanup) => Err(anyhow::anyhow!(
                "{error:#}; 原子的書き込みの一時ファイルも削除できません: {} ({cleanup})",
                temp.display()
            )),
        },
    }
}

/// 状態を JSON（整形済み）として原子的に書き出す。
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    let text = serde_json::to_string_pretty(value)?;
    write_atomic(path, text.as_bytes())
}

/// 設定に JSON 文字列で保存された一覧を読む。**壊れていても空一覧にしない。**
///
/// 表情プリセットや配信プロファイルは「読み込む → 要素を足す → 全体を書き戻す」形で
/// 更新する。読めなかったときに空一覧から始めると、**次の保存で登録済みの内容がすべて
/// 消える**。空一覧と「読めなかった」を型で区別し、書き戻す側がエラーをそのまま
/// ユーザーへ返せるようにする。
pub fn load_list<T: serde::de::DeserializeOwned>(
    cfg: &Config,
    key: &str,
) -> Result<Vec<T>, String> {
    let raw = cfg.get(key, "");
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&raw).map_err(|error| {
        format!(
            "保存済みの設定「{key}」を読み取れません（{error}）。\
             上書きで失わないよう操作を中止しました。設定画面から内容を直すか、\
             作り直す場合は設定画面で該当項目を空にしてください。"
        )
    })
}

/// 状態の「読む → 直す → 書き戻す」を直列化する、名前付きロック。
///
/// 生成パイプラインは複数の工程と進捗通知を並行して進め、配信モードは別スレッドの
/// 音声処理から状態を触る。ロックが無いと、同じ状態を更新する2つの処理が同じ内容を
/// 読み、あとから書いた側の内容だけが残って**もう片方の変更が消える**（lost update）。
/// 読み込みの直前に取得し、書き戻しが終わるまで保持すること。
///
/// 引数は対象を表す任意の名前（設定キーやファイル種別）。
/// 返り値を保持したまま `.await` しないこと（状態更新は同期処理で完結させる）。
pub fn lock_state(key: &str) -> std::sync::MutexGuard<'static, ()> {
    static LOCKS: OnceLock<Mutex<HashMap<String, &'static Mutex<()>>>> = OnceLock::new();
    let registry = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mutex: &'static Mutex<()> = {
        // フォールバック許可: ロックが毒された場合も更新自体は続ける（中身は () で不変）。
        let mut map = registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // キーは有限（設定キーの数だけ）なので、1つずつ leak して 'static にする。
        map.entry(key.to_string())
            .or_insert_with(|| Box::leak(Box::new(Mutex::new(()))))
    };
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 読み込めなかった状態ファイルを退避し、退避先を返す。
///
/// 破損したファイルをその場に残すと、次の保存（＝空の状態の書き戻し）で上書きされて
/// 原因調査もユーザーによる復旧もできなくなる。拡張子を変えて隣に残す。
pub fn quarantine_corrupt(path: &Path) -> Option<PathBuf> {
    if !path.exists() {
        return None;
    }
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let name = path.file_name()?.to_string_lossy().to_string();
    let target = path.with_file_name(format!("{name}.corrupt-{stamp}"));
    std::fs::rename(path, &target).ok()?;
    Some(target)
}

fn temp_path(path: &Path) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "state".to_string());
    // 同じディレクトリに置く（rename が原子的なのは同一ファイルシステム内だけ）。
    path.with_file_name(format!(".{name}.tmp-{}-{nanos}", std::process::id()))
}

/// 状態ファイルの書き込みが共通層を迂回していないかを走査するガード。
#[cfg(test)]
mod guard {
    use crate::guard_scan;

    /// `std::fs::write` の直接使用を許すファイルと、その理由。
    /// アプリ状態ファイルはここに足さず、`store::write_atomic` を使うこと。
    const RAW_WRITE_ALLOWED: [(&str, &str); 3] = [
        ("store.rs", "共通層の実装そのもの"),
        (
            "permissions.rs",
            "ユーザーが指定したファイルへの書き込みファサード（状態ファイルではない）",
        ),
        (
            "commands.rs",
            "取り込んだイラストの作業ディレクトリへの複製。状態ファイルではなく、\
             失っても取り込み直せる",
        ),
    ];

    #[test]
    fn 状態ファイルの書き込みは共通層を通す() {
        let mut offenders = Vec::new();
        for source in guard_scan::all_sources() {
            if RAW_WRITE_ALLOWED
                .iter()
                .any(|(allowed, _)| *allowed == source.name)
            {
                continue;
            }
            // コネクタが書くのは利用者向けの成果物であって、アプリの状態ファイルではない。
            // ここではなく `permissions::guard` のファサード規則の管轄。
            if source.is_stage() || source.is_output() {
                continue;
            }
            if source.body().contains("fs::write(") {
                offenders.push(source.path.display().to_string());
            }
        }
        assert!(
            offenders.is_empty(),
            "状態ファイルは crate::store::write_atomic / write_json_atomic を使ってください: {offenders:?}"
        );
    }

    /// 「壊れていたら空で続ける」フォールバックは、そのまま書き戻すとデータが全損する。
    ///
    /// 破損を空一覧・空ストアへ落とす箇所は、必ず (1) 破損ファイルを
    /// `store::quarantine_corrupt()` で退避するか、(2) `store::load_list()` で
    /// エラーとして返して書き戻しを止めるか、のどちらかを通すこと。
    #[test]
    fn 破損を空で置き換える箇所は退避かエラー返却を伴う() {
        let mut offenders = Vec::new();
        for source in guard_scan::all_sources() {
            if source.name == "store.rs" {
                continue; // 退避・読み取りの定義元
            }
            let body = source.body();
            let silently_empty = body.contains("from_str(&text).unwrap_or_default()")
                || body.contains("from_str(&raw).unwrap_or_default()");
            if !silently_empty {
                continue;
            }
            if body.contains("quarantine_corrupt") || body.contains("store::load_list") {
                continue;
            }
            offenders.push(source.path.display().to_string());
        }
        assert!(
            offenders.is_empty(),
            "破損時に空で続ける箇所は store::quarantine_corrupt での退避か \
             store::load_list でのエラー返却を伴わせてください\
             （そのまま書き戻すとユーザーのデータが恒久的に消えます）: {offenders:?}"
        );
    }

    /// 退避に失敗した場合の分岐は、呼び出し行ごとに確かめる。
    ///
    /// 「ファイル内のどこかに `ok_or_else(` があれば合格」にすると、無関係な別の
    /// `ok_or_else` で通ってしまう。認めるのは (1) 戻り値へ `ok_or_else(` を連ねる形、
    /// (2) `if let Some(...)` で受けて、その分岐から `return` か `bail!` で抜ける形、
    /// の2つだけ。`bail!` を認めるのは、それ自体が早期 return だから
    /// （退避できた側で `bail!` する実装まで落とすと、正しいコードを直せなくなる）。
    #[test]
    fn 破損ファイルの退避失敗時は空状態で続けない() {
        let mut offenders = Vec::new();
        for source in guard_scan::all_sources() {
            if source.name == "store.rs" {
                continue; // 退避の定義元
            }
            for (index, line) in source.body().lines().enumerate() {
                if !line.contains("quarantine_corrupt(") {
                    continue;
                }
                let chained = line.contains("ok_or_else(");
                let following: Vec<&str> = source.body().lines().skip(index).take(12).collect();
                let exits = following
                    .iter()
                    .any(|line| line.contains("return") || line.contains("bail!"));
                let branched = line.contains("if let Some(") && exits;
                if !chained && !branched {
                    offenders.push(format!("{}:{}", source.path.display(), index + 1));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "破損ファイルを退避できなければ空状態の保存へ進ませないでください: {offenders:?}"
        );
    }

    /// 状態保存先の取得失敗をカレントディレクトリや一時領域へ落とすと、保存成功に見えて
    /// 次回起動でデータが消える。
    #[test]
    fn 状態保存先を別フォルダへ黙って降格しない() {
        let mut offenders = Vec::new();
        for source in guard_scan::all_sources() {
            let body = source.body();
            if body.contains("app_data_dir()")
                && (body.contains("PathBuf::from(\".\")")
                    || body.contains("unwrap_or_else(|_| std::env::temp_dir())"))
            {
                offenders.push(source.path.display().to_string());
            }
        }
        assert!(
            offenders.is_empty(),
            "状態保存先を作業フォルダや一時領域へ黙って降格しないでください: {offenders:?}"
        );
    }

    /// 設定保存の失敗を無視すると、画面上の状態と次回起動時の状態が食い違う。
    #[test]
    fn 設定保存の結果を無視しない() {
        let offenders: Vec<String> = guard_scan::all_sources()
            .into_iter()
            .filter(|source| {
                source.body().lines().any(|line| {
                    let line = line.trim();
                    line.starts_with("let _ =") && line.contains("set_all(")
                })
            })
            .map(|source| source.path.display().to_string())
            .collect();
        assert!(
            offenders.is_empty(),
            "Config::set_all の失敗は伝播またはログ記録してください: {offenders:?}"
        );
    }

    /// 設定へJSON一覧を保存する箇所は、読み込みから書き戻しまでを名前付きロックで囲む。
    #[test]
    fn 設定一覧の書き戻しはロックの中で行う() {
        let mut offenders = Vec::new();
        for source in guard_scan::all_sources() {
            if source.name == "config.rs" {
                continue; // set_string の定義元
            }
            let body = source.body();
            if body.contains("set_string(") && !body.contains("lock_state(") {
                offenders.push(source.path.display().to_string());
            }
        }
        assert!(
            offenders.is_empty(),
            "JSON一覧を保存する箇所は store::lock_state() を取ってから更新してください\
             （並行して走る工程・音声処理同士で更新が消えます）: {offenders:?}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{lock_state, quarantine_corrupt, write_atomic, write_json_atomic};

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("picovtuber-store-{label}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn 上書きしても一時ファイルを残さない() {
        let dir = temp_dir("overwrite");
        let path = dir.join("state.json");
        write_atomic(&path, b"first").unwrap();
        write_atomic(&path, b"second").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
        let left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(
            left,
            vec!["state.json".to_string()],
            "一時ファイルが残っている"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn 親ディレクトリが無くても作成して書ける() {
        let dir = temp_dir("mkdir");
        let path = dir.join("nested").join("deep").join("state.json");
        write_json_atomic(&path, &serde_json::json!({ "a": 1 })).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"a\""));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// ロックが無いと、同じ一覧を並行更新した片方の追加が消える（lost update）。
    #[test]
    fn 並列更新でも追加が消えない() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Mutex};

        // 設定ストアの代わりに「読む→少し待つ→書き戻す」共有状態で再現する。
        let store = Arc::new(Mutex::new(Vec::<usize>::new()));
        let seq = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..8 {
            let store = Arc::clone(&store);
            let seq = Arc::clone(&seq);
            handles.push(std::thread::spawn(move || {
                let _guard = lock_state("PICOVTUBER_TEST_LIST");
                // 読む
                let mut items = store.lock().unwrap().clone();
                // 直す（ロックが無ければここで他スレッドと交差する）
                std::thread::sleep(std::time::Duration::from_millis(5));
                items.push(seq.fetch_add(1, Ordering::SeqCst));
                // 書き戻す
                *store.lock().unwrap() = items;
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(
            store.lock().unwrap().len(),
            8,
            "並列更新で追加が失われている"
        );
    }

    #[test]
    fn 破損ファイルは退避して次の保存で消えない() {
        let dir = temp_dir("quarantine");
        let path = dir.join("config.json");
        write_atomic(&path, "{ 壊れた".as_bytes()).unwrap();
        let saved = quarantine_corrupt(&path).expect("退避されるはず");
        assert!(saved.exists());
        assert!(!path.exists());
        assert_eq!(std::fs::read_to_string(&saved).unwrap(), "{ 壊れた");
        // 存在しないファイルには何もしない。
        assert!(quarantine_corrupt(&path).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
