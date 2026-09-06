//! 最終リグの切替中断を復旧する。未完成pendingの採用や工程状態の変更はしない。
use std::{fs, io, path::Path};

fn plain(path: &Path) -> io::Result<()> {
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor)?;
        let mut linked = metadata.file_type().is_symlink();
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            linked |= metadata.file_attributes() & 0x400 != 0;
        }
        if linked {
            return Err(io::Error::other("復旧対象にリンクを含められません"));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn lock_byte(path: &Path) -> io::Result<fs::File> {
    use std::{
        io::{Seek, SeekFrom, Write},
        os::windows::io::AsRawHandle,
    };
    use windows_sys::Win32::Storage::FileSystem::LockFile;
    plain(
        path.parent()
            .ok_or_else(|| io::Error::other("ロックの親がありません"))?,
    )?;
    match fs::symlink_metadata(path) {
        Ok(_) => plain(path)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    if file.metadata()?.len() == 0 {
        file.seek(SeekFrom::Start(0))?;
        file.write_all(b"0")?;
        file.sync_all()?;
    }
    // Python msvcrt.locking(...,1)と同じbyte0を非待機でロックする。closeで解除される。
    if unsafe { LockFile(file.as_raw_handle(), 0, 0, 1, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(file)
}

#[cfg(not(windows))]
fn lock_byte(_path: &Path) -> io::Result<fs::File> {
    Err(io::Error::other("この環境の起動復旧ロックは未対応です"))
}

fn restore(character: &Path) -> io::Result<bool> {
    plain(character)?;
    let previous = character.join("temp/rig2d-previous");
    if !previous.exists() {
        return Ok(false);
    }
    plain(&previous)?;
    let _publication = lock_byte(&character.join("temp/rig2d.lock"))?;
    let destination = character.join("rig2d");
    if destination.exists() {
        plain(&destination)?;
        return Ok(false);
    }
    // pendingは完成と断定できない。公開前の旧世代だけを元の位置へ戻す。
    if !previous.is_dir() || !previous.join("rig.json").is_file() {
        return Err(io::Error::other(
            "退避リグが不完全です。自動採用せず保持します",
        ));
    }
    plain(&previous.join("rig.json"))?;
    fs::rename(previous, destination)?;
    Ok(true)
}

pub fn recover_final_rigs(root: &Path) -> Vec<String> {
    let mut warnings = Vec::new();
    if !root.exists() {
        return warnings;
    }
    let attempt = || -> io::Result<Vec<String>> {
        plain(root)?;
        let entries = fs::read_dir(root)?.collect::<Result<Vec<_>, _>>()?;
        let mut candidates = Vec::new();
        for entry in entries {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("c_")
                && name
                    .bytes()
                    .all(|v| v.is_ascii_lowercase() || v.is_ascii_digit() || v == b'_')
            {
                let path = entry.path();
                if path.join("temp/rig2d-previous").exists() {
                    candidates.push(path);
                }
            }
        }
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        let workspace = root.join("temp");
        if !workspace.exists() {
            fs::create_dir(&workspace)?;
        }
        // 別プロセスの補完推論から公開までを妨げない。稼働中なら復旧せず案内する。
        let _generation = lock_byte(&workspace.join("completion-gpu.lock"))?;
        let mut messages = Vec::new();
        for path in candidates {
            match restore(&path) {
                Ok(true)=>messages.push(format!("{}: 切替中断の旧リグを復旧しました。工程状態は変更していないため、必要なら局所補完を再実行してください",path.display())),
                Ok(false)=>{},
                Err(error)=>messages.push(format!("{}: 自動復旧せず保持しました: {error}",path.display())),
            }
        }
        Ok(messages)
    };
    match attempt() { Ok(messages)=>warnings.extend(messages),Err(error)=>warnings.push(format!("リグの起動復旧を実行できませんでした。他プロセス稼働中なら終了後に再起動してください: {error}")) }
    warnings
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    fn fixture() -> tempfile::TempDir {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../temp/tests");
        fs::create_dir_all(&root).unwrap();
        let dir = tempfile::tempdir_in(root).unwrap();
        fs::create_dir_all(dir.path().join("c_fixture/temp/rig2d-previous")).unwrap();
        fs::write(
            dir.path().join("c_fixture/temp/rig2d-previous/rig.json"),
            b"old",
        )
        .unwrap();
        dir
    }
    #[test]
    fn restores_only_previous_without_changing_stage_or_pending() {
        let dir = fixture();
        let character = dir.path().join("c_fixture");
        fs::write(character.join("character.json"), b"running").unwrap();
        fs::create_dir(character.join("temp/rig2d-pending")).unwrap();
        assert_eq!(recover_final_rigs(dir.path()).len(), 1);
        assert_eq!(fs::read(character.join("rig2d/rig.json")).unwrap(), b"old");
        assert_eq!(
            fs::read(character.join("character.json")).unwrap(),
            b"running"
        );
        assert!(character.join("temp/rig2d-pending").exists());
        assert!(recover_final_rigs(dir.path()).is_empty());
    }
    #[test]
    fn running_writer_and_second_recovery_cannot_steal_lock() {
        let dir = fixture();
        fs::create_dir(dir.path().join("temp")).unwrap();
        let path = dir.path().join("temp/completion-gpu.lock");
        let held = lock_byte(&path).unwrap();
        assert!(lock_byte(&path).is_err());
        assert_eq!(recover_final_rigs(dir.path()).len(), 1);
        assert!(!dir.path().join("c_fixture/rig2d").exists());
        drop(held);
        assert_eq!(recover_final_rigs(dir.path()).len(), 1);
    }
    #[test]
    fn publication_lock_prevents_restore() {
        let dir = fixture();
        let held = lock_byte(&dir.path().join("c_fixture/temp/rig2d.lock")).unwrap();
        assert_eq!(recover_final_rigs(dir.path()).len(), 1);
        assert!(!dir.path().join("c_fixture/rig2d").exists());
        drop(held);
    }
    #[test]
    fn python_msvcrt_byte_zero_blocks_rust_recovery() {
        use std::{
            io::{BufRead, BufReader, Write},
            process::{Command, Stdio},
        };
        struct Child(std::process::Child);
        impl Drop for Child {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let dir = fixture();
        fs::create_dir(dir.path().join("temp")).unwrap();
        let path = dir.path().join("temp/completion-gpu.lock");
        let python =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../sidecar/.venv/Scripts/python.exe");
        let mut child=Child(Command::new(python).args(["-u","-c",
            "import sys,msvcrt; f=open(sys.argv[1],'a+b'); f.write(b'0'); f.flush(); f.seek(0); msvcrt.locking(f.fileno(),msvcrt.LK_NBLCK,1); print('locked'); sys.stdin.readline(); f.close()"])
            .arg(&path).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap());
        let mut line = String::new();
        BufReader::new(child.0.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert_eq!(line.trim(), "locked");
        assert!(lock_byte(&path).is_err());
        assert_eq!(recover_final_rigs(dir.path()).len(), 1);
        assert!(!dir.path().join("c_fixture/rig2d").exists());
        child.0.stdin.take().unwrap().write_all(b"\n").unwrap();
        assert!(child.0.wait().unwrap().success());
        assert_eq!(recover_final_rigs(dir.path()).len(), 1);
        assert!(dir.path().join("c_fixture/rig2d/rig.json").exists());
    }
}
