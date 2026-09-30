use super::*;
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
};
pub(super) struct Output {
    pub directory: PathBuf,
    _lock: File,
}
impl Output {
    pub fn open(dataset: &Path, digest: &str, validation: bool) -> Result<Self, String> {
        let split = if validation {
            "validation"
        } else {
            "development"
        };
        let directory = dataset
            .parent()
            .ok_or("EVAL_DATASET_PARENT stage=replay")?
            .join("results")
            .join(format!(
                "{}-q{}-{split}",
                &digest[..16],
                judge::questions::VERSION
            ));
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&directory)
            .map_err(io_error)?;
        let meta = fs::symlink_metadata(&directory).map_err(io_error)?;
        if !meta.is_dir() || meta.permissions().mode() & 0o077 != 0 {
            return Err("EVAL_PRIVATE_DIRECTORY stage=replay".into());
        }
        let lock = private_file(&directory.join("lock"), false)?;
        lock.try_lock().map_err(|_| "EVAL_BUSY stage=replay")?;
        Ok(Self {
            directory,
            _lock: lock,
        })
    }
    pub fn cache(&self) -> Result<BTreeMap<String, Record>, String> {
        let path = self.directory.join("answers.jsonl");
        reject_symlink(&path)?;
        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
            Err(e) => return Err(io_error(e)),
        };
        let mut cache = BTreeMap::new();
        for line in BufReader::new(file).lines() {
            let record: Record = serde_json::from_str(&line.map_err(io_error)?)
                .map_err(|_| "EVAL_CACHE_JSON stage=replay")?;
            if cache.insert(record.id.clone(), record).is_some() {
                return Err("EVAL_CACHE_DUPLICATE stage=replay".into());
            }
        }
        Ok(cache)
    }
    pub fn append(&self, record: &Record) -> Result<(), String> {
        let mut file = private_file(&self.directory.join("answers.jsonl"), true)?;
        writeln!(
            file,
            "{}",
            serde_json::to_string(record).map_err(|_| "EVAL_JSON stage=replay")?
        )
        .map_err(io_error)?;
        file.sync_all().map_err(io_error)
    }
    pub fn write(&self, name: &str, value: &Value) -> Result<(), String> {
        let path = self.directory.join(name);
        let temporary = self.directory.join(format!("{name}.tmp"));
        reject_symlink(&path)?;
        let mut file = private_file(&temporary, false)?;
        file.set_len(0).map_err(io_error)?;
        file.write_all(
            serde_json::to_string_pretty(value)
                .map_err(|_| "EVAL_JSON stage=replay")?
                .as_bytes(),
        )
        .map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        fs::rename(temporary, path).map_err(io_error)
    }
}
fn private_file(path: &Path, append: bool) -> Result<File, String> {
    reject_symlink(path)?;
    OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .append(append)
        .truncate(false)
        .mode(0o600)
        .open(path)
        .map_err(io_error)
}
fn reject_symlink(path: &Path) -> Result<(), String> {
    if fs::symlink_metadata(path).is_ok_and(|m| {
        m.file_type().is_symlink() || !m.is_file() || m.permissions().mode() & 0o077 != 0
    }) {
        return Err("EVAL_PRIVATE_FILE stage=replay".into());
    }
    Ok(())
}
pub(super) fn io_error(e: std::io::Error) -> String {
    format!("EVAL_IO stage=replay: {e}")
}
