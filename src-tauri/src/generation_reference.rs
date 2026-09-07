//! 公開済みの不変リグ世代を、読了まで Windows の byte 0 lease で保持する。
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct Reference {
    schema_version: u32,
    generation: String,
    previous: Option<String>,
    rig_sha256: String,
    completion_sha256: String,
}

fn valid_id(value: &str) -> bool {
    value.len() == 34
        && value.starts_with("g_")
        && value[2..]
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

pub fn present(root: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(root.join("rig-current.json")) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

fn reference(root: &Path) -> io::Result<Reference> {
    let path = root.join("rig-current.json");
    crate::recovery::plain(&path)?;
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(1025).read_to_end(&mut bytes)?;
    if bytes.len() > 1024 {
        return Err(io::Error::other("公開参照の容量が不正です"));
    }
    let value: Reference = serde_json::from_slice(&bytes)?;
    if value.schema_version != 1
        || !valid_id(&value.generation)
        || value.previous.as_ref().is_some_and(|v| !valid_id(v))
        || [&value.rig_sha256, &value.completion_sha256]
            .iter()
            .any(|v| {
                v.len() != 64
                    || !v
                        .bytes()
                        .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            })
    {
        return Err(io::Error::other("公開参照が不正です"));
    }
    Ok(value)
}

fn catalog(root: &Path) -> io::Result<fs::File> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match crate::recovery::lock_byte(&root.join("rig-catalog.lock")) {
            Ok(file) => return Ok(file),
            Err(error) if error.raw_os_error() == Some(33) && Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(20))
            }
            Err(error) => return Err(error),
        }
    }
}

pub struct Reader {
    pub generation: String,
    pub directory: PathBuf,
    pub outputs: std::collections::BTreeMap<String, String>,
    root: PathBuf,
    lease: PathBuf,
    handle: Option<fs::File>,
}

impl Reader {
    pub fn acquire(root: &Path, metadata_limit: u64) -> io::Result<Self> {
        let _catalog = catalog(root)?;
        let selected = reference(root)?;
        let directory = root.join("rig-generations").join(&selected.generation);
        crate::recovery::plain(&directory)?;
        let path = directory.join("rig.json");
        crate::recovery::plain(&path)?;
        let mut file = fs::File::open(path)?;
        let mut digest = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            digest.update(&buffer[..count]);
        }
        if format!("{:x}", digest.finalize()) != selected.rig_sha256 {
            return Err(io::Error::other("公開リグが改変されています"));
        }
        let completion_path = directory.join("completion.json");
        crate::recovery::plain(&completion_path)?;
        let mut metadata = Vec::new();
        fs::File::open(completion_path)?
            .take(metadata_limit.saturating_add(1))
            .read_to_end(&mut metadata)?;
        if metadata.len() as u64 > metadata_limit
            || format!("{:x}", Sha256::digest(&metadata)) != selected.completion_sha256
        {
            return Err(io::Error::other(
                "公開補完証跡が改変されているか容量超過です",
            ));
        }
        #[derive(Deserialize)]
        struct Completion {
            outputs: std::collections::BTreeMap<String, String>,
        }
        let completion: Completion = serde_json::from_slice(&metadata)?;
        let leases = root.join("rig-leases");
        fs::create_dir_all(&leases)?;
        crate::recovery::plain(&leases)?;
        // tempfile は排他的に作成する。名前自体をプロトコルの固定形式へ変換する。
        let temporary = tempfile::NamedTempFile::new_in(&leases)?;
        let name = format!(
            "l_{:x}.lease",
            Sha256::digest(temporary.path().as_os_str().to_string_lossy().as_bytes())
        );
        let name = format!("{}.lease", &name[..34]);
        let lease = leases.join(name);
        let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&lease)?;
        let result = (|| {
            file.write_all(format!("0{}", selected.generation).as_bytes())?;
            file.sync_all()?;
            drop(file);
            crate::recovery::lock_byte(&lease)
        })();
        let handle = match result {
            Ok(file) => file,
            Err(error) => {
                fs::remove_file(&lease)?;
                return Err(error);
            }
        };
        Ok(Self {
            generation: selected.generation,
            directory,
            outputs: completion.outputs,
            root: root.to_path_buf(),
            lease,
            handle: Some(handle),
        })
    }

    pub fn finish(mut self) -> io::Result<()> {
        let guard = catalog(&self.root)?;
        self.handle.take();
        fs::remove_file(&self.lease)?;
        drop(guard);
        collect(&self.root)
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        // 失敗・中断でも OS ハンドルを閉じる。残った lease は次の collect で回収する。
        self.handle.take();
    }
}

