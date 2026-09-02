// permissions.rs - 全コネクタ共通のファイル許可の正本。
//
// 生成工程は利用者のイラストを読み、成果物を書く。どのコネクタも自前で許可判定を書くと、
// シンボリックリンクの実体検査・追加許可ルート・拒否パス判定のどれかが必ず抜ける。
// だから判定はここ1箇所に集約し、通過したパスを `SafePath` 型で表す。
//
// `SafePath` の生成経路は `resolve()` / `SafePath::join()` / `SafePath::sibling()` /
// `SafePath::app_owned()` だけ。派生パスも `..` による脱出を防ぐため再検証する。
// 自前の許可判定と `resolve` 漏れは `permissions::guard` のテストが検出する。

use std::path::{Component, Path, PathBuf};

use crate::config::Config;

/// 許可判定を通過したパス。
///
/// `AsRef<Path>` を実装しているので `std::fs` へそのまま渡せてしまう。書き込みは
/// 必ず `permissions::fs::write()` を使うこと（親フォルダ自動作成と統一エラー文言を
/// 素通りさせないため）。`permissions::guard` が直接使用を検出する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafePath(PathBuf);

impl SafePath {
    /// アプリ自身が決めた内部パス（作業ディレクトリなど、利用者入力を含まないもの）。
    ///
    /// 利用者から受け取った文字列にこれを使わないこと。使うと許可判定を丸ごと迂回する。
    pub fn app_owned(path: PathBuf) -> Self {
        SafePath(path)
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }

    /// 配下の相対パスを足す。`..` での脱出は拒否する。
    pub fn join(&self, relative: impl AsRef<Path>) -> Result<SafePath, String> {
        let relative = relative.as_ref();
        if relative.is_absolute() {
            return Err(format!(
                "配下のパスには相対パスを指定してください: {}",
                relative.display()
            ));
        }
        if relative
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(format!(
                "上位フォルダへ抜けるパスは使えません: {}",
                relative.display()
            ));
        }
        Ok(SafePath(self.0.join(relative)))
    }

    /// 同じフォルダ内の別名ファイル。
    pub fn sibling(&self, file_name: &str) -> Result<SafePath, String> {
        if file_name.is_empty() || file_name.contains(['/', '\\']) {
            return Err(format!("ファイル名として使えません: {file_name}"));
        }
        let parent = self
            .0
            .parent()
            .ok_or_else(|| format!("親フォルダを特定できません: {}", self.0.display()))?;
        Ok(SafePath(parent.join(file_name)))
    }
}

impl AsRef<Path> for SafePath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl std::fmt::Display for SafePath {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.0.display())
    }
}

/// OSがどの設定にも関わらず常に拒否する場所。
///
/// システム領域を書き換えられる経路をアプリが持たないようにする。
#[cfg(target_os = "windows")]
const ALWAYS_DENIED: [&str; 3] = [
    "C:\\Windows",
    "C:\\Program Files",
    "C:\\Program Files (x86)",
];
#[cfg(not(target_os = "windows"))]
const ALWAYS_DENIED: [&str; 4] = ["/bin", "/sbin", "/usr/bin", "/etc"];

/// 利用者由来のパスを許可判定に通し、`SafePath` へ変換する。
///
/// 判定順は「拒否 → 許可」。拒否を先に見るのは、許可ルートの下に拒否したい場所を
/// 置けるようにするため（逆順だと許可が拒否を上書きしてしまう）。
pub fn resolve(cfg: &Config, input: &str) -> Result<SafePath, String> {
    if input.trim().is_empty() {
        return Err("パスが空です".to_string());
    }
    let requested = PathBuf::from(input);
    // 実体を見る。シンボリックリンクで許可ルートの外を指していても、ここで判明する。
    // 未作成のファイル（これから書く成果物）は canonicalize できないので、存在する
    // 祖先まで遡って解決し、残りを足し戻す。
    let resolved = canonicalize_existing_ancestor(&requested)?;

    for denied in ALWAYS_DENIED {
        if starts_with_path(&resolved, Path::new(denied)) {
            return Err(format!(
                "システムが使う場所は操作できません: {}",
                resolved.display()
            ));
        }
    }
    for denied in list_paths(cfg, "PICOVTUBER_DENIED_PATHS") {
        if starts_with_path(&resolved, &denied) {
            return Err(format!(
                "設定で拒否されている場所です: {}",
                resolved.display()
            ));
        }
    }

    let mut allowed = list_paths(cfg, "PICOVTUBER_ALLOWED_PATHS");
    let root = cfg.get("PICOVTUBER_ALLOWED_ROOT", "");
    if !root.trim().is_empty() {
        allowed.push(PathBuf::from(root));
    }
    if allowed.is_empty() {
        return Err(
            "操作を許可するフォルダが未設定です。設定画面の「許可するフォルダ」で\
             PICOVTUBER_ALLOWED_ROOT を指定してください。"
                .to_string(),
        );
    }
    let permitted = allowed.iter().any(|allowed| {
        // 許可ルート自体もリンクの場合があるので、同じ手順で実体へ寄せてから比べる。
        let allowed = canonicalize_existing_ancestor(allowed).unwrap_or_else(|_| allowed.clone());
        starts_with_path(&resolved, &allowed)
    });
    if !permitted {
        return Err(format!(
            "許可されたフォルダの外です: {}（設定画面の「許可するフォルダ」を確認してください）",
            resolved.display()
        ));
    }
    Ok(SafePath(resolved))
}

