//! Publish only explicitly installed user skill trees, never their parent home.
use crate::{Error, current_uid};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

type Files = BTreeMap<PathBuf, (Vec<u8>, bool)>;

pub fn publish_skills(home: &Path, cache: &Path) -> Result<Vec<(PathBuf, PathBuf)>, Error> {
    crate::runtime::directory(cache, current_uid()?)?;
    let source = home.join(".agents/skills");
    if !source.exists() {
        return Ok(vec![]);
    }
    let mut mounts = BTreeMap::new();
    for entry in fs::read_dir(&source).map_err(io_error)? {
        let alias = entry.map_err(io_error)?.path();
        if !alias.join("SKILL.md").is_file() {
            continue;
        }
        let root = alias.canonicalize().map_err(io_error)?;
        let mut files = Files::new();
        collect(&root, &root, &mut files)?;
        let mut digest = Sha256::new();
        for (path, (bytes, executable)) in &files {
            digest.update(path.as_os_str().as_encoded_bytes());
            digest.update([0, *executable as u8]);
            digest.update((bytes.len() as u64).to_le_bytes());
            digest.update(bytes);
        }
        let version = cache.join(format!("{:x}", digest.finalize()));
        publish(&version, &files)?;
        mounts.insert(root, version.clone());
        mounts.insert(alias, version);
    }
    Ok(mounts
        .into_iter()
        .map(|(target, source)| (source, target))
        .collect())
}

fn collect(root: &Path, directory: &Path, files: &mut Files) -> Result<(), Error> {
    for entry in fs::read_dir(directory).map_err(io_error)? {
        let path = entry.map_err(io_error)?.path();
        let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
        if metadata.is_dir() {
            collect(root, &path, files)?;
        } else if metadata.is_file() {
            if metadata.len() > 8 * 1024 * 1024 || files.len() >= 1000 {
                return Err(Error::new(
                    "SKILL_SIZE",
                    "skill_publish",
                    "skill tree exceeds bounded snapshot size",
                ));
            }
            files.insert(
                path.strip_prefix(root).unwrap().into(),
                (
                    fs::read(&path).map_err(io_error)?,
                    metadata.permissions().mode() & 0o111 != 0,
                ),
            );
        } else {
            return Err(Error::new(
                "SKILL_FILE_TYPE",
                "skill_publish",
                "skill contents must be regular files/directories; nested links are not published",
            ));
        }
    }
    Ok(())
}

fn publish(version: &Path, files: &Files) -> Result<(), Error> {
    crate::runtime::directory(version, current_uid()?)?;
    for (relative, (bytes, executable)) in files {
        let path = version.join(relative);
        let mut directory = version.to_path_buf();
        for part in relative.parent().unwrap().components() {
            directory.push(part);
            crate::runtime::directory(&directory, current_uid()?)?;
        }
        let mode = if *executable { 0o555 } else { 0o444 };
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(&path)
        {
            Ok(mut file) => {
                file.write_all(bytes)
                    .and_then(|()| file.set_permissions(fs::Permissions::from_mode(mode)))
                    .and_then(|()| file.sync_all())
                    .map_err(io_error)?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(io_error(e)),
        }
        let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
        if !metadata.is_file()
            || metadata.permissions().mode() & 0o777 != mode
            || fs::read(&path).map_err(io_error)? != *bytes
        {
            return Err(Error::new(
                "SKILL_SNAPSHOT_CHANGED",
                "skill_publish",
                "cached skill file differs from its immutable source snapshot",
            ));
        }
    }
    Ok(())
}
fn io_error(error: std::io::Error) -> Error {
    Error::new("SKILL_IO", "skill_publish", error)
}
