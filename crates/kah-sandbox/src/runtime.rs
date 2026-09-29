use crate::Error;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, DirBuilder, File, OpenOptions},
    io::{self, Read},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn io_error(error: io::Error) -> Error {
    Error::new("RUNTIME_IO", "executor_runtime", error)
}

pub fn current_uid() -> Result<u32, Error> {
    let output = Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .map_err(io_error)?;
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .map_err(|e| Error::new("RUNTIME_UID", "executor_runtime", e))
}

/// Publish only the public native executable where the rootless daemon can read
/// it. Credentials and the user's private installation directory stay private.
pub fn publish_binary(source: &Path, cache: &Path) -> Result<PathBuf, Error> {
    let uid = current_uid()?;
    directory(cache, uid)?;
    let digest = hash(source)?;
    let version = cache.join(&digest);
    directory(&version, uid)?;
    let binary = version.join("codex");
    if !binary.try_exists().map_err(io_error)? {
        copy_new(source, &binary, &digest)?;
    }
    let metadata = fs::symlink_metadata(&binary).map_err(io_error)?;
    if !metadata.is_file() || metadata.uid() != uid || hash(&binary)? != digest {
        return Err(Error::new(
            "RUNTIME_HASH",
            "executor_runtime",
            "cached executable does not match installed Codex",
        ));
    }
    if metadata.mode() & 0o777 != 0o555 {
        return Err(Error::new(
            "RUNTIME_MODE",
            "executor_runtime",
            "cached executable must be read-only and executable",
        ));
    }
    Ok(binary)
}

fn directory(path: &Path, uid: u32) -> Result<(), Error> {
    match DirBuilder::new().mode(0o755).create(path) {
        Ok(()) => (),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(io_error(e)),
    }
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if !metadata.is_dir() || metadata.uid() != uid || metadata.mode() & 0o022 != 0 {
        return Err(Error::new(
            "RUNTIME_DIRECTORY",
            "executor_runtime",
            "runtime directory must be owned by the operator, with no group/other write access",
        ));
    }
    Ok(())
}

fn hash(path: &Path) -> Result<String, Error> {
    let mut file = File::open(path).map_err(io_error)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(io_error)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn copy_new(source: &Path, target: &Path, digest: &str) -> Result<(), Error> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = target.with_extension(format!("{}-{nonce}", std::process::id()));
    let result = (|| {
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o500)
            .open(&temporary)
            .map_err(io_error)?;
        io::copy(&mut File::open(source).map_err(io_error)?, &mut output).map_err(io_error)?;
        output.sync_all().map_err(io_error)?;
        if hash(&temporary)? != digest {
            return Err(Error::new(
                "RUNTIME_SOURCE_CHANGED",
                "executor_runtime",
                "Codex changed during copy; launch again",
            ));
        }
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o555)).map_err(io_error)?;
        match fs::hard_link(&temporary, target) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(io_error(e)),
        }
    })();
    let _ = fs::remove_file(&temporary);
    result
}
