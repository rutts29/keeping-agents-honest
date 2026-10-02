//! Reconcile native replacement writes before restoring a shared file link.
use super::OverlayError;
use std::{fs, io::Write, os::unix::fs::symlink, path::Path};
mod files;
mod merge;

pub(super) fn repair(source: &Path, target: &Path) -> Result<(), OverlayError> {
    recover(source, target).map_err(|detail| OverlayError {
        code: "OVERLAY_SHARED_RECOVERY",
        detail: format!("{target:?}: {detail}; files retained, retry after resolving the conflict"),
    })?;
    eprintln!("kah repaired shared Codex file {target:?}");
    Ok(())
}

fn recover(source: &Path, target: &Path) -> Result<(), String> {
    let name = target.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if !matches!(
        name,
        "session_index.jsonl" | "history.jsonl" | "auth.json" | "models_cache.json"
    ) {
        return Err("unexpected regular file in a shared directory/instruction path".into());
    }
    // Serializes kah repairs across every overlay of this native home.
    let _lease = files::open_locked(
        &source.parent().unwrap().join(".kah-overlay-repair.lock"),
        true,
    )?;
    if fs::read_link(target).is_ok_and(|p| p == source) {
        return Ok(());
    }
    let local = files::Snapshot::read(target)?;
    if name == "models_cache.json" {
        // Disposable cache: retain the canonical cache; native Codex refetches it.
        return relink(source, target, &local);
    }
    if name == "auth.json" {
        return auth(source, target, &local);
    }
    let mut global = files::open_locked(source, true)?;
    append_merged(name, source, target, &local, &mut global)?;
    relink(source, target, &local)
}

fn append_merged(
    name: &str,
    source: &Path,
    target: &Path,
    local: &files::Snapshot,
    global: &mut fs::File,
) -> Result<(), String> {
    // Never replace native history's locked inode, nor rewrite the global index.
    // O_APPEND and one write per complete JSON record preserve concurrent appends.
    for _ in 0..3 {
        let original = files::Snapshot::from_file(source, global)?;
        let additions = if name == "session_index.jsonl" {
            merge::index(&original.bytes, &local.bytes)?
        } else {
            merge::history(&original.bytes, &local.bytes)?
        };
        original.unchanged(source)?;
        local.unchanged(target)?;
        if additions.is_empty() {
            return Ok(());
        }
        for line in additions {
            global.write_all(&line).map_err(files::error)?;
        }
        global.sync_all().map_err(files::error)?;
        original.same_inode(source)?;
        // The native index append doesn't lock across processes. Reconcile again
        // so an intervening newer timestamp cannot be shadowed by our append.
    }
    Err("concurrent index/history changes; retry when the writer finishes".into())
}

fn auth(source: &Path, target: &Path, local: &files::Snapshot) -> Result<(), String> {
    // Auth's native writer does not honor a cross-process file lock. Never merge
    // a fork while a native client can refresh either store.
    let global = files::Snapshot::read(source)?;
    let use_local = merge::auth(&global.bytes, &local.bytes)?;
    files::auth_quiescent(source, target, use_local)?;
    global.unchanged(source)?;
    local.unchanged(target)?;
    if use_local {
        super::storage::write_private(source, &local.bytes)
            .map_err(|_| "atomic auth replacement failed")?;
        fs::File::open(source.parent().unwrap())
            .and_then(|f| f.sync_all())
            .map_err(files::error)?;
    }
    relink(source, target, local)
}

fn relink(source: &Path, target: &Path, local: &files::Snapshot) -> Result<(), String> {
    local.unchanged(target)?;
    let temporary = target.with_extension(format!("kah-link-{}", std::process::id()));
    symlink(source, &temporary).map_err(files::error)?;
    let result = fs::rename(&temporary, target);
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(files::error)?;
    fs::File::open(target.parent().unwrap())
        .and_then(|f| f.sync_all())
        .map_err(files::error)
}