pub fn collect(root: &Path) -> io::Result<()> {
    use std::io::{Seek, SeekFrom};
    let _catalog = catalog(root)?;
    let selected = reference(root)?;
    let mut protected = std::collections::BTreeSet::from([selected.generation]);
    protected.extend(selected.previous);
    let leases = root.join("rig-leases");
    if leases.exists() {
        crate::recovery::plain(&leases)?;
        for entry in fs::read_dir(&leases)? {
            let path = entry?.path();
            crate::recovery::plain(&path)?;
            let name = path
                .file_name()
                .and_then(|v| v.to_str())
                .ok_or_else(|| io::Error::other("読者 lease の名前が不正です"))?;
            if !name.is_ascii()
                || !path.is_file()
                || name.len() != 40
                || !name.starts_with("l_")
                || !name.ends_with(".lease")
                || !name[2..34]
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            {
                return Err(io::Error::other("読者 lease の名前が不正です"));
            }
            let locked = match crate::recovery::lock_byte(&path) {
                Ok(file) => Some(file),
                Err(e) if e.raw_os_error() == Some(33) => None,
                Err(e) => return Err(e),
            };
            if let Some(file) = locked {
                // OSロックを取れた読者は終了済み。作成途中で死んだ空leaseも内容解釈せず回収する。
                drop(file);
                fs::remove_file(path)?;
                continue;
            }
            let mut file = fs::File::open(&path)?;
            file.seek(SeekFrom::Start(1))?;
            let mut data = String::new();
            file.take(35).read_to_string(&mut data)?;
            if !valid_id(&data) {
                return Err(io::Error::other("読者 lease の内容が不正です"));
            }
            protected.insert(data);
        }
    }
    let parent = root.join("rig-generations");
    crate::recovery::plain(&parent)?;
    fn check_tree(path: &Path) -> io::Result<()> {
        crate::recovery::plain(path)?;
        if path.is_dir() {
            for child in fs::read_dir(path)? {
                check_tree(&child?.path())?;
            }
        }
        Ok(())
    }
    for entry in fs::read_dir(&parent)? {
        let path = entry?.path();
        let name = path.file_name().and_then(|v| v.to_str()).unwrap_or("");
        if valid_id(name) && !protected.contains(name) {
            check_tree(&path)?;
            fs::remove_dir_all(path)?;
        }
    }
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn fixed_reader_survives_new_reference_and_collects_after_release() {
        let temp = Path::new(env!("CARGO_MANIFEST_DIR")).join("../temp/generation-reference");
        fs::create_dir_all(&temp).unwrap();
        let fixture = tempfile::tempdir_in(temp).unwrap();
        let root = fixture.path();
        let publish = |n: u32, previous: Option<String>| {
            let generation = format!("g_{n:032x}");
            let dir = root.join("rig-generations").join(&generation);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("rig.json"), b"{}").unwrap();
            fs::write(dir.join("completion.json"), b"{\"outputs\":{}}").unwrap();
            fs::write(root.join("rig-current.json"), serde_json::to_vec(&serde_json::json!({"schema_version":1,"generation":generation,"previous":previous,"rig_sha256":format!("{:x}",Sha256::digest(b"{}")),"completion_sha256":format!("{:x}",Sha256::digest(b"{\"outputs\":{}}"))})).unwrap()).unwrap();
            generation
        };
        let first = publish(1, None);
        fs::create_dir_all(root.join("rig-leases")).unwrap();
        for (index, data) in [b"".as_slice(), b"0g_12", b"invalid-content"]
            .iter()
            .enumerate()
        {
            fs::write(
                root.join("rig-leases")
                    .join(format!("l_{index:032x}.lease")),
                data,
            )
            .unwrap();
        }
        collect(root).unwrap();
        assert_eq!(fs::read_dir(root.join("rig-leases")).unwrap().count(), 0);
        let reader = Reader::acquire(root, 1024).unwrap();
        let second = publish(2, Some(first));
        publish(3, Some(second));
        collect(root).unwrap();
        assert!(reader.directory.exists());
        let held = reader.directory.clone();
        reader.finish().unwrap();
        assert!(!held.exists());
        assert_eq!(
            fs::read_dir(root.join("rig-generations")).unwrap().count(),
            2
        );
    }
}
