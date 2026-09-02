// xtask - 開発・ビルド用のタスクランナー。
//
// Node を使わずに、UI の配信ビルド・同梱物の取得・残存プロセス回収・PR前の一括検証を
// 行う。`cargo xtask <サブコマンド>` で実行する（エイリアスは `.cargo/config.toml`）。
//
// ここに置くのは「開発者が繰り返す操作」だけ。製品の実行時に必要な処理はアプリ側へ置く
// （xtask は配布物に入らない）。

mod setup;
mod ui;
mod verify;
mod version;

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (command, rest) = args
        .split_first()
        .map(|(first, rest)| (first.as_str(), rest))
        .unwrap_or(("help", &[]));

    let result = match command {
        "build-ui" => ui::build_dist(&repo_root()),
        "dev" => dev(),
        "build" => build(),
        "setup" => setup::run(&repo_root(), rest),
        "verify" => verify::run(&repo_root()),
        "stop" => stop(),
        "bump-version" => version::bump(&repo_root(), rest),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        unknown => Err(anyhow::anyhow!(
            "不明なサブコマンドです: {unknown}\n`cargo xtask help` で一覧を出せます。"
        )),
    };

    if let Err(error) = result {
        eprintln!("エラー: {error:#}");
        std::process::exit(1);
    }
}

fn print_help() {
    println!(
        "cargo xtask <サブコマンド>

  build-ui                  ui/ を Tauri 配信用の ui-dist/ へビルドする
  dev                       build-ui のあと tauri dev で起動する
  build                     製品ビルド（src-tauri/target/release/bundle/）
  setup <対象>              同梱物を取得する（viewer / runtime / models / all）
  verify                    PR前の一括ローカル検証
  stop                      残存プロセスを回収する
  bump-version <種別>       版番号を同期更新する（major / minor / patch）

検証は軽い順に使うこと: cargo check → cargo test --lib → cargo xtask verify。
GUI 起動と製品ビルドは高コストなので最後の手段にする。"
    );
}

/// リポジトリルート（`xtask/` の親）。
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask の親")
        .to_path_buf()
}

fn dev() -> anyhow::Result<()> {
    let root = repo_root();
    // 配信フロントが古いまま起動すると、直したはずの画面が変わらない。毎回作り直す。
    ui::build_dist(&root)?;
    run_command(
        Command::new("cargo")
            .current_dir(&root)
            .args(["tauri", "dev"]),
        "cargo tauri dev",
    )
}

fn build() -> anyhow::Result<()> {
    let root = repo_root();
    ui::build_dist(&root)?;
    run_command(
        Command::new("cargo")
            .current_dir(&root)
            .args(["tauri", "build"]),
        "cargo tauri build",
    )
}

/// 開発中に起動したまま残ったプロセスを回収する。
///
/// **名前だけで一括終了しない。** `python` は他アプリも使っており、名前一致で殺すと
/// 稼働中のアプリを壊す。回収するのは PicoVTuber 本体と、コマンドラインが
/// PicoVTuber 管理下のパスを含む子プロセスだけにする。
fn stop() -> anyhow::Result<()> {
    let root = repo_root();
    let marker = root.to_string_lossy().replace('\\', "/");

    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("powershell");
        command.args([
            "-NoProfile",
            "-Command",
            &format!(
                "Get-CimInstance Win32_Process | Where-Object {{ $_.Name -eq 'picovtuber.exe' -or ($_.CommandLine -and $_.CommandLine.Replace('\\','/').Contains('{marker}')) }} | ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }}"
            ),
        ]);
        command
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut command = Command::new("pkill");
        // -f はコマンドライン全体を見る。PicoVTuber 管理下のパスを含むものだけが対象。
        command.args(["-f", &marker]);
        command
    };

    // 対象が1つも無ければ pkill は終了コード1を返す。これは失敗ではない。
    let status = command.status()?;
    if status.success() {
        println!("残存プロセスを回収しました。");
    } else {
        println!("回収対象のプロセスはありませんでした。");
    }
    Ok(())
}

/// コマンドを実行し、失敗したら理由付きで返す。
pub fn run_command(command: &mut Command, label: &str) -> anyhow::Result<()> {
    let status = command
        .status()
        .map_err(|error| anyhow::anyhow!("{label} を起動できません: {error}"))?;
    if !status.success() {
        anyhow::bail!("{label} が失敗しました（終了コード {:?}）", status.code());
    }
    Ok(())
}
