//! Lossless content-addressed observer sources; references never choose content.
use crate::Result;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

mod segments;

fn directory(path: &Path) -> Result<PathBuf> {
    path.ancestors()
        .find(|p| p.join("sources").is_dir())
        .map(|p| p.join("sources"))
        .ok_or_else(|| "REVIEW_SOURCE_DIRECTORY stage=source_store".into())
}
fn reference(root: &Path, bytes: &[u8], encoding: &str) -> Result<Value> {
    let digest = format!("{:x}", Sha256::digest(bytes));
    if !root.join(&digest).exists() {
        segments::store(root, bytes, &digest)?;
    }
    verify(&root.join(&digest), &digest)?;
    Ok(json!({"$kah_source":digest,"encoding":encoding}))
}
fn write_once(target: &Path, bytes: &[u8]) -> Result<()> {
    if target.exists() {
        return Ok(());
    }
    let temporary = target.with_extension(format!("{}.tmp", crate::relay_store::nonce()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    match fs::hard_link(&temporary, target) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    }
    fs::remove_file(temporary)?;
    fs::File::open(target.parent().unwrap())?.sync_all()?;
    Ok(())
}
fn verify(path: &Path, digest: &str) -> Result<Vec<u8>> {
    // Nested delivery locks can create an empty sources directory. References
    // still identify immutable content in the containing review/workspace.
    // A present source must verify; never hide corruption using another copy.
    let resolved = if path.exists() {
        path.to_owned()
    } else {
        path.ancestors()
            .skip(1)
            .map(|parent| parent.join("sources").join(digest))
            .find(|candidate| candidate.is_file())
            .unwrap_or_else(|| path.to_owned())
    };
    segments::recover(&resolved, digest)
}
fn read_bytes(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(|e| {
        format!(
            "REVIEW_SOURCE_IO stage=source_store: {}: {e}",
            path.display()
        )
        .into()
    })
}

fn strings(value: &Value, root: &Path) -> Result<Value> {
    if value.get("$kah_source").is_some() {
        return Ok(value.clone());
    }
    match value {
        Value::String(text) => reference(root, text.as_bytes(), "utf8"),
        Value::Array(items) => Ok(Value::Array(
            items
                .iter()
                .map(|v| strings(v, root))
                .collect::<Result<_>>()?,
        )),
        Value::Object(items) => Ok(Value::Object(
            items
                .iter()
                .map(|(k, v)| Ok((k.clone(), strings(v, root)?)))
                .collect::<Result<_>>()?,
        )),
        _ => Ok(value.clone()),
    }
}
pub fn expand(value: Value, root: &Path) -> Result<Value> {
    if let Some(digest) = value.get("$kah_source").and_then(Value::as_str) {
        if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("REVIEW_SOURCE_ID stage=source_store".into());
        }
        let bytes = verify(&root.join(digest), digest)?;
        return match value["encoding"].as_str() {
            Some("utf8") => Ok(Value::String(String::from_utf8(bytes)?)),
            Some("json") => expand(serde_json::from_slice(&bytes)?, root),
            _ => Err("REVIEW_SOURCE_ENCODING stage=source_store".into()),
        };
    }
    match value {
        Value::Array(items) => Ok(Value::Array(
            items
                .into_iter()
                .map(|v| expand(v, root))
                .collect::<Result<_>>()?,
        )),
        Value::Object(items) => Ok(Value::Object(
            items
                .into_iter()
                .map(|(k, v)| Ok((k, expand(v, root)?)))
                .collect::<Result<_>>()?,
        )),
        _ => Ok(value),
    }
}
fn compact(value: &Value, root: &Path) -> Result<Value> {
    if value.get("$kah_source").is_some() {
        return Ok(value.clone());
    }
    let mut result = value.clone();
    if let Some(object) = result.as_object_mut() {
        for (key, source) in object.iter_mut() {
            if [
                "state",
                "original",
                "prepared_state",
                "jev_state",
                "trigger",
            ]
            .contains(&key.as_str())
            {
                if source.get("$kah_source").is_some() {
                    continue;
                }
                let encoded = strings(source, root)?;
                *source = reference(root, &serde_json::to_vec(&encoded)?, "json")?;
            } else if key != "documents" {
                *source = compact(source, root)?;
            }
        }
        if let Some(documents) = object.get_mut("documents").and_then(Value::as_array_mut) {
            for document in documents {
                if document.get("$kah_source").is_none() {
                    let encoded = strings(document, root)?;
                    *document = reference(root, &serde_json::to_vec(&encoded)?, "json")?;
                }
            }
        }
    } else if let Some(array) = result.as_array_mut() {
        for value in array {
            *value = compact(value, root)?;
        }
    }
    Ok(result)
}
pub fn save(directory: &Path, name: &str, value: &Value) -> Result<()> {
    let root = if let Some(parent) = directory
        .ancestors()
        .find(|p| p.join(".source-root").is_file())
    {
        PathBuf::from(fs::read_to_string(parent.join(".source-root"))?)
    } else {
        self::directory(directory)?
    };
    let stored = compact(value, &root)?;
    if expand(stored.clone(), &root)? != *value {
        return Err("REVIEW_SOURCE_ROUNDTRIP stage=source_store".into());
    }
    crate::unattended_report::save(directory, name, &stored)
}
pub fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let value = serde_json::from_slice(&read_bytes(path)?)?;
    let root = directory(path).unwrap_or_else(|_| path.parent().unwrap().join("sources"));
    let expanded = expand(value, &root)?;
    Ok(serde_json::from_value(expanded)?)
}
pub fn initialize(private: &Path, workspace: &Path) -> Result<()> {
    kah_core::run_store::ensure_private_directory(&private.join("sources"))?;
    fs::create_dir_all(workspace.join("sources"))?;
    fs::write(
        workspace.join(".source-root"),
        private.join("sources").as_os_str().as_encoded_bytes(),
    )?;
    Ok(())
}
pub fn text(private: &Path, target: &Path, text: &str) -> Result<()> {
    let root = private.join("sources");
    let source = reference(&root, text.as_bytes(), "utf8")?;
    crate::unattended_report::save(
        target.parent().unwrap(),
        target.file_name().unwrap().to_str().unwrap(),
        &source,
    )
}
pub fn load_text(path: &Path) -> Result<String> {
    let bytes = read_bytes(path)?;
    if let Ok(value) = serde_json::from_slice::<Value>(&bytes)
        && value.get("$kah_source").is_some()
    {
        let root = directory(path)?;
        return expand(value, &root)?
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "REVIEW_SOURCE_ENCODING stage=source_text".into());
    }
    Ok(String::from_utf8(bytes)?)
}
pub fn print_text(path: &Path) -> Result<()> {
    let bytes = read_bytes(path)?;
    let output = match serde_json::from_slice::<Value>(&bytes) {
        Ok(value) => {
            let value = expand(value, &directory(path)?)?;
            match value.as_str() {
                Some(text) => text.to_owned(),
                None => serde_json::to_string_pretty(&value)?,
            }
        }
        Err(_) => String::from_utf8(bytes)?,
    };
    std::io::stdout().write_all(output.as_bytes())?;
    Ok(())
}
/// Losslessly migrates digest-verified snapshots to shared storage blocks.
pub fn segment_sources(root: &Path) -> Result<usize> {
    segments::migrate(root)
}
/// Checks either a complete source or an individual binary storage block.
pub fn verify_blob(path: &Path) -> Result<()> {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("REVIEW_SOURCE_ID stage=source_store")?;
    let digest = name.strip_prefix("block-").unwrap_or(name);
    if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("REVIEW_SOURCE_ID stage=source_store".into());
    }
    if name.starts_with("block-") {
        if format!("{:x}", Sha256::digest(fs::read(path)?)) != digest {
            return Err("REVIEW_SOURCE_DIGEST stage=source_store".into());
        }
    } else {
        verify(path, digest)?;
    }
    Ok(())
}

