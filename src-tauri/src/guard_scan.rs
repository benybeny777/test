// guard_scan.rs - 設計ガードが走査するソース一式を1箇所で決める（テスト専用）。
//
// 各ガードがそれぞれ独自にディレクトリを歩くと、走査範囲がバラバラになる。範囲から
// 外れたファイルで同じ規則を破っても1件も検出されず、**ガードの穴はガードが無いのと
// 同じ**になる。だから「どこを見るか」はここだけで決め、すべてのガードがこれを使う。
//
// 新しいガードを書くときも自前でディレクトリを歩かないこと。歩いていないかは
// `guard_scan::guard` のメタガードが検出する。

use std::path::{Path, PathBuf};

/// 走査対象のソース1件。
pub struct Source {
    /// 表示用の短い名前（ファイル名）。
    pub name: String,
    /// リポジトリルートからの相対パス。
    pub path: PathBuf,
    pub text: String,
}

impl Source {
    /// 生成工程コネクタ（`src-tauri/src/pipeline/stages/*.rs`）か。
    pub fn is_stage(&self) -> bool {
        self.path.starts_with("src-tauri/src/pipeline/stages")
    }

    /// 配信出力コネクタ（`src-tauri/src/studio/outputs/*.rs`）か。
    pub fn is_output(&self) -> bool {
        self.path.starts_with("src-tauri/src/studio/outputs")
    }

    /// テストコード（`#[cfg(test)]` 以降）を落とした本体。
    ///
    /// テスト内のバイト列スライスやダミーの外部コマンドまで違反として拾わないため。
    pub fn body(&self) -> &str {
        match self.text.find("#[cfg(test)]") {
            Some(pos) => self.text.split_at(pos).0,
            None => &self.text,
        }
    }
}

/// リポジトリルート。ガードがリポジトリ内のファイルを見るときはこれを使うこと。
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri の親")
        .to_path_buf()
}

/// バックエンドと再利用コアのソース全部。ガードはこれを使うこと。
pub fn all_sources() -> Vec<Source> {
    let mut out = backend_sources();
    out.extend(core_sources());
    out
}

/// `src-tauri/src/**/*.rs` 一式。
pub fn backend_sources() -> Vec<Source> {
    let root = repo_root();
    let mut out = Vec::new();
    collect_rs(&root, &root.join("src-tauri/src"), &mut out);
    assert!(
        out.len() > 10,
        "バックエンドの読み込みに失敗している可能性: {}",
        out.len()
    );
    out
}

/// `crates/picovtuber-core/src/**/*.rs` 一式。
/// コアへ切り出したコードも、アプリ内コードと同じ設計ガードから外さない。
pub fn core_sources() -> Vec<Source> {
    let root = repo_root();
    let mut out = Vec::new();
    collect_rs(&root, &root.join("crates/picovtuber-core/src"), &mut out);
    assert!(
        out.len() >= 3,
        "再利用コアの読み込みに失敗している可能性: {}",
        out.len()
    );
    out
}

/// 生成工程コネクタだけ。工程固有の規則を検査するガードが使う。
pub fn stage_sources() -> Vec<Source> {
    backend_sources()
        .into_iter()
        .filter(Source::is_stage)
        .collect()
}

/// 配信出力コネクタだけ。
pub fn output_sources() -> Vec<Source> {
    backend_sources()
        .into_iter()
        .filter(Source::is_output)
        .collect()
}

fn collect_rs(root: &Path, dir: &Path, out: &mut Vec<Source>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    // 走査順を固定して、検出結果の並びが実行のたびに変わらないようにする。
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_rs(root, &path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            out.push(Source {
                name: path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default(),
                path: path.strip_prefix(root).unwrap_or(&path).to_path_buf(),
                text,
            });
        }
    }
}

/// メタガード: 各ガードが走査範囲を自前で決めていないこと。
#[cfg(test)]
mod guard {
    #[test]
    fn 再利用コアへ製品依存を持ち込まない() {
        const FORBIDDEN: &[&str] = &["tauri", "PICOVTUBER_", "crate::config"];
        let mut violations = Vec::new();
        for source in super::core_sources() {
            for dependency in FORBIDDEN {
                if source.body().contains(dependency) {
                    violations.push(format!("{}: {dependency}", source.path.display()));
                }
            }
        }
        assert!(
            violations.is_empty(),
            "再利用コアへ PicoVTuber 製品固有の依存を持ち込まないでください: {violations:?}"
        );
    }

    #[test]
    fn ガードは走査範囲を自前で決めない() {
        let mut violations = Vec::new();
        for source in super::all_sources() {
            if source.name == "guard_scan.rs" {
                continue; // 走査範囲の定義元
            }
            // `mod guard` を持つファイル＝設計ガード。共通の走査を使っているか。
            if !source.text.contains("mod guard {") {
                continue;
            }
            if !source.text.contains("guard_scan::") {
                violations.push(source.path.display().to_string());
            }
        }
        assert!(
            violations.is_empty(),
            "設計ガードは crate::guard_scan の走査を使ってください\
             （自前でディレクトリを歩くと走査範囲がずれ、規則違反を見逃します）: {violations:?}"
        );
    }
}
