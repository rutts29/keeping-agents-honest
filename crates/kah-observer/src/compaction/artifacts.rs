use super::*;

pub(super) fn parse_trajectory(stdout: &[u8]) -> Result<(Vec<Value>, ResolverUsage), ()> {
    let text = std::str::from_utf8(stdout).map_err(|_| ())?;
    let mut events = Vec::new();
    let mut usage = ResolverUsage::default();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let event: Value = serde_json::from_str(line).map_err(|_| ())?;
        if event.get("type").and_then(Value::as_str) == Some("turn.completed") {
            let observed = &event["usage"];
            usage.input_tokens = observed["input_tokens"].as_u64().unwrap_or(0);
            usage.cached_input_tokens = observed["cached_input_tokens"].as_u64().unwrap_or(0);
            usage.output_tokens = observed["output_tokens"].as_u64().unwrap_or(0);
            usage.reasoning_output_tokens =
                observed["reasoning_output_tokens"].as_u64().unwrap_or(0);
        }
        events.push(event);
    }
    Ok((events, usage))
}

pub(super) struct TemporaryCallFiles {
    pub(super) directory: PathBuf,
    pub(super) schema: PathBuf,
    pub(super) final_message: PathBuf,
}

pub(super) struct ArtifactStore {
    pub(super) directory: PathBuf,
}

impl ArtifactStore {
    pub(super) fn open(source_root: &Path) -> Result<Self, ()> {
        let configured = env::var_os("KAH_ARTIFACT_DIR").map(PathBuf::from);
        let directory = configured
            .or_else(|| {
                env::var_os("XDG_STATE_HOME")
                    .map(PathBuf::from)
                    .map(|path| path.join("kah/observer-artifacts"))
            })
            .or_else(|| {
                env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|path| path.join(".local/state/kah/observer-artifacts"))
            })
            .ok_or(())?;
        if !directory.is_absolute() {
            return Err(());
        }
        fs::create_dir_all(&directory).map_err(|_| ())?;
        let metadata = fs::symlink_metadata(&directory).map_err(|_| ())?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(());
        }
        set_private_directory_permissions(&directory)?;
        let canonical = directory.canonicalize().map_err(|_| ())?;
        if canonical.starts_with(source_root) {
            return Err(());
        }
        Ok(Self {
            directory: canonical,
        })
    }

    pub(super) fn retain(&self, trajectory: &[u8], final_message: &[u8]) -> Result<String, ()> {
        let artifact_id = artifact_id()?;
        self.write_private(
            &self
                .directory
                .join(format!("{artifact_id}.trajectory.jsonl")),
            trajectory,
        )?;
        self.write_private(
            &self.directory.join(format!("{artifact_id}.final.json")),
            final_message,
        )?;
        Ok(artifact_id)
    }

    pub(super) fn write_private(&self, path: &Path, bytes: &[u8]) -> Result<(), ()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path).map_err(|_| ())?;
        file.write_all(bytes).map_err(|_| ())?;
        file.sync_all().map_err(|_| ())?;
        set_private_file_permissions(path)
    }
}

pub(super) fn artifact_id() -> Result<String, ()> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ())?
        .as_nanos();
    let sequence = ARTIFACT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut digest = Sha256::new();
    digest.update(now.to_le_bytes());
    digest.update(std::process::id().to_le_bytes());
    digest.update(sequence.to_le_bytes());
    let hex = format!("{:x}", digest.finalize());
    Ok(hex[..24].to_owned())
}

#[cfg(unix)]
pub(super) fn set_private_directory_permissions(path: &Path) -> Result<(), ()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|_| ())
}

#[cfg(not(unix))]
pub(super) fn set_private_directory_permissions(_path: &Path) -> Result<(), ()> {
    Ok(())
}

#[cfg(unix)]
pub(super) fn set_private_file_permissions(path: &Path) -> Result<(), ()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|_| ())
}

#[cfg(not(unix))]
pub(super) fn set_private_file_permissions(_path: &Path) -> Result<(), ()> {
    Ok(())
}

impl TemporaryCallFiles {
    pub(super) fn create() -> Result<Self, ()> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ())?
            .as_nanos();
        let directory =
            env::temp_dir().join(format!("kah-observer-{}-{nonce}", std::process::id()));
        create_private_directory(&directory)?;
        let files = Self {
            schema: directory.join("projection-proposal.schema.json"),
            final_message: directory.join("projection-proposal.json"),
            directory,
        };
        create_private_empty_file(&files.schema)?;
        create_private_empty_file(&files.final_message)?;
        Ok(files)
    }
}

pub(super) fn create_private_directory(path: &Path) -> Result<(), ()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).map_err(|_| ())?;
    set_private_directory_permissions(path)
}

pub(super) fn create_private_empty_file(path: &Path) -> Result<(), ()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(|_| ())?;
    set_private_file_permissions(path)
}

impl Drop for TemporaryCallFiles {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.schema);
        let _ = fs::remove_file(&self.final_message);
        let _ = fs::remove_dir(&self.directory);
    }
}