/// 設定に複数パスを持つキーを読む。区切りはOS標準のパス区切り。
fn list_paths(cfg: &Config, key: &str) -> Vec<PathBuf> {
    let raw = cfg.get(key, "");
    std::env::split_paths(&raw)
        .filter(|path| !path.as_os_str().is_empty())
        .collect()
}

/// 存在する祖先まで `canonicalize` し、残りの相対部分を足し戻す。
///
/// これから作るファイルのパスは `canonicalize` できない。祖先で解決することで、
/// 途中にリンクがあっても実体で判定できる。`..` は解決前に潰す。
fn canonicalize_existing_ancestor(path: &Path) -> Result<PathBuf, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("現在のフォルダを取得できません: {error}"))?
            .join(path)
    };

    let mut existing = absolute.as_path();
    let mut tail = PathBuf::new();
    loop {
        if existing.exists() {
            break;
        }
        let Some(parent) = existing.parent() else {
            return Err(format!("パスを解決できません: {}", absolute.display()));
        };
        let name = existing
            .file_name()
            .ok_or_else(|| format!("パスを解決できません: {}", absolute.display()))?;
        tail = Path::new(name).join(&tail);
        existing = parent;
    }

    let mut resolved = existing
        .canonicalize()
        .map_err(|error| format!("パスを解決できません: {} ({error})", existing.display()))?;
    // 残りの相対部分に `..` が混ざっていたら、リンク解決の後で脱出できてしまう。
    for component in tail.components() {
        match component {
            Component::ParentDir => {
                return Err(format!(
                    "上位フォルダへ抜けるパスは使えません: {}",
                    absolute.display()
                ))
            }
            Component::CurDir => {}
            other => resolved.push(other.as_os_str()),
        }
    }
    Ok(resolved)
}

/// パスの前方一致。文字列ではなくコンポーネント単位で比べる。
///
/// 文字列比較だと `/home/user/pics2` が `/home/user/pics` の配下と誤判定される。
fn starts_with_path(path: &Path, prefix: &Path) -> bool {
    path.starts_with(prefix)
}

/// `SafePath` 専用のファイル操作ファサード。
///
/// 親フォルダの自動作成と、利用者へそのまま見せられるエラー文言をここへ集約する。
pub mod fs {
    use super::SafePath;

    pub fn read(path: &SafePath) -> Result<Vec<u8>, String> {
        std::fs::read(path.as_path()).map_err(|error| format!("読み込めません: {path} ({error})"))
    }

    pub fn read_to_string(path: &SafePath) -> Result<String, String> {
        std::fs::read_to_string(path.as_path())
            .map_err(|error| format!("読み込めません: {path} ({error})"))
    }

    pub fn write(path: &SafePath, bytes: &[u8]) -> Result<(), String> {
        if let Some(parent) = path.as_path().parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!("保存先を作成できません: {} ({error})", parent.display())
            })?;
        }
        std::fs::write(path.as_path(), bytes)
            .map_err(|error| format!("書き込めません: {path} ({error})"))
    }

    pub fn create_dir_all(path: &SafePath) -> Result<(), String> {
        std::fs::create_dir_all(path.as_path())
            .map_err(|error| format!("フォルダを作成できません: {path} ({error})"))
    }

    pub fn exists(path: &SafePath) -> bool {
        path.as_path().exists()
    }
}

/// 許可判定の迂回を検出するガード。
#[cfg(test)]
mod guard {
    use crate::guard_scan;

    /// コネクタが自前で許可判定を書いていないこと。
    ///
    /// 自前実装はリンク実体検査・追加許可ルート・拒否パス判定のどれかが必ず抜ける。
    #[test]
    fn コネクタは自前の許可判定を持たない() {
        let mut offenders = Vec::new();
        for source in guard_scan::all_sources() {
            if source.name == "permissions.rs" {
                continue; // 判定の定義元
            }
            let body = source.body();
            // 独自のルート設定キーを作っていないか。
            if body.contains("PICOVTUBER_")
                && (body.contains("_ROOT\"") || body.contains("_PATHS\""))
                && !body.contains("PICOVTUBER_ALLOWED_ROOT")
                && !body.contains("PICOVTUBER_ALLOWED_PATHS")
                && !body.contains("PICOVTUBER_DENIED_PATHS")
            {
                offenders.push(format!("{}: 独自のルート設定キー", source.path.display()));
            }
            // 許可判定らしき自前実装。
            if body.contains("fn is_allowed") || body.contains("fn check_permission") {
                offenders.push(format!("{}: 自前の許可判定", source.path.display()));
            }
        }
        assert!(
            offenders.is_empty(),
            "ファイル許可は crate::permissions::resolve を使ってください\
             （自前実装はリンク実体検査や拒否パス判定が抜けます）: {offenders:?}"
        );
    }