pub fn share(private: &Path, workspace: &Path) -> Result<()> {
    let public = workspace.join("sources");
    for entry in fs::read_dir(private.join("sources"))? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().ends_with(".tmp") {
            continue;
        }
        let target = public.join(entry.file_name());
        if target.exists() && fs::metadata(&target)?.ino() != fs::metadata(entry.path())?.ino() {
            let temporary = target.with_extension(format!("{}.tmp", crate::relay_store::nonce()));
            fs::hard_link(entry.path(), &temporary)?;
            fs::rename(temporary, &target)?;
        }
        if !target.exists() {
            fs::hard_link(entry.path(), target)?;
        }
    }
    kah_run::access::share_tree(&public, kah_run::access::service_uid()?, false)?;
    Ok(())
}
pub fn migrate(worker: &str) -> Result<Value> {
    let store = super::state::Store::open(worker)?;
    let state = store
        .load()?
        .ok_or("REVIEW_STATE_MISSING stage=source_migration")?;
    initialize(&store.directory, &state.observer.workspace)?;
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("REVIEW_HOME stage=source_migration")?);
    super::workspace::prepare(&home, &state)?;
    let mut paths = Vec::new();
    collect(&store.directory, &mut paths)?;
    collect(&state.observer.workspace.join("inputs"), &mut paths)?;
    let mut migrated = 0;
    for path in paths {
        if path.extension().is_some_and(|e| e == "txt") {
            let full = load_text(&path)?;
            text(&store.directory, &path, &full)?;
            migrated += 1;
            continue;
        }
        let value: Value = serde_json::from_slice(&fs::read(&path)?)?;
        let root = store.directory.join("sources");
        let stored = compact(&value, &root)?;
        if stored != value {
            if expand(stored.clone(), &root)? != expand(value, &root)? {
                return Err("REVIEW_SOURCE_ROUNDTRIP stage=source_migration".into());
            }
            if path.starts_with(&state.observer.workspace) {
                share(&store.directory, &state.observer.workspace)?;
            }
            crate::unattended_report::save(
                path.parent().unwrap(),
                path.file_name().unwrap().to_str().unwrap(),
                &stored,
            )?;
            migrated += 1;
        }
    }
    let segmented = segment_sources(&store.directory.join("sources"))?;
    share(&store.directory, &state.observer.workspace)?;
    kah_run::access::share_tree(
        &state.observer.workspace.join("inputs"),
        kah_run::access::service_uid()?,
        false,
    )?;
    Ok(
        json!({"worker":worker,"recordsMigrated":migrated,"sourcesSegmented":segmented,"digestsVerified":true,"fullSourcesRecoverable":true}),
    )
}
fn collect(root: &Path, paths: &mut Vec<PathBuf>) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() && entry.file_name() != "sources" {
            collect(&path, paths)?;
        } else if path.extension().is_some_and(|e| e == "json" || e == "txt") {
            paths.push(path);
        }
    }
    Ok(())
}
