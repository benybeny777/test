use std::{
    ffi::OsString,
    io::{BufRead, BufReader, Read},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SidecarError {
    #[error("サイドカーを起動できません: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("サイドカーI/Oに失敗しました: {0}")]
    Io(#[from] std::io::Error),
    #[error("サイドカーが不正なJSON Linesを返しました: {0}")]
    Json(#[from] serde_json::Error),
    #[error("サイドカーを中断しました")]
    Cancelled,
    #[error("サイドカーが失敗しました ({status}): {stderr}")]
    Failed { status: String, stderr: String },
}

pub struct SidecarProcess {
    child: Option<Child>,
}

impl SidecarProcess {
    pub fn spawn(
        program: &Path,
        arguments: &[OsString],
        working_directory: &Path,
    ) -> Result<Self, SidecarError> {
        let child = Command::new(program)
            .args(arguments)
            .current_dir(working_directory)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(SidecarError::Spawn)?;
        Ok(Self { child: Some(child) })
    }

    pub fn relay_json_lines(
        mut self,
        cancelled: Arc<AtomicBool>,
        mut relay: impl FnMut(Value),
    ) -> Result<(), SidecarError> {
        let child = self.child.as_mut().expect("起動済みプロセスが必要です");
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| SidecarError::Io(std::io::Error::other("stdoutを取得できません")))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| SidecarError::Io(std::io::Error::other("stderrを取得できません")))?;
        let (sender, receiver) = mpsc::channel();
        let reader = thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let error_reader = thread::spawn(move || {
            let mut error_text = String::new();
            let mut stderr = stderr;
            stderr.read_to_string(&mut error_text).map(|_| error_text)
        });
        loop {
            if cancelled.load(Ordering::Relaxed) {
                child.kill()?;
                child.wait()?;
                self.child = None;
                let _ = reader.join();
                let _ = error_reader.join();
                return Err(SidecarError::Cancelled);
            }
            match receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(line) => relay(serde_json::from_str(&line?)?),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if child.try_wait()?.is_some() {
                        break;
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        let _ = reader.join();
        let status = child.wait()?;
        let error_text = error_reader.join().map_err(|_| {
            SidecarError::Io(std::io::Error::other("stderr読取スレッドが停止しました"))
        })??;
        self.child = None;
        if status.success() {
            Ok(())
        } else {
            Err(SidecarError::Failed {
                status: status.to_string(),
                stderr: error_text,
            })
        }
    }
}

impl Drop for SidecarProcess {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
