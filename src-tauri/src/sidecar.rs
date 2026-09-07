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
    #[cfg(windows)]
    job: Option<std::os::windows::io::OwnedHandle>,
}

impl SidecarProcess {
    pub fn spawn(
        program: &Path,
        arguments: &[OsString],
        working_directory: &Path,
    ) -> Result<Self, SidecarError> {
        let mut command = Command::new(program);
        command
            .args(arguments)
            .current_dir(working_directory)
            // JSON Lines はOSのコードページに依存させず、常にUTF-8で受け渡す。
            .env("PYTHONUTF8", "1")
            .env("PYTHONIOENCODING", "utf-8")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED};
            command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
        }
        let child = command.spawn().map_err(SidecarError::Spawn)?;
        let mut process = Self {
            child: Some(child),
            #[cfg(windows)]
            job: None,
        };
        #[cfg(windows)]
        {
            // 停止状態で所属させ、孫プロセスがJobの外へ先行することを防ぐ。
            process.job = Some(attach_job(
                process.child.as_ref().expect("起動済みプロセスが必要です"),
            )?);
            resume_owned_thread(
                process
                    .child
                    .as_ref()
                    .expect("起動済みプロセスが必要です")
                    .id(),
            )?;
        }
        Ok(process)
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
        let read_result = (|| -> Result<(), SidecarError> {
            loop {
                if cancelled.load(Ordering::Relaxed) {
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
            Ok(())
        })();
        if read_result.is_err() {
            #[cfg(windows)]
            self.job.take();
            // 既に終了した場合のkill失敗でもwaitと読取スレッド回収を省略しない。
            let _ = child.kill();
        }
        let status = child.wait();
        // 親が正常終了しても、pipeを継承した孫を先に回収して読取待ちを解く。
        #[cfg(windows)]
        self.job.take();
        let _ = reader.join();
        let error_result = error_reader.join().map_err(|_| {
            SidecarError::Io(std::io::Error::other("stderr読取スレッドが停止しました"))
        });
        self.child = None;
        read_result?;
        let status = status?;
        let error_text = error_result??;
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
        #[cfg(windows)]
        self.job.take();
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(windows)]
fn attach_job(child: &Child) -> std::io::Result<std::os::windows::io::OwnedHandle> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::System::JobObjects::*;
    // 無名・非継承のハンドルを唯一保持する。失敗経路もOwnedHandleで閉じる。
    unsafe {
        let raw = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if raw.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let job = OwnedHandle::from_raw_handle(raw);
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            raw,
            JobObjectExtendedLimitInformation,
            (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of_val(&info) as u32,
        ) == 0
            || AssignProcessToJobObject(raw, child.as_raw_handle()) == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        Ok(job)
    }
}

#[cfg(windows)]
fn resume_owned_thread(pid: u32) -> std::io::Result<()> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::{
        Foundation::INVALID_HANDLE_VALUE,
        System::{Diagnostics::ToolHelp::*, Threading::*},
    };
    // 停止中の自分のChildのPIDだけを対象にする。名前による列挙・終了は行わない。
    unsafe {
        let raw = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if raw == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error());
        }
        let snapshot = OwnedHandle::from_raw_handle(raw);
        let mut entry: THREADENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of_val(&entry) as u32;
        let mut found = Thread32First(snapshot.as_raw_handle(), &mut entry);
        while found != 0 {
            if entry.th32OwnerProcessID == pid {
                let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                if thread.is_null() {
                    return Err(std::io::Error::last_os_error());
                }
                let owned = OwnedHandle::from_raw_handle(thread);
                if ResumeThread(owned.as_raw_handle()) == u32::MAX {
                    return Err(std::io::Error::last_os_error());
                }
                return Ok(());
            }
            found = Thread32Next(snapshot.as_raw_handle(), &mut entry);
        }
        Err(std::io::Error::other(
            "起動したサイドカーの初期スレッドが見つかりません",
        ))
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::{
        Foundation::WAIT_OBJECT_0,
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };

    fn python() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../sidecar/.venv/Scripts/python.exe")
    }

    fn open_owned_test_child(value: &Value) -> OwnedHandle {
        let pid = value["pid"].as_u64().expect("テストの子PID") as u32;
        // テストが起動したPythonから返された子だけを待機し、操作対象を広げない。
        unsafe {
            let raw = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
            assert!(!raw.is_null(), "{}", std::io::Error::last_os_error());
            OwnedHandle::from_raw_handle(raw)
        }
    }

    fn assert_terminated(handle: &OwnedHandle) {
        unsafe {
            assert_eq!(
                WaitForSingleObject(handle.as_raw_handle(), 5_000),
                WAIT_OBJECT_0
            );
        }
    }

    fn spawn_tree(ending: &str) -> SidecarProcess {
        let script = format!(
            "import subprocess,sys,json,time; p=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)']); print(json.dumps({{'pid':p.pid,'text':'日本語'}}),flush=True); time.sleep(.2); {ending}"
        );
        SidecarProcess::spawn(
            &python(),
            &["-c".into(), script.into()],
            Path::new(env!("CARGO_MANIFEST_DIR")),
        )
        .unwrap()
    }

    #[test]
    fn job_reclaims_descendants_on_normal_exit_cancel_and_bad_json() {
        for ending in [
            "sys.exit(0)",
            "time.sleep(30)",
            "print('invalid-json',flush=True); time.sleep(30)",
        ] {
            let cancelled = Arc::new(AtomicBool::new(false));
            let mut descendant = None;
            let result = spawn_tree(ending).relay_json_lines(cancelled.clone(), |value| {
                assert_eq!(value["text"], "日本語");
                descendant = Some(open_owned_test_child(&value));
                if ending == "time.sleep(30)" {
                    cancelled.store(true, Ordering::Relaxed);
                }
            });
            if ending == "sys.exit(0)" {
                assert!(result.is_ok(), "{result:?}");
            } else if ending == "time.sleep(30)" {
                assert!(matches!(result, Err(SidecarError::Cancelled)));
            } else {
                assert!(matches!(result, Err(SidecarError::Json(_))));
            }
            assert_terminated(&descendant.expect("子生成の通知"));
        }
    }

    #[test]
    fn job_reclaims_descendants_when_guard_is_dropped() {
        let mut process = spawn_tree("time.sleep(30)");
        let stdout = process.child.as_mut().unwrap().stdout.take().unwrap();
        let (sender, receiver) = mpsc::channel();
        let reader = thread::spawn(move || {
            let mut line = String::new();
            let result = BufReader::new(stdout).read_line(&mut line).map(|_| line);
            let _ = sender.send(result);
        });
        let line = receiver
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
        let descendant = open_owned_test_child(&serde_json::from_str(&line).unwrap());
        drop(process);
        assert_terminated(&descendant);
        reader.join().unwrap();
    }
}
