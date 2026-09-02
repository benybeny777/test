// tools/process.rs - 外部プロセス実行の共通層。
//
// ML工程は PicoVTuber 管理下の Python を子プロセスとして呼ぶ。各所で `Command::new` を
// 直接使うと、次の3つが必ずどこかで抜ける。
//   1. Windows のコンソール窓抑止（CREATE_NO_WINDOW）— 抜けると生成のたびに黒い窓が明滅する
//   2. 中断時の子プロセス回収（kill_on_drop）— 抜けると打ち切っても Python が動き続ける
//   3. 出力回収の待ち切り — 抜けるとタイムアウトが効かず、上限を超えても戻らない
//
// だから起動は `command()`、実行は `output()` / `output_text()` を通す。
// `tokio::time::timeout(..., command.output())` で自前に包むのは禁止。上限は付くが、
// 打ち切りで届くのは `kill_on_drop` による直下の子までで、**さらに子を産む Python
// ランチャや、起動直後に本体を立て直す実行ファイルが残る**。共通層は
// `terminate_process_tree()` でツリーごと回収する。
//
// 直接起動と自前タイムアウトは `tools::process::guard` のテストが検出する。

use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

use crate::text;

/// 非同期実行用のコマンドを作る。**外部プロセスの起動は必ずここを通す。**
pub fn command(program: &str) -> Command {
    let mut command = Command::new(program);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // 打ち切り時に直下の子を確実に落とす（孫は terminate_process_tree が見る）。
        .kill_on_drop(true);
    apply_hide_window(&mut command);
    command
}

/// 常駐プロセス用の同期コマンド。
///
/// future の終了後も動かし続けるもの（配信中の仮想カメラブリッジなど）に使う。
/// 窓抑止だけを共通化し、stdio と待ち方は用途ごとに指定する。
pub fn command_sync(program: &str) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    apply_hide_window_sync(&mut command);
    command
}

/// 実行結果。標準出力・標準エラーはUTF-8として安全にデコード済み。
#[derive(Debug)]
pub struct Output {
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn success(&self) -> bool {
        self.status == Some(0)
    }
}

/// タイムアウト付きで実行し、出力を回収する。
///
/// 上限を超えたらプロセスツリーごと落としてから戻る。**待ち切らずに戻らない**
/// （戻ってしまうと、呼び出し側は終わったつもりで次の工程へ進み、実際には裏で
/// 前の工程が同じファイルを書き続ける）。
pub async fn output(mut command: Command, timeout: Duration) -> anyhow::Result<Output> {
    let mut child = command
        .spawn()
        .map_err(|error| anyhow::anyhow!("外部プログラムを起動できません: {error}"))?;
    let pid = child.id();

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let collect_out = tokio::spawn(read_all(stdout));
    let collect_err = tokio::spawn(read_all(stderr));

    let waited = tokio::time::timeout(timeout, child.wait()).await;
    let status = match waited {
        Ok(status) => status
            .map_err(|error| anyhow::anyhow!("外部プログラムの終了を待てません: {error}"))?
            .code(),
        Err(_) => {
            // 上限超過。直下の子だけでなくツリーごと回収する。
            if let Some(pid) = pid {
                terminate_process_tree(pid);
            }
            child.start_kill().ok();
            child.wait().await.ok();
            let stdout = join_capture(collect_out).await;
            let stderr = join_capture(collect_err).await;
            anyhow::bail!(
                "外部プログラムが {} 秒以内に終わりませんでした（中断しました）: {}",
                timeout.as_secs(),
                text::preview(&format!("{stdout}{stderr}"), 200)
            );
        }
    };

    Ok(Output {
        status,
        stdout: join_capture(collect_out).await,
        stderr: join_capture(collect_err).await,
    })
}

/// 実行して標準出力だけを取る。失敗時は標準エラーを含む理由を返す。
pub async fn output_text(command: Command, timeout: Duration) -> anyhow::Result<String> {
    let result = output(command, timeout).await?;
    if !result.success() {
        anyhow::bail!(
            "外部プログラムが失敗しました（終了コード {:?}）: {}",
            result.status,
            text::preview(&result.stderr, 400)
        );
    }
    Ok(result.stdout)
}

/// 出力回収タスクを**待ち切って**結果を得る。
///
/// 自分で `child` を持つ長時間処理（進捗表示や中断が要るもの）も、出力回収の待ちは
/// これを通すこと。待ち方だけ自前で書くと、打ち切り後に回収タスクを放置してしまう。
pub async fn join_capture(handle: tokio::task::JoinHandle<String>) -> String {
    // フォールバック許可: 回収タスクが落ちても、出力が取れないだけで処理は続けられる。
    handle.await.unwrap_or_default()
}

