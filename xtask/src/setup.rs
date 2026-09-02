// setup.rs - 同梱物（three.js / three-vrm、Python ランタイム、生成モデルの重み）の取得。
//
// **第三者資産を PicoVTuber のリリースへ再配布しない**（ライセンス条件が個別に違うため）。
// 利用者のPCが公式配布元から直接取得する。出所と条件は THIRD_PARTY_NOTICES.md が正本。
//
// 大容量の取得は**明示操作のときだけ**行う。起動・通常ビルド・引数省略へ混ぜない
// （AGENTS.md の規則）。取得前に OS・CPU・GPU を判定し、使えない構成のものは取らない。

use std::path::Path;

/// three.js / three-vrm の取得先と、置き場所。
///
/// 版を固定するのは、配信ビューの描画が上流の変更で黙って変わらないようにするため。
/// 更新は `audit-picovtuber-dependencies` の手順で行う。
const VIEWER_ASSETS: [(&str, &str); 3] = [
    (
        "three.module.js",
        "https://cdn.jsdelivr.net/npm/three@0.180.0/build/three.module.js",
    ),
    (
        "GLTFLoader.js",
        "https://cdn.jsdelivr.net/npm/three@0.180.0/examples/jsm/loaders/GLTFLoader.js",
    ),
    (
        "three-vrm.module.js",
        "https://cdn.jsdelivr.net/npm/@pixiv/three-vrm@3.4.4/lib/three-vrm.module.js",
    ),
];

pub fn run(root: &Path, args: &[String]) -> anyhow::Result<()> {
    let target = args.first().map(String::as_str).unwrap_or("");
    match target {
        "viewer" => viewer(root),
        "runtime" => runtime(),
        "models" => models(),
        "all" => {
            viewer(root)?;
            runtime()?;
            models()
        }
        "" => anyhow::bail!(
            "取得対象を指定してください: viewer / runtime / models / all\n\
             （引数を省略したときに大容量の取得を始めない決まりです）"
        ),
        unknown => anyhow::bail!("不明な取得対象です: {unknown}"),
    }
}

/// three.js / three-vrm を `ui/vendor/` へ取得する。
fn viewer(root: &Path) -> anyhow::Result<()> {
    let dir = root.join("ui").join("vendor");
    std::fs::create_dir_all(&dir)?;
    for (name, url) in VIEWER_ASSETS {
        let target = dir.join(name);
        if target.exists() {
            println!("すでに取得済み: ui/vendor/{name}");
            continue;
        }
        println!("取得中: {url}");
        let body = reqwest::blocking::get(url)
            .and_then(|response| response.error_for_status())
            .and_then(|response| response.bytes())
            .map_err(|error| anyhow::anyhow!("{name} を取得できません: {error}"))?;
        let text = String::from_utf8(body.to_vec())
            .map_err(|error| anyhow::anyhow!("{name} を文字列として読めません: {error}"))?;
        let rewritten = rewrite_bare_specifiers(&text);
        // 途中で落ちたファイルを残さないよう、書き終えてから rename で確定する。
        let part = dir.join(format!("{name}.part"));
        std::fs::write(&part, rewritten.as_bytes())?;
        std::fs::rename(&part, &target)?;
        println!("  ui/vendor/{name} ({} MB)", megabytes(rewritten.len()));
    }
    println!("配信ビューのライブラリが揃いました。");
    Ok(())
}

/// `from 'three'` のような裸のモジュール指定子を、隣のファイルへの相対パスへ書き換える。
///
/// 裸の指定子はブラウザが解決できず、import map が要る。ところが import map は
/// インラインの `<script>` なので、アプリの CSP（`script-src 'self'`）では読めない。
/// CSP を緩めるよりは、取得時に1回だけ書き換えるほうが安全に済む。
fn rewrite_bare_specifiers(source: &str) -> String {
    source
        .replace("from 'three'", "from './three.module.js'")
        .replace("from \"three\"", "from \"./three.module.js\"")
        .replace(
            "from 'three/examples/jsm/loaders/GLTFLoader.js'",
            "from './GLTFLoader.js'",
        )
        .replace(
            "from \"three/examples/jsm/loaders/GLTFLoader.js\"",
            "from \"./GLTFLoader.js\"",
        )
}

#[cfg(test)]
mod tests {
    use super::rewrite_bare_specifiers;

    /// 書き換えが漏れると、配信ビューが「モジュールを解決できません」で真っ白になる。
    #[test]
    fn 裸の指定子を相対パスへ書き換える() {
        let source = "import * as THREE from 'three';\nimport { GLTFLoader } from \"three\";";
        let rewritten = rewrite_bare_specifiers(source);
        assert!(rewritten.contains("'./three.module.js'"));
        assert!(rewritten.contains("\"./three.module.js\""));
        assert!(!rewritten.contains("from 'three'"));
    }

    /// 相対パスの import まで壊さないこと。
    #[test]
    fn 相対パスの指定子には触らない() {
        let source = "import { x } from './three.module.js';";
        assert_eq!(rewrite_bare_specifiers(source), source);
    }
}

/// Python ランタイムの用意。
fn runtime() -> anyhow::Result<()> {
    // 採用する Python の配布形式（埋め込み版か、venv か）と、CUDA 対応の組合せが
    // 決まるまでは案内だけにする。中途半端に取得すると、動かない環境へ数GBを置く。
    anyhow::bail!(
        "Python ランタイムの自動取得はまだ有効にしていません。\n\
         採用する配布形式と CUDA / PyTorch の対応組合せを決めてから実装します\n\
         （着手条件は docs/TASKS.md）。\n\
         いまは Python 3.12 を用意し、設定の PICOVTUBER_RUNTIME_PYTHON でその場所を指してください。"
    )
}

/// 生成モデルの重みの取得。
fn models() -> anyhow::Result<()> {
    // ライセンスと再配布条件を確認する前に、既定の取得対象へ入れない（AGENTS.md）。
    anyhow::bail!(
        "生成モデルの重みの自動取得はまだ有効にしていません。\n\
         採用するモデルのライセンスと再配布条件を確認してから、取得先とハッシュを\n\
         THIRD_PARTY_NOTICES.md へ記載したうえで実装します（着手条件は docs/TASKS.md）。\n\
         いまは重みを手元に置き、設定の PICOVTUBER_MODELS_DIR でその場所を指してください。"
    )
}

/// 容量は十進の MB で表示する（bytes や MiB を混ぜない。AGENTS.md の規則）。
fn megabytes(bytes: usize) -> String {
    format!("{:.1}", bytes as f64 / 1_000_000.0)
}
