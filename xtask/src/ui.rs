// ui.rs - `ui/` を Tauri 配信用の `ui-dist/` へビルドする。
//
// 正本は `ui/`。`ui-dist/` は生成物なので Git 管理外で、`tauri.conf.json` の
// `frontendDist` が指す。**clone 直後は存在しない**ため、`tauri::generate_context!` が
// コンパイル時に落ちる。だから dev / build の前と、Claude Code のセッション開始フックで
// 必ず作り直す。
//
// 差分同期にしているのは、毎回全消しすると大きな素材のコピーで起動が遅くなるから。
// 同期元に無くなったファイルは生成先からも消す（古い画面が残って「直したのに変わらない」
// という混乱を生まないため）。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// `ui/` に置いてよい拡張子。
///
/// 開発用のメモやスクリーンショットが配布物へ混ざらないよう、allowlist で固定する。
const ALLOWED_EXTENSIONS: [&str; 9] = [
    "html", "css", "js", "mjs", "json", "png", "svg", "woff2", "wasm",
];

pub fn build_dist(root: &Path) -> anyhow::Result<()> {
    let source = root.join("ui");
    let target = root.join("ui-dist");
    if !source.is_dir() {
        anyhow::bail!("配信元の ui/ がありません: {}", source.display());
    }
    std::fs::create_dir_all(&target)?;

    let mut copied = HashSet::new();
    sync_dir(&source, &target, &source, &mut copied)?;
    remove_stale(&target, &target, &copied)?;
    println!(
        "ui/ を ui-dist/ へ配信ビルドしました（{} ファイル）",
        copied.len()
    );
    Ok(())
}

fn sync_dir(
    dir: &Path,
    target_root: &Path,
    source_root: &Path,
    copied: &mut HashSet<PathBuf>,
) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            sync_dir(&path, target_root, source_root, copied)?;
            continue;
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if !ALLOWED_EXTENSIONS.contains(&extension) {
            // 拡張子が想定外のものは黙って飛ばさず、何を飛ばしたか伝える。
            println!(
                "  配信対象外のため飛ばしました: {}",
                path.strip_prefix(source_root).unwrap_or(&path).display()
            );
            continue;
        }
        let relative = path.strip_prefix(source_root)?.to_path_buf();
        let destination = target_root.join(&relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // サイズと更新時刻が同じなら飛ばす（毎回全部コピーすると起動が遅くなる）。
        if !needs_copy(&path, &destination)? {
            copied.insert(relative);
            continue;
        }
        std::fs::copy(&path, &destination)?;
        copied.insert(relative);
    }
    Ok(())
}

fn needs_copy(source: &Path, destination: &Path) -> anyhow::Result<bool> {
    let Ok(target_meta) = std::fs::metadata(destination) else {
        return Ok(true);
    };
    let source_meta = std::fs::metadata(source)?;
    if source_meta.len() != target_meta.len() {
        return Ok(true);
    }
    match (source_meta.modified(), target_meta.modified()) {
        (Ok(source_time), Ok(target_time)) => Ok(source_time > target_time),
        // 更新時刻を取れない環境ではコピーする（古いまま残すより安全）。
        _ => Ok(true),
    }
}

/// 同期元に無くなったファイルを生成先から消す。
fn remove_stale(dir: &Path, target_root: &Path, copied: &HashSet<PathBuf>) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            remove_stale(&path, target_root, copied)?;
            // 空になったフォルダも片付ける（残すと配布物に空ディレクトリが混ざる）。
            if std::fs::read_dir(&path)?.next().is_none() {
                std::fs::remove_dir(&path)?;
            }
            continue;
        }
        let relative = path.strip_prefix(target_root)?.to_path_buf();
        if !copied.contains(&relative) {
            std::fs::remove_file(&path)?;
            println!("  古いファイルを削除: {}", relative.display());
        }
    }
    Ok(())
}