/// プロセスツリーごと終了させる。
///
/// Python ランチャのように「起動した子がさらに子を産む」形では、直下の子を kill しても
/// 孫が残ってパイプの書き込み端を握り続ける。すると終端（EOF）が来ず、出力回収が
/// 孫の寿命まで戻らない。
pub fn terminate_process_tree(pid: u32) {
    #[cfg(target_os = "windows")]
    {
        // taskkill の /T がツリー、/F が強制。ここでの失敗は握りつぶさず記録もしないが、
        // 直後の start_kill で直下の子は必ず落とすため処理は前へ進む。
        let _ = command_sync("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(target_os = "windows"))]
    {
        // 負のPIDはプロセスグループ全体。`kill` コマンドを介さず送る。
        let _ = command_sync("pkill")
            .args(["-TERM", "-P", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// 子プロセスの出力を最後まで読み、UTF-8として安全にデコードする。
///
/// チャンク境界は文字の途中に落ちるので、`text::Utf8Stream` を通す。
async fn read_all<R>(reader: Option<R>) -> String
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    use tokio::io::AsyncReadExt;
    let Some(mut reader) = reader else {
        return String::new();
    };
    let mut stream = text::Utf8Stream::default();
    let mut out = String::new();
    let mut buffer = [0_u8; 4096];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => return out,
            Ok(count) => out.push_str(&stream.push(buffer.split_at(count).0)),
            // フォールバック許可: 読み取りが切れても、そこまでの出力は診断に使える。
            Err(_) => return out,
        }
    }
}

#[cfg(target_os = "windows")]
fn apply_hide_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
fn apply_hide_window(_command: &mut Command) {}

#[cfg(target_os = "windows")]
fn apply_hide_window_sync(command: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
fn apply_hide_window_sync(_command: &mut std::process::Command) {}

/// 外部プロセス起動が共通層を迂回していないかを走査するガード。
#[cfg(test)]
mod guard {
    use crate::guard_scan;

    #[test]
    fn 外部プロセスの起動は共通層を通す() {
        let mut offenders = Vec::new();
        for source in guard_scan::all_sources() {
            if source.name == "process.rs" {
                continue; // 共通層の実装そのもの
            }
            let body = source.body();
            if body.contains("Command::new(") {
                offenders.push(source.path.display().to_string());
            }
        }
        assert!(
            offenders.is_empty(),
            "外部プロセスは crate::tools::process::command を使ってください\
             （直接起動は窓抑止・子プロセス回収・出力の待ち切りが抜けます）: {offenders:?}"
        );
    }

    #[test]
    fn 窓抑止を各所へ写さない() {
        let mut offenders = Vec::new();
        for source in guard_scan::all_sources() {
            if source.name == "process.rs" {
                continue; // 定義元
            }
            if source.body().contains("CREATE_NO_WINDOW") {
                offenders.push(source.path.display().to_string());
            }
        }
        assert!(
            offenders.is_empty(),
            "コンソール窓の抑止は共通層に集約してください（写すと抜けたファイルで窓が明滅します）: {offenders:?}"
        );
    }

    /// 一度きりの実行を自前で組み立てると、出力回収の待ち方だけが抜ける。
    #[test]
    fn 実行の打ち切りを自前で包まない() {
        let mut offenders = Vec::new();
        for source in guard_scan::all_sources() {
            if source.name == "process.rs" {
                continue; // 定義元
            }
            for (index, line) in source.body().lines().enumerate() {
                if !line.contains("timeout(") {
                    continue;
                }
                if line.contains(".output()") || line.contains(".wait()") {
                    offenders.push(format!("{}:{}", source.path.display(), index + 1));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "外部プロセスの打ち切りは process::output / join_capture を通してください\
             （自前の timeout は孫プロセスを残し、上限まで戻らなくなります）: {offenders:?}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{command, output, output_text};
    use std::time::Duration;

    #[tokio::test]
    async fn 標準出力を回収できる() {
        let mut cmd = command(if cfg!(windows) { "cmd" } else { "sh" });
        if cfg!(windows) {
            cmd.args(["/C", "echo picovtuber"]);
        } else {
            cmd.args(["-c", "echo picovtuber"]);
        }
        let text = output_text(cmd, Duration::from_secs(10)).await.unwrap();
        assert!(text.contains("picovtuber"), "{text}");
    }

    #[tokio::test]
    async fn 失敗は理由付きで返る() {
        let mut cmd = command(if cfg!(windows) { "cmd" } else { "sh" });
        if cfg!(windows) {
            cmd.args(["/C", "echo こわれた 1>&2 & exit 3"]);
        } else {
            cmd.args(["-c", "echo こわれた >&2; exit 3"]);
        }
        let result = output(cmd, Duration::from_secs(10)).await.unwrap();
        assert_eq!(result.status, Some(3));
        assert!(!result.success());
        assert!(result.stderr.contains("こわれた"), "{}", result.stderr);
    }

    /// 上限を超えたら中断して戻ること。戻らないと、呼び出し側は終わったつもりで
    /// 次の工程へ進み、裏で前の工程が同じファイルを書き続ける。
    #[tokio::test]
    async fn 上限を超えたら中断して戻る() {
        let mut cmd = command(if cfg!(windows) { "cmd" } else { "sh" });
        if cfg!(windows) {
            cmd.args(["/C", "ping -n 30 127.0.0.1 > nul"]);
        } else {
            cmd.args(["-c", "sleep 30"]);
        }
        let started = std::time::Instant::now();
        let error = output(cmd, Duration::from_millis(300))
            .await
            .expect_err("上限超過はエラーで返る");
        assert!(error.to_string().contains("中断"), "{error}");
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "上限を過ぎても戻ってこない: {:?}",
            started.elapsed()
        );
    }

    /// 起動できないプログラムは、実行できたことにしない。
    #[tokio::test]
    async fn 存在しないプログラムは起動失敗として返る() {
        let cmd = command("picovtuber-このプログラムは存在しない");
        let error = output(cmd, Duration::from_secs(5))
            .await
            .expect_err("起動失敗はエラーで返る");
        assert!(error.to_string().contains("起動できません"), "{error}");
    }
}
