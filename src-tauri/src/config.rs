// config.rs - 設定の正本（永続ストア）。
//
// 値の優先順位: 永続ファイル（app_config_dir/config.json）> 環境変数 > ハードコード既定値。
// コネクタは get() で都度読むため、設定画面での保存はアプリ再起動なしに反映される。

use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::{Map, Value};

pub struct Config {
    path: Mutex<Option<PathBuf>>,
    /// 原本を読めず・退避できず、保存先を持てないまま起動した理由。
    /// 保存要求が来たときに「なぜ保存できないのか」を利用者へそのまま返すため覚えておく。
    read_only_reason: Mutex<Option<String>>,
    values: Mutex<Map<String, Value>>,
    /// 「スナップショットを取る → ファイルへ書く」を直列化するためのロック。
    /// これが無いと、2つの保存が同時に走ったとき **あとから取ったスナップショットが
    /// 先に書かれ、古い方が後から上書きする**ことがあり、メモリ上には残っているのに
    /// ファイルからは片方の変更が消える（再起動で設定が戻る）。
    write: Mutex<()>,
}

impl Config {
    pub fn new() -> Self {
        Config {
            path: Mutex::new(None),
            read_only_reason: Mutex::new(None),
            values: Mutex::new(Map::new()),
            write: Mutex::new(()),
        }
    }

