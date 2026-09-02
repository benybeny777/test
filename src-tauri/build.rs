// build.rs - Tauri ビルド準備 ＋ コネクタの自動 mod 生成。
//
// src/pipeline/stages/*.rs と src/studio/outputs/*.rs を走査し、各ファイルの
// `pub mod <名前>;` 宣言を OUT_DIR へ生成する。pipeline/mod.rs・studio/mod.rs が
// これを include! することで、「決まった場所にファイルを置くだけ」でコネクタが追加できる
// （中央レジストリの編集不要）という思想を Rust でも維持する。

use std::{
    env, fs,
    path::{Component, Path, PathBuf},
};

/// Rust の識別子として使える名前か（コネクタ名はそのまま `mod` 名になる）。
fn is_valid_module_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_lowercase() || first == '_')
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn relative_path(from_dir: &Path, to_file: &Path) -> PathBuf {
    let from_components: Vec<Component<'_>> = from_dir.components().collect();
    let to_components: Vec<Component<'_>> = to_file.components().collect();
    let mut common_len = 0;

    while common_len < from_components.len()
        && common_len < to_components.len()
        && from_components[common_len] == to_components[common_len]
    {
        common_len += 1;
    }

    if common_len == 0 {
        return to_file.to_path_buf();
    }

    let mut rel = PathBuf::new();
    for _ in common_len..from_components.len() {
        rel.push("..");
    }
    for component in &to_components[common_len..] {
        rel.push(component.as_os_str());
    }
    rel
}

/// `dir` 直下の `*.rs`（`mod.rs` を除く）を `#[path] pub mod` として宣言する。
fn generate_mods(dir: &str, out_name: &str) {
    let out_dir = env::var("OUT_DIR").unwrap();
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let mut decls = String::new();
    if let Ok(entries) = fs::read_dir(dir) {
        let mut names: Vec<String> = entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let path = entry.path();
                let stem = path.file_stem()?.to_str()?.to_string();
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                if ext == "rs" && stem != "mod" {
                    Some(stem)
                } else {
                    None
                }
            })
            .collect();
        names.sort();
        for name in &names {
            // ファイル名はそのまま `pub mod <名前>;` になる。ハイフン等が入っていると
            // 生成コードが構文エラーになり、原因の分からないビルド失敗になるため、
            // ここで何が悪いのかを明示して止める。
            assert!(
                is_valid_module_name(name),
                "コネクタのファイル名 \"{name}.rs\" は Rust のモジュール名にできません。\
                 英小文字・数字・アンダースコアだけを使ってください（例: my_stage.rs）。"
            );
            // include! 経由だと mod 解決位置が OUT_DIR 基準になるため、生成先から
            // 実ファイルへの相対パスを #[path] で明示する。
            let source_path = Path::new(&manifest_dir)
                .join(dir)
                .join(format!("{name}.rs"));
            let rel = relative_path(Path::new(&out_dir), &source_path)
                .to_string_lossy()
                .replace('\\', "/");
            decls.push_str(&format!("#[path = \"{rel}\"]\npub mod {name};\n"));
        }
    }
    fs::write(Path::new(&out_dir).join(out_name), decls).unwrap();
    println!("cargo:rerun-if-changed={dir}");
}

fn main() {
    generate_mods("src/pipeline/stages", "stage_mods.rs");
    generate_mods("src/studio/outputs", "output_mods.rs");
    tauri_build::build();
}
