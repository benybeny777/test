// verify.rs - PR前の一括ローカル検証。
//
// **途中で失敗しても最後まで実行する。** 1つ目で止めると、直すたびに全部を回し直す
// ことになり、結局「あとでまとめて」になる。末尾に失敗とスキップをまとめて出す。
//
// 日常の小修正で毎回これを回す意味ではない。軽い順に
// `cargo check` → `cargo test --lib` → ここ、の順で使う。

use std::path::Path;
use std::process::Command;

/// 検証1件の結果。
enum Outcome {
    Passed,
    Failed(String),
    /// 実行できなかった（環境に必要なものが無い）。**成功と混ぜない。**
    Skipped(String),
}

pub fn run(root: &Path) -> anyhow::Result<()> {
    let mut results: Vec<(&str, Outcome)> = Vec::new();

    results.push((
        "本体の書式",
        cargo(
            root,
            &["fmt", "--manifest-path", "src-tauri/Cargo.toml", "--check"],
        ),
    ));
    results.push((
        "xtask の書式",
        cargo(
            root,
            &["fmt", "--manifest-path", "xtask/Cargo.toml", "--check"],
        ),
    ));
    results.push((
        "再利用コアの書式",
        cargo(
            root,
            &[
                "fmt",
                "--manifest-path",
                "crates/picovtuber-core/Cargo.toml",
                "--check",
            ],
        ),
    ));
    results.push((
        "静的解析",
        cargo(
            root,
            &[
                "clippy",
                "--manifest-path",
                "src-tauri/Cargo.toml",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
        ),
    ));
    results.push((
        "テストと設計ガード",
        cargo(
            root,
            &["test", "--manifest-path", "src-tauri/Cargo.toml", "--lib"],
        ),
    ));
    results.push((
        "再利用コアのテスト",
        cargo(
            root,
            &[
                "test",
                "--manifest-path",
                "crates/picovtuber-core/Cargo.toml",
            ],
        ),
    ));
    results.push(("共通スキルと自動認識ファイルの同期", skills(root)));
    results.push(("JavaScript の構文", javascript(root)));
    results.push(("Git 差分の行末", git_diff_check(root)));

    println!("\n=== 検証結果 ===");
    let mut failed = 0;
    let mut skipped = 0;
    for (label, outcome) in &results {
        match outcome {
            Outcome::Passed => println!("  OK    {label}"),
            Outcome::Failed(reason) => {
                failed += 1;
                println!("  失敗  {label}: {reason}");
            }
            Outcome::Skipped(reason) => {
                skipped += 1;
                println!("  スキップ {label}: {reason}");
            }
        }
    }
    if skipped > 0 {
        println!("\nスキップが {skipped} 件あります。**成功と混ぜず**、PR本文へ未確認として書いてください。");
    }
    if failed > 0 {
        anyhow::bail!("{failed} 件失敗しました");
    }
    println!("\nすべて通りました。");
    Ok(())
}

fn cargo(root: &Path, args: &[&str]) -> Outcome {
    match Command::new("cargo").current_dir(root).args(args).status() {
        Ok(status) if status.success() => Outcome::Passed,
        Ok(status) => Outcome::Failed(format!("終了コード {:?}", status.code())),
        Err(error) => Outcome::Skipped(format!("cargo を起動できません: {error}")),
    }
}

/// `.agents/skills/` の正本と `.claude/skills/` の自動認識ファイルが対応していること。
///
/// 名前がずれると、Claude Code は入口を見つけられず手順なしで作業を始める。
/// 自動認識ファイルへ手順を複製していないことも見る（複製すると正本が2つになる）。
fn skills(root: &Path) -> Outcome {
    let canonical = root.join(".agents").join("skills");
    let entries = root.join(".claude").join("skills");
    let Ok(dirs) = std::fs::read_dir(&canonical) else {
        return Outcome::Skipped(format!("{} を読めません", canonical.display()));
    };

    let mut problems = Vec::new();
    let mut names = Vec::new();
    for entry in dirs.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        names.push(name.clone());
        if !entry.path().join("SKILL.md").is_file() {
            problems.push(format!("{name}: 正本の SKILL.md がありません"));
            continue;
        }
        let entry_file = entries.join(&name).join("SKILL.md");
        let Ok(text) = std::fs::read_to_string(&entry_file) else {
            problems.push(format!("{name}: .claude/skills 側の入口がありません"));
            continue;
        };
        if !text.contains(&format!(".agents/skills/{name}/SKILL.md")) {
            problems.push(format!("{name}: 入口が正本を指していません"));
        }
        // 入口は「正本を読め」と言うだけ。手順を複製すると正本が2つになる。
        if text.lines().count() > 12 {
            problems.push(format!("{name}: 入口に手順が複製されています"));
        }
    }

    // 逆向き（入口だけ残っている）も見る。
    if let Ok(existing) = std::fs::read_dir(&entries) {
        for entry in existing.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if !names.contains(&name) {
                problems.push(format!("{name}: 正本が無いのに入口だけ残っています"));
            }
        }
    }

    if problems.is_empty() {
        Outcome::Passed
    } else {
        Outcome::Failed(problems.join(" / "))
    }
}

/// 追跡している JavaScript の構文検査。`node` が無ければスキップ（成功にしない）。
fn javascript(root: &Path) -> Outcome {
    let files = tracked_files(root, &[".js", ".mjs"]);
    if files.is_empty() {
        return Outcome::Passed;
    }
    for file in files {
        match Command::new("node")
            .current_dir(root)
            .args(["--check", &file])
            .status()
        {
            Ok(status) if status.success() => {}
            Ok(_) => return Outcome::Failed(format!("{file} の構文エラー")),
            Err(_) => {
                return Outcome::Skipped(
                    "node が無いため構文検査を実行していません（未確認として扱ってください）"
                        .to_string(),
                )
            }
        }
    }
    Outcome::Passed
}

fn git_diff_check(root: &Path) -> Outcome {
    match Command::new("git")
        .current_dir(root)
        .args(["diff", "--check"])
        .status()
    {
        Ok(status) if status.success() => Outcome::Passed,
        Ok(_) => Outcome::Failed("行末の空白や衝突マーカーがあります".to_string()),
        Err(error) => Outcome::Skipped(format!("git を起動できません: {error}")),
    }
}

/// 検査対象の列挙は `git ls-files` に集約する（未追跡の生成物を巻き込まない）。
fn tracked_files(root: &Path, extensions: &[&str]) -> Vec<String> {
    let Ok(output) = Command::new("git")
        .current_dir(root)
        .args(["ls-files"])
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| extensions.iter().any(|extension| line.ends_with(extension)))
        .map(str::to_string)
        .collect()
}
