// version.rs - 版番号の同期更新。
//
// 版番号は3か所（アプリ Cargo / xtask Cargo / Tauri 設定）にある。手で書き換えると
// 必ずどれかが取り残され、「表示されている版」と「配布物の版」がずれる。
//
// `minor` と `major` は**利用者から明示指示があったときだけ**上げる。エージェントが
// 機能内容から独断で上げない（AGENTS.md の規則）。

use std::path::Path;

pub fn bump(root: &Path, args: &[String]) -> anyhow::Result<()> {
    let kind = args.first().map(String::as_str).unwrap_or("");
    let current = read_version(root)?;
    let next = next_version(&current, kind)?;

    write_cargo_version(&root.join("src-tauri").join("Cargo.toml"), &next)?;
    write_cargo_version(&root.join("xtask").join("Cargo.toml"), &next)?;
    write_tauri_version(&root.join("src-tauri").join("tauri.conf.json"), &next)?;

    println!("版番号を {current} から {next} へ更新しました（3か所を同期）。");
    println!("判定した種別と理由を PR 本文へ残してください。");
    Ok(())
}

/// アプリ Cargo の `version` を正本として読む。
fn read_version(root: &Path) -> anyhow::Result<String> {
    let path = root.join("src-tauri").join("Cargo.toml");
    let text = std::fs::read_to_string(&path)?;
    text.lines()
        .find_map(|line| {
            let line = line.trim();
            let rest = line
                .strip_prefix("version")?
                .trim_start()
                .strip_prefix('=')?;
            Some(rest.trim().trim_matches('"').to_string())
        })
        .ok_or_else(|| anyhow::anyhow!("{} から版番号を読めません", path.display()))
}

/// 次の版番号。
fn next_version(current: &str, kind: &str) -> anyhow::Result<String> {
    let parts: Vec<u32> = current
        .split('.')
        .map(|part| part.parse::<u32>())
        .collect::<Result<_, _>>()
        .map_err(|_| anyhow::anyhow!("版番号が SemVer ではありません: {current}"))?;
    let [major, minor, patch] = parts.as_slice() else {
        anyhow::bail!("版番号が SemVer ではありません: {current}");
    };
    Ok(match kind {
        "major" => format!("{}.0.0", major + 1),
        "minor" => format!("{major}.{}.0", minor + 1),
        "patch" => format!("{major}.{minor}.{}", patch + 1),
        "" => anyhow::bail!("種別を指定してください: major / minor / patch"),
        unknown => anyhow::bail!("不明な種別です: {unknown}"),
    })
}

fn write_cargo_version(path: &Path, next: &str) -> anyhow::Result<()> {
    let text = std::fs::read_to_string(path)?;
    let mut out = String::new();
    let mut replaced = false;
    for line in text.lines() {
        // `[package]` 直後の version だけを置き換える。依存の version は触らない。
        if !replaced && line.trim_start().starts_with("version") && line.contains('=') {
            out.push_str(&format!("version = \"{next}\"\n"));
            replaced = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if !replaced {
        anyhow::bail!("{} に version 行がありません", path.display());
    }
    std::fs::write(path, out)?;
    Ok(())
}

fn write_tauri_version(path: &Path, next: &str) -> anyhow::Result<()> {
    let text = std::fs::read_to_string(path)?;
    let mut value: serde_json::Value = serde_json::from_str(&text)?;
    value["version"] = serde_json::Value::String(next.to_string());
    std::fs::write(path, format!("{}\n", serde_json::to_string_pretty(&value)?))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::next_version;

    #[test]
    fn 種別ごとに正しく繰り上げる() {
        assert_eq!(next_version("1.2.3", "patch").unwrap(), "1.2.4");
        assert_eq!(next_version("1.2.3", "minor").unwrap(), "1.3.0");
        assert_eq!(next_version("1.2.3", "major").unwrap(), "2.0.0");
    }

    /// 種別を省略したときに勝手に上げない（どれを上げるかは利用者の判断）。
    #[test]
    fn 種別の省略と誤りを断る() {
        assert!(next_version("1.2.3", "").is_err());
        assert!(next_version("1.2.3", "たくさん").is_err());
        assert!(next_version("1.2", "patch").is_err());
        assert!(next_version("v1.2.3", "patch").is_err());
    }
}
