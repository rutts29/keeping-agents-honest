use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::{Read, Seek},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};
const LIMIT: u64 = 32 * 1024 * 1024;

pub(super) fn error(e: std::io::Error) -> String {
    e.to_string()
}

pub(super) fn open_locked(path: &Path, create: bool) -> Result<File, String> {
    let file = OpenOptions::new()
        .read(true)
        .append(true)
        .create(create)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(error)?;
    validate(&file.metadata().map_err(error)?)?;
    file.try_lock()
        .map_err(|_| "file is busy; retry when its writer finishes")?;
    Ok(file)
}
fn validate(meta: &Metadata) -> Result<(), String> {
    if !meta.is_file() || meta.nlink() != 1 || meta.len() > LIMIT {
        return Err("expected an unaliased regular file no larger than 32 MiB".into());
    }
    Ok(())
}

pub(super) struct Snapshot {
    pub bytes: Vec<u8>,
    metadata: Metadata,
    _file: Option<File>,
}
impl Snapshot {
    pub fn read(path: &Path) -> Result<Self, String> {
        let mut file = open_locked(path, false)?;
        let mut snapshot = Self::from_file(path, &mut file)?;
        snapshot._file = Some(file);
        Ok(snapshot)
    }
    pub fn from_file(path: &Path, file: &mut File) -> Result<Self, String> {
        let metadata = file.metadata().map_err(error)?;
        validate(&metadata)?;
        file.rewind().map_err(error)?;
        let mut bytes = vec![];
        file.take(LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(error)?;
        if bytes.len() as u64 > LIMIT {
            return Err("file exceeded 32 MiB".into());
        }
        let snapshot = Self {
            bytes,
            metadata,
            _file: None,
        };
        snapshot.unchanged(path)?;
        Ok(snapshot)
    }
    pub fn same_inode(&self, path: &Path) -> Result<(), String> {
        let now = fs::symlink_metadata(path).map_err(error)?;
        if now.dev() != self.metadata.dev() || now.ino() != self.metadata.ino() {
            return Err("native writer replaced the file during repair".into());
        }
        Ok(())
    }
    pub fn unchanged(&self, path: &Path) -> Result<(), String> {
        self.same_inode(path)?;
        let now = fs::symlink_metadata(path).map_err(error)?;
        if now.len() != self.metadata.len()
            || now.mtime() != self.metadata.mtime()
            || now.mtime_nsec() != self.metadata.mtime_nsec()
            || now.ctime() != self.metadata.ctime()
            || now.ctime_nsec() != self.metadata.ctime_nsec()
        {
            return Err("native writer changed the file during repair".into());
        }
        Ok(())
    }
}

pub(super) fn auth_quiescent(
    source: &Path,
    target: &Path,
    replacing_global: bool,
) -> Result<(), String> {
    for entry in fs::read_dir("/proc").map_err(error)?.flatten() {
        let path = entry.path();
        let Ok(exe) = fs::read_link(path.join("exe")) else {
            continue;
        };
        if !exe
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("codex"))
        {
            continue;
        }
        let Ok(env) = fs::read(path.join("environ")) else {
            continue;
        };
        let vars = env.split(|b| *b == 0).collect::<Vec<_>>();
        let value = |key: &[u8]| {
            vars.iter()
                .find_map(|v| v.strip_prefix(key))
                .map(|v| String::from_utf8_lossy(v).into_owned())
        };
        let home = value(b"CODEX_HOME=").or_else(|| value(b"HOME=").map(|h| format!("{h}/.codex")));
        let Some(home) = home else { continue };
        let auth = Path::new(&home).join("auth.json");
        if auth == target
            || (replacing_global
                && (auth == source || fs::read_link(&auth).is_ok_and(|p| p == source)))
        {
            return Err("auth stores may be active; close native Codex clients using these stores before retrying".into());
        }
    }
    Ok(())
}