    /// 起動時にファイルパスを設定し、あれば読み込む。
    ///
    /// 破損などで読めなかった場合は警告文字列を返す（呼び出し側がログへ記録する。
    /// ファイルが無いだけの初回起動は正常なので `None`）。原本を安全に退避できない
    /// 場合は保存先を設定せず、読み取り専用の既定値で起動する。
    pub fn load(&self, file: PathBuf) -> Result<Option<String>, String> {
        let mut warning = None;
        match std::fs::read_to_string(&file) {
            Ok(text) => match serde_json::from_str::<Value>(&text) {
                Ok(Value::Object(map)) => *self.values.lock().unwrap() = map,
                Ok(_) | Err(_) => {
                    // フォールバック許可: 破損時は空設定で起動（初回起動を成立させるため）。
                    // ただし「設定が全部消えた」ように見える事象の原因調査ができるよう必ず記録する。
                    // 破損ファイルはその場に残すと次の保存（＝空設定の書き戻し）で消えるので退避する。
                    if let Some(saved) = crate::store::quarantine_corrupt(&file) {
                        warning = Some(format!(
                            "設定ファイルが JSON として読めないため空設定で起動します: {}（破損ファイルは {} へ退避しました）",
                            file.display(),
                            saved.display()
                        ));
                    } else {
                        return Ok(Some(self.start_read_only(format!(
                            "壊れた設定ファイルを退避できないため、設定を保存しない読み取り専用状態で起動します: {}",
                            file.display()
                        ))));
                    }
                }
            },
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                return Ok(Some(self.start_read_only(format!(
                    "設定ファイルを読み込めないため、設定を保存しない読み取り専用状態で起動します: {} ({error})",
                    file.display()
                ))));
            }
            Err(_) => {} // ファイルが無いのは初回起動の正常系
        }
        *self.path.lock().unwrap() = Some(file);
        Ok(warning)
    }

    /// 保存先を持たない読み取り専用状態に入り、その理由を覚えて同じ文言を返す。
    ///
    /// 覚えておかないと、設定画面の保存が「config.load() が未実行です」という実装都合の
    /// 文言になり、利用者には原因も対象ファイルも分からない。
    pub fn start_read_only(&self, reason: String) -> String {
        *self.read_only_reason.lock().unwrap() = Some(reason.clone());
        reason
    }

    /// PicoVTuber が管理する設定ディレクトリを返す。
    ///
    /// config.json と同じ寿命で管理するファイルは、カレントディレクトリや一時領域へ
    /// 降格せずこの場所だけへ保存する。
    pub fn config_dir(&self) -> Result<PathBuf, String> {
        let path = self.path.lock().unwrap();
        let Some(file) = path.as_ref() else {
            let reason = self.read_only_reason.lock().unwrap().clone();
            return Err(reason.unwrap_or_else(|| {
                "PicoVTuber の設定保存先がまだ初期化されていません。".to_string()
            }));
        };
        file.parent()
            .map(PathBuf::from)
            .ok_or_else(|| "PicoVTuber の設定ディレクトリを特定できません。".to_string())
    }

    /// 文字列設定を取得。空文字は未設定とみなし env → 既定値へフォールバック。
    pub fn get(&self, key: &str, fallback: &str) -> String {
        if let Some(value) = self.values.lock().unwrap().get(key) {
            if let Some(text) = value.as_str() {
                if !text.is_empty() {
                    // シークレットはOSの資格情報保護へ預けてあるため読み取り時に復号する
                    // （非保護値は素通り）。
                    let decrypted = crate::secret::decrypt(text);
                    // 復号できないまま保護形式が残るのは、別PC・別OSユーザー・別OSの
                    // `config.json` を持ち込んだ場合。**この端末では使えない値**なので
                    // 未設定として扱い、設定画面の入力案内へ乗せる。保存値は上書きしない。
                    if crate::secret::is_encrypted(&decrypted) {
                        return fallback.to_string();
                    }
                    return decrypted;
                }
            } else if !value.is_null() {
                return value.to_string();
            }
        }
        if let Ok(env_value) = std::env::var(key) {
            if !env_value.is_empty() {
                return env_value;
            }
        }
        fallback.to_string()
    }

    /// 真偽値設定を取得。
    pub fn get_bool(&self, key: &str, fallback: bool) -> bool {
        if let Some(value) = self.values.lock().unwrap().get(key) {
            if let Some(flag) = value.as_bool() {
                return flag;
            }
            if let Some(text) = value.as_str() {
                return text == "true" || text == "1";
            }
        }
        if let Ok(env_value) = std::env::var(key) {
            return env_value == "true" || env_value == "1";
        }
        fallback
    }

    /// 数値設定を取得。読めない値は既定値へ落とす（設定の書き間違いで起動できなくしない）。
    ///
    /// 値域は呼び出し側が決める。ここで一律に丸めると、閾値と係数で意味の違う制限に
    /// なってしまうため。
    pub fn get_f32(&self, key: &str, fallback: f32) -> f32 {
        let raw = self.get(key, "");
        if raw.is_empty() {
            return fallback;
        }
        raw.parse::<f32>()
            .ok()
            .filter(|value| value.is_finite())
            .unwrap_or(fallback)
    }

    /// 整数設定を取得。読めない値は既定値へ落とす。
    pub fn get_u32(&self, key: &str, fallback: u32) -> u32 {
        let raw = self.get(key, "");
        if raw.is_empty() {
            return fallback;
        }
        raw.parse::<u32>().unwrap_or(fallback)
    }

    /// 永続設定または環境変数に空でない値があるか。
    pub fn has_value(&self, key: &str) -> bool {
        if let Some(value) = self.values.lock().unwrap().get(key) {
            if let Some(text) = value.as_str() {
                return !text.is_empty();
            }
            return !value.is_null();
        }
        std::env::var(key)
            .map(|value| !value.is_empty())
            .unwrap_or(false)
    }

    /// 永続化されている生の値一式（設定画面の初期表示用）。
    pub fn get_all(&self) -> Map<String, Value> {
        self.values.lock().unwrap().clone()
    }

    /// 設定を部分更新して永続化する。
    ///
    /// 内容が1つも変わらない場合はファイルへ書かない。設定は「読み込み → 全体を書き戻す」
    /// 更新なので、定期処理から同じ値で呼ばれるたびに config.json 全体を書き直してしまい、
    /// 無意味なディスク書き込みが延々と続く。
    pub fn set_all(&self, patch: Map<String, Value>) -> anyhow::Result<()> {
        self.set_all_with_writer(patch, crate::store::write_json_atomic)
    }

    fn set_all_with_writer(
        &self,
        patch: Map<String, Value>,
        write_snapshot: impl FnOnce(&std::path::Path, &Value) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        // スナップショットの採取から書き込みまでを直列化する（並び替わりを防ぐ）。
        // フォールバック許可: 毒された場合も保存自体は続ける（中身は () で不変）。
        let _write = self
            .write
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let path = self.path.lock().unwrap().clone();
        let Some(path) = path else {
            // 読み取り専用で起動した場合は、その理由（対象ファイルを含む）をそのまま返す。
            let reason = self.read_only_reason.lock().unwrap().clone();
            return Err(anyhow::anyhow!(
                reason.unwrap_or_else(|| "config.load() が未実行です".to_string())
            ));
        };
        let next = {
            let values = self.values.lock().unwrap();
            let mut next = values.clone();
            let mut changed = false;
            for (key, value) in patch {
                if next.get(&key) != Some(&value) {
                    next.insert(key, value);
                    changed = true;
                }
            }
            if !changed {
                return Ok(());
            }
            next
        };
        let snapshot = Value::Object(next);
        // 書き込み中に落ちても設定が全損しないよう、共通層の原子的な書き込みを使う。
        // ディスクI/O中は values ロックを外し、コネクタ等の get() を止めない。
        write_snapshot(&path, &snapshot)?;
        let Value::Object(next) = snapshot else {
            unreachable!("設定スナップショットは必ずJSONオブジェクト")
        };
        *self.values.lock().unwrap() = next;
        Ok(())
    }

    /// 単一の設定を永続化する。
    ///
    /// JSON一覧を持つキーを更新する場合は、呼び出し側が `store::lock_state()` を
    /// 取ってから「読む → 直す → 書き戻す」を行うこと（`store::guard` が検査する）。
    pub fn set_string(&self, key: &str, value: String) -> anyhow::Result<()> {
        let mut patch = Map::new();
        patch.insert(key.to_string(), Value::String(value));
        self.set_all(patch)
    }

    /// テスト専用: 永続化せずインメモリの設定値だけを書き換える
    /// （環境変数と違いテスト間で競合しない）。
    #[cfg(test)]
    pub fn set(&self, key: &str, value: &str) {
        self.values
            .lock()
            .unwrap()
            .insert(key.to_string(), Value::String(value.to_string()));
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("picovtuber-config-{label}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 別PC・別OSユーザーの config.json を持ち込むと復号できない。
    /// その暗号文をそのまま値として使うと、原因の分からない失敗になる。
    #[test]
    fn 復号できない暗号文は未設定として扱う() {
        let cfg = Config::new();
        cfg.set("PICOVTUBER_TEST_SECRET", "enc:dpapi:v1:こわれたデータ");
        assert_eq!(cfg.get("PICOVTUBER_TEST_SECRET", ""), "");
        assert_eq!(cfg.get("PICOVTUBER_TEST_SECRET", "既定値"), "既定値");
        // 平文はそのまま読める。
        cfg.set("PICOVTUBER_TEST_SECRET", "plain-value");
        assert_eq!(cfg.get("PICOVTUBER_TEST_SECRET", ""), "plain-value");
    }

    #[test]
    fn 保存した設定を読み直せる() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("config.json");

        let cfg = Config::new();
        assert!(
            cfg.load(path.clone()).unwrap().is_none(),
            "初回起動は警告なし"
        );
        cfg.set_string("PICOVTUBER_TEST_KEY", "値".to_string())
            .unwrap();

        let reloaded = Config::new();
        assert!(reloaded.load(path.clone()).unwrap().is_none());
        assert_eq!(reloaded.get("PICOVTUBER_TEST_KEY", ""), "値");

        // 原子的な書き込みなので、保存後に一時ファイルが残らない。
        let left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(left, vec!["config.json".to_string()]);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 設定の書き間違いでアプリが起動できなくなると、直す手段まで失われる。
    #[test]
    fn 数値設定は読めない値を既定へ落とす() {
        let cfg = Config::new();
        assert_eq!(cfg.get_f32("PICOVTUBER_TEST_NUM", 0.5), 0.5);
        cfg.set("PICOVTUBER_TEST_NUM", "0.25");
        assert_eq!(cfg.get_f32("PICOVTUBER_TEST_NUM", 0.5), 0.25);
        cfg.set("PICOVTUBER_TEST_NUM", "たくさん");
        assert_eq!(cfg.get_f32("PICOVTUBER_TEST_NUM", 0.5), 0.5);
        // NaN / 無限大は「数値として読めた」ことにしない（比較が素通りするため）。
        cfg.set("PICOVTUBER_TEST_NUM", "NaN");
        assert_eq!(cfg.get_f32("PICOVTUBER_TEST_NUM", 0.5), 0.5);
        cfg.set("PICOVTUBER_TEST_NUM", "inf");
        assert_eq!(cfg.get_f32("PICOVTUBER_TEST_NUM", 0.5), 0.5);

        cfg.set("PICOVTUBER_TEST_INT", "48000");
        assert_eq!(cfg.get_u32("PICOVTUBER_TEST_INT", 16000), 48000);
        cfg.set("PICOVTUBER_TEST_INT", "-1");
        assert_eq!(cfg.get_u32("PICOVTUBER_TEST_INT", 16000), 16000);
    }

    /// 設定は毎回「全体を書き戻す」ので、同じ値での保存を弾かないと
    /// 定期処理から呼ばれるたびに無意味なディスク書き込みが続く。
    #[test]
    fn 内容が変わらない保存はファイルへ書かない() {
        let dir = temp_dir("nochange");
        let path = dir.join("config.json");

        let cfg = Config::new();
        cfg.load(path.clone()).unwrap();
        cfg.set_string("PICOVTUBER_TEST_KEY", "値".to_string())
            .unwrap();
        assert!(path.exists());

        // ファイルを消してから同じ値で保存する。書き込みが起きれば再生成される。
        std::fs::remove_file(&path).unwrap();
        cfg.set_string("PICOVTUBER_TEST_KEY", "値".to_string())
            .unwrap();
        assert!(!path.exists(), "同じ値なのに書き込んでいる");

        // 値が変われば当然書く。
        cfg.set_string("PICOVTUBER_TEST_KEY", "別の値".to_string())
            .unwrap();
        assert!(path.exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 保存が同時に走っても、ファイルには全部の変更が入った状態が残ること。
    #[test]
    fn 同時保存でも変更が消えない() {
        use std::sync::Arc;
        let dir = temp_dir("concurrent");
        let path = dir.join("config.json");
        let cfg = Arc::new(Config::new());
        cfg.load(path.clone()).unwrap();

        let mut handles = Vec::new();
        for index in 0..8 {
            let cfg = Arc::clone(&cfg);
            handles.push(std::thread::spawn(move || {
                cfg.set_string(&format!("PICOVTUBER_TEST_KEY_{index}"), index.to_string())
                    .unwrap();
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }

        let reloaded = Config::new();
        reloaded.load(path.clone()).unwrap();
        for index in 0..8 {
            assert_eq!(
                reloaded.get(&format!("PICOVTUBER_TEST_KEY_{index}"), ""),
                index.to_string(),
                "同時保存で {index} 番目の変更がファイルから消えている"
            );
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn 設定ファイルの保存失敗時は実行中の値を変えない() {
        let dir = temp_dir("write-failure");
        let path = dir.join("config.json");
        let cfg = Config::new();
        cfg.load(path).unwrap();
        cfg.set_string("PICOVTUBER_TEST_KEY", "保存前".to_string())
            .unwrap();

        let mut patch = serde_json::Map::new();
        patch.insert(
            "PICOVTUBER_TEST_KEY".to_string(),
            serde_json::Value::String("保存後".to_string()),
        );
        let error = cfg
            .set_all_with_writer(patch, |_, _| anyhow::bail!("書き込み失敗"))
            .expect_err("保存失敗を返す");
        assert!(error.to_string().contains("書き込み失敗"));
        assert_eq!(cfg.get("PICOVTUBER_TEST_KEY", ""), "保存前");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn 破損した設定は退避されて上書きで失われない() {
        let dir = temp_dir("corrupt");
        let path = dir.join("config.json");
        // 書き込み途中で落ちた状態を模す（末尾が切れた JSON）。
        crate::store::write_atomic(&path, "{\n  \"PICOVTUBER_TEST_KEY\": \"値\"".as_bytes())
            .unwrap();

        let cfg = Config::new();
        let warning = cfg
            .load(path.clone())
            .expect("破損時も退避できる")
            .expect("破損は警告する");
        assert!(warning.contains("退避"), "退避先を案内する: {warning}");

        // 空設定で起動したあと保存しても、破損ファイルの中身は隣に残っている。
        cfg.set_string("PICOVTUBER_OTHER", "x".to_string()).unwrap();
        let saved: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.contains(".corrupt-"))
            .collect();
        assert_eq!(saved.len(), 1, "破損ファイルが退避されていない");
        let text = std::fs::read_to_string(dir.join(&saved[0])).unwrap();
        assert!(text.contains("PICOVTUBER_TEST_KEY"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn 読み込めない設定でも保存を止めて起動を続ける() {
        let dir = temp_dir("unreadable");
        // ファイルの代わりにディレクトリを渡し、読み取り不能をOS共通で再現する。
        let cfg = Config::new();
        let warning = cfg
            .load(dir.clone())
            .expect("読み取り失敗だけで起動を止めない")
            .expect("読み取り専用状態を警告する");
        assert!(warning.contains("読み取り専用"), "warning: {warning}");
        let error = cfg
            .set_string("PICOVTUBER_OTHER", "x".to_string())
            .expect_err("原本を読めない状態では保存しない");
        // 保存できない理由は、実装都合ではなく起動時と同じ文言（対象パス付き）で返す。
        assert!(error.to_string().contains("読み取り専用"), "error: {error}");
        assert!(
            error.to_string().contains(&dir.display().to_string()),
            "error: {error}"
        );
        assert!(dir.is_dir(), "元の対象を上書きしていない");
        std::fs::remove_dir_all(&dir).ok();
    }
}