    /// `SafePath` へ `std::fs::write` を直接使っていないこと。
    ///
    /// `SafePath` は `AsRef<Path>` なので直接書けてしまい、親フォルダ自動作成と
    /// 統一エラー文言を素通りする。
    #[test]
    fn safepathへの書き込みはファサードを通す() {
        let mut offenders = Vec::new();
        for source in guard_scan::all_sources() {
            if source.name == "permissions.rs" || source.name == "store.rs" {
                continue; // ファサードと状態ストアの定義元
            }
            for (index, line) in source.body().lines().enumerate() {
                if line.contains("fs::write(") && line.contains("safe") {
                    offenders.push(format!("{}:{}", source.path.display(), index + 1));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "SafePath への書き込みは permissions::fs::write を使ってください: {offenders:?}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve, SafePath};
    use crate::config::Config;

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("picovtuber-perm-{label}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn config_with_root(root: &std::path::Path) -> Config {
        let cfg = Config::new();
        cfg.set("PICOVTUBER_ALLOWED_ROOT", &root.display().to_string());
        cfg
    }

    #[test]
    fn 許可ルートが未設定なら理由付きで断る() {
        let cfg = Config::new();
        let error = resolve(&cfg, "/tmp/x").expect_err("未設定では通さない");
        assert!(error.contains("PICOVTUBER_ALLOWED_ROOT"), "{error}");
    }

    #[test]
    fn 許可ルートの内側だけ通す() {
        let dir = temp_dir("inside");
        let cfg = config_with_root(&dir);

        let inside = dir.join("input.png");
        std::fs::write(&inside, b"x").unwrap();
        assert!(resolve(&cfg, &inside.display().to_string()).is_ok());

        // まだ存在しない成果物のパスも通る（祖先まで遡って解決するため）。
        let future = dir.join("pack").join("model.vrm");
        assert!(resolve(&cfg, &future.display().to_string()).is_ok());

        // 外は通さない。
        let outside = temp_dir("outside").join("secret.txt");
        let error =
            resolve(&cfg, &outside.display().to_string()).expect_err("許可ルートの外は通さない");
        assert!(error.contains("許可されたフォルダの外"), "{error}");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 文字列の前方一致で判定すると、`pics2` が `pics` の配下と誤判定される。
    #[test]
    fn 名前が前方一致するだけの兄弟フォルダを通さない() {
        let base = temp_dir("sibling");
        let allowed = base.join("pics");
        let neighbour = base.join("pics2");
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::create_dir_all(&neighbour).unwrap();
        let cfg = config_with_root(&allowed);

        let target = neighbour.join("a.png");
        std::fs::write(&target, b"x").unwrap();
        assert!(
            resolve(&cfg, &target.display().to_string()).is_err(),
            "pics2 が pics の配下と誤判定されている"
        );
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn 上位へ抜けるパスを拒否する() {
        let base = temp_dir("escape");
        let allowed = base.join("work");
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::write(base.join("outside.txt"), b"x").unwrap();
        let cfg = config_with_root(&allowed);

        let escaping = format!("{}/../outside.txt", allowed.display());
        assert!(resolve(&cfg, &escaping).is_err(), ".. での脱出を許している");
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn 拒否設定は許可より先に効く() {
        let dir = temp_dir("denied");
        let secret = dir.join("secret");
        std::fs::create_dir_all(&secret).unwrap();
        let cfg = config_with_root(&dir);
        cfg.set("PICOVTUBER_DENIED_PATHS", &secret.display().to_string());

        let target = secret.join("a.png");
        std::fs::write(&target, b"x").unwrap();
        let error = resolve(&cfg, &target.display().to_string())
            .expect_err("許可ルート内でも拒否設定が優先される");
        assert!(error.contains("拒否"), "{error}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn 派生パスは再検証して脱出させない() {
        let dir = temp_dir("derive");
        let safe = SafePath::app_owned(dir.clone());
        assert!(safe.join("stage/out.png").is_ok());
        assert!(safe.join("../outside.png").is_err(), ".. を許している");
        assert!(safe.join("/etc/passwd").is_err(), "絶対パスを許している");
        assert!(safe.sibling("other.json").is_ok());
        assert!(safe.sibling("../other.json").is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ファサードは保存先の親フォルダを作る() {
        let dir = temp_dir("facade");
        let safe = SafePath::app_owned(dir.join("a").join("b").join("c.txt"));
        super::fs::write(&safe, b"hello").unwrap();
        assert_eq!(super::fs::read_to_string(&safe).unwrap(), "hello");
        assert!(super::fs::exists(&safe));
        std::fs::remove_dir_all(&dir).ok();
    }
}
